// ParamOverride Engine — JSON patch engine with conditional application
//
// Three scopes (loaded from system_settings):
//   "ParamOverride:global" — applied to ALL requests
//   "ParamOverride:type:{provider_type}" — applied to requests going to this type
//   "ParamOverride:provider:{provider_name}" — applied to requests going to this provider
//
// Applied in order: global -> type -> provider (later overrides earlier)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::database::Database;
use crate::database::dao::SystemSettingsDao;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideRule {
    pub path: String,
    pub mode: OverrideMode,
    pub value: Option<Value>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default = "default_logic")]
    pub logic: String,
}

fn default_logic() -> String {
    "AND".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideMode {
    Set,
    Remove,
    Append,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub path: String,
    pub mode: ConditionMode,
    pub value: Option<Value>,
    #[serde(default)]
    pub pass_missing_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionMode {
    Equals,
    NotEquals,
    Exists,
    NotExists,
    Contains,
}

/// Context for template variable substitution
pub struct OverrideContext<'a> {
    pub user_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub provider_name: &'a str,
}

pub struct ParamOverrideEngine;

impl ParamOverrideEngine {
    /// Apply all applicable overrides to a request body
    pub fn apply(
        body: &mut Value,
        provider_type: &str,
        provider_name: &str,
        db: &Database,
        template_ctx: Option<&OverrideContext<'_>>,
    ) {
        let global = Self::load_rules(db, "ParamOverride:global");
        let type_rules = Self::load_rules(db, &format!("ParamOverride:type:{}", provider_type));
        let provider_rules = Self::load_rules(db, &format!("ParamOverride:provider:{}", provider_name));

        for rule in global.iter().chain(type_rules.iter()).chain(provider_rules.iter()) {
            Self::apply_rule(body, rule, template_ctx);
        }
    }

    fn load_rules(db: &Database, key: &str) -> Vec<OverrideRule> {
        match SystemSettingsDao::get(db, key) {
            Ok(Some(setting)) => {
                // Value should be a JSON array of OverrideRule
                match serde_json::from_value::<Vec<OverrideRule>>(setting.value) {
                    Ok(rules) => rules,
                    Err(e) => {
                        tracing::warn!("[ParamOverride] Failed to parse rules for '{}': {}", key, e);
                        Vec::new()
                    }
                }
            }
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::warn!("[ParamOverride] Failed to load setting '{}': {}", key, e);
                Vec::new()
            }
        }
    }

    fn apply_rule(body: &mut Value, rule: &OverrideRule, template_ctx: Option<&OverrideContext<'_>>) {
        if !Self::check_conditions(body, &rule.conditions, &rule.logic) {
            return;
        }
        // Resolve template variables in value if present
        let resolved_value = rule.value.as_ref().map(|v| Self::resolve_templates(v, template_ctx));

        match rule.mode {
            OverrideMode::Set => Self::set_path(body, &rule.path, resolved_value),
            OverrideMode::Remove => Self::remove_path(body, &rule.path),
            OverrideMode::Append => Self::append_path(body, &rule.path, resolved_value),
            OverrideMode::Merge => Self::merge_path(body, &rule.path, resolved_value),
        }
    }

    fn check_conditions(body: &Value, conditions: &[Condition], logic: &str) -> bool {
        if conditions.is_empty() {
            return true;
        }

        let results = conditions.iter().map(|c| Self::check_single_condition(body, c));

        if logic == "OR" {
            results.into_iter().any(|r| r)
        } else {
            // Default AND
            results.into_iter().all(|r| r)
        }
    }

    fn check_single_condition(body: &Value, condition: &Condition) -> bool {
        let current = Self::get_path(body, &condition.path);

        match condition.mode {
            ConditionMode::Exists => current.is_some(),
            ConditionMode::NotExists => current.is_none(),
            ConditionMode::Equals => {
                match current {
                    Some(ref val) => condition.value.as_ref().map_or(false, |expected| val == expected),
                    None => condition.pass_missing_key,
                }
            }
            ConditionMode::NotEquals => {
                match current {
                    Some(ref val) => condition.value.as_ref().map_or(true, |expected| val != expected),
                    None => condition.pass_missing_key,
                }
            }
            ConditionMode::Contains => {
                match current {
                    Some(Value::Array(arr)) => {
                        condition.value.as_ref().map_or(false, |expected| arr.contains(expected))
                    }
                    Some(Value::String(s)) => {
                        condition.value.as_ref()
                            .and_then(|v| v.as_str())
                            .map_or(false, |expected| s.contains(expected))
                    }
                    None => condition.pass_missing_key,
                    _ => false,
                }
            }
        }
    }

    // ─── Path operations ─────────────────────────────────────────────────────

    fn set_path(body: &mut Value, path: &str, value: Option<Value>) {
        let Some(val) = value else { return };
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = body;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Final segment — set the value
                if let Some(obj) = current.as_object_mut() {
                    obj.insert(part.to_string(), val);
                }
                return;
            }
            // Navigate or create intermediate objects
            if !current.get(*part).is_some() {
                if let Some(obj) = current.as_object_mut() {
                    obj.insert(part.to_string(), Value::Object(serde_json::Map::new()));
                }
            }
            current = current.get_mut(*part).unwrap();
        }
    }

    fn remove_path(body: &mut Value, path: &str) {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return;
        }
        if parts.len() == 1 {
            if let Some(obj) = body.as_object_mut() {
                obj.remove(parts[0]);
            }
            return;
        }
        // Navigate to parent, then remove the last key
        let mut current = body;
        for part in &parts[..parts.len() - 1] {
            match current.get_mut(*part) {
                Some(next) => current = next,
                None => return,
            }
        }
        if let Some(obj) = current.as_object_mut() {
            obj.remove(parts.last().unwrap().to_owned());
        }
    }

    fn append_path(body: &mut Value, path: &str, value: Option<Value>) {
        let Some(val) = value else { return };
        // Navigate to target, append to array
        let target = Self::get_path_mut(body, path);
        match target {
            Some(Value::Array(arr)) => arr.push(val),
            Some(other) => {
                // Convert to array with old value + new value
                let old = other.clone();
                *other = Value::Array(vec![old, val]);
            }
            None => {
                // Create the path with a new array
                Self::set_path(body, path, Some(Value::Array(vec![val])));
            }
        }
    }

    fn merge_path(body: &mut Value, path: &str, value: Option<Value>) {
        let Some(Value::Object(merge_obj)) = value else { return };
        let target = Self::get_path_mut(body, path);
        match target {
            Some(Value::Object(existing)) => {
                for (k, v) in merge_obj {
                    existing.insert(k, v);
                }
            }
            Some(_) => {
                // Target exists but is not an object — overwrite
                // Navigate and set
                Self::set_path(body, path, Some(Value::Object(merge_obj)));
            }
            None => {
                Self::set_path(body, path, Some(Value::Object(merge_obj)));
            }
        }
    }

    fn get_path(body: &Value, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = body;
        for part in parts {
            match current.get(part) {
                Some(next) => current = next,
                None => return None,
            }
        }
        Some(current.clone())
    }

    fn get_path_mut<'a>(body: &'a mut Value, path: &str) -> Option<&'a mut Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = body;
        for part in parts {
            current = current.get_mut(part)?;
        }
        Some(current)
    }

    /// Resolve template variables in a value: {{user_id}}, {{model}}, {{provider_name}}
    fn resolve_templates(value: &Value, ctx: Option<&OverrideContext<'_>>) -> Value {
        match value {
            Value::String(s) => {
                let mut result = s.clone();
                if let Some(ctx) = ctx {
                    if result.contains("{{user_id}}") {
                        result = result.replace("{{user_id}}", ctx.user_id.unwrap_or(""));
                    }
                    if result.contains("{{model}}") {
                        result = result.replace("{{model}}", ctx.model.unwrap_or(""));
                    }
                    if result.contains("{{provider_name}}") {
                        result = result.replace("{{provider_name}}", ctx.provider_name);
                    }
                }
                Value::String(result)
            }
            Value::Object(map) => {
                let resolved: serde_json::Map<String, Value> = map.iter()
                    .map(|(k, v)| (k.clone(), Self::resolve_templates(v, ctx)))
                    .collect();
                Value::Object(resolved)
            }
            Value::Array(arr) => {
                Value::Array(arr.iter().map(|v| Self::resolve_templates(v, ctx)).collect())
            }
            other => other.clone(),
        }
    }
}
