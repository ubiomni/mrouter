//! Pricing engine — ratio-based cost calculation using system_settings
//!
//! Uses model ratios, completion ratios, cache ratios, and group ratios
//! stored in system_settings to compute request cost.

use std::collections::HashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::database::Database;
use crate::database::dao::SystemSettingsDao;

/// Pricing engine that computes cost from ratio-based settings
#[derive(Debug, Clone)]
pub struct PricingEngine {
    /// Model ratio: model_name -> ratio (base cost multiplier)
    pub model_ratios: HashMap<String, Decimal>,
    /// Completion ratio: model_name -> ratio (output cost relative to input)
    pub completion_ratios: HashMap<String, Decimal>,
    /// Cache read ratio: model_name -> ratio
    pub cache_ratios: HashMap<String, Decimal>,
    /// Cache write ratio: model_name -> ratio
    pub cache_write_ratios: HashMap<String, Decimal>,
    /// Group ratio: group_name -> multiplier
    pub group_ratios: HashMap<String, Decimal>,
    /// Provider-specific overrides: provider_name -> (model_name -> ratio)
    pub provider_overrides: HashMap<String, HashMap<String, Decimal>>,
}

impl PricingEngine {
    /// Load pricing configuration from system_settings table.
    ///
    /// Keys follow the pattern:
    /// - "ModelRatio" — JSON object { "model_name": ratio, ... }
    /// - "CompletionRatio" — JSON object { "model_name": ratio, ... }
    /// - "CacheRatio" — JSON object { "model_name": ratio, ... }
    /// - "CacheWriteRatio" — JSON object { "model_name": ratio, ... }
    /// - "GroupRatio" — JSON object { "group_name": ratio, ... }
    /// - "ModelRatio:provider:{name}" — provider-specific model ratios
    pub fn load(db: &Database) -> Self {
        let model_ratios = Self::load_ratio_map(db, "ModelRatio");
        let completion_ratios = Self::load_ratio_map(db, "CompletionRatio");
        let cache_ratios = Self::load_ratio_map(db, "CacheRatio");
        let cache_write_ratios = Self::load_ratio_map(db, "CacheWriteRatio");
        let group_ratios = Self::load_ratio_map(db, "GroupRatio");
        let provider_overrides = Self::load_provider_overrides(db);

        Self {
            model_ratios,
            completion_ratios,
            cache_ratios,
            cache_write_ratios,
            group_ratios,
            provider_overrides,
        }
    }

    /// Calculate cost for a request using the ratio-based formula.
    ///
    /// Formula:
    /// ```text
    /// input_cost  = input_tokens  * model_ratio * group_ratio / 1_000_000
    /// output_cost = output_tokens * model_ratio * completion_ratio * group_ratio / 1_000_000
    /// cache_read  = cache_read_tokens  * model_ratio * cache_ratio * group_ratio / 1_000_000
    /// cache_write = cache_write_tokens * model_ratio * cache_write_ratio * group_ratio / 1_000_000
    /// total = input_cost + output_cost + cache_read + cache_write
    /// ```
    pub fn calculate_cost(
        &self,
        model: &str,
        provider_name: &str,
        group: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_write_tokens: i64,
    ) -> f64 {
        let million = dec!(1_000_000);

        // Resolve model ratio (provider override first, then global)
        let model_ratio = self.resolve_model_ratio(model, provider_name);
        let completion_ratio = self.get_ratio(&self.completion_ratios, model, dec!(2));
        let cache_ratio = self.get_ratio(&self.cache_ratios, model, dec!(0.1));
        let cache_write_ratio = self.get_ratio(&self.cache_write_ratios, model, dec!(1));
        let group_ratio = self.group_ratios
            .get(group)
            .copied()
            .unwrap_or(dec!(1));

        let input_cost = Decimal::from(input_tokens) * model_ratio * group_ratio / million;
        let output_cost = Decimal::from(output_tokens) * model_ratio * completion_ratio * group_ratio / million;
        let cache_read_cost = Decimal::from(cache_read_tokens) * model_ratio * cache_ratio * group_ratio / million;
        let cache_write_cost = Decimal::from(cache_write_tokens) * model_ratio * cache_write_ratio * group_ratio / million;

        let total = input_cost + output_cost + cache_read_cost + cache_write_cost;
        total.to_string().parse::<f64>().unwrap_or(0.0).max(0.0)
    }

    /// Resolve model ratio: check provider override first, then global
    fn resolve_model_ratio(&self, model: &str, provider_name: &str) -> Decimal {
        if let Some(provider_map) = self.provider_overrides.get(provider_name) {
            if let Some(ratio) = provider_map.get(model) {
                return *ratio;
            }
        }
        self.get_ratio(&self.model_ratios, model, dec!(1))
    }

    /// Get a ratio from a map, with wildcard/prefix fallback
    fn get_ratio(&self, map: &HashMap<String, Decimal>, model: &str, default: Decimal) -> Decimal {
        // Exact match
        if let Some(ratio) = map.get(model) {
            return *ratio;
        }

        // Try prefix match (e.g., "gpt-4o" matches "gpt-4o-2024-08-06")
        let mut best_match: Option<(&str, Decimal)> = None;
        for (pattern, ratio) in map.iter() {
            if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1];
                if model.starts_with(prefix) {
                    // Keep the longest matching prefix
                    if best_match.map_or(true, |(prev, _)| prefix.len() > prev.len()) {
                        best_match = Some((prefix, *ratio));
                    }
                }
            }
        }

        if let Some((_, ratio)) = best_match {
            return ratio;
        }

        // Fallback to default
        default
    }

    /// Load a ratio map from a system setting key
    fn load_ratio_map(db: &Database, key: &str) -> HashMap<String, Decimal> {
        let mut map = HashMap::new();

        if let Ok(Some(setting)) = SystemSettingsDao::get(db, key) {
            if let Some(obj) = setting.value.as_object() {
                for (model_name, value) in obj {
                    if let Some(ratio) = value.as_f64() {
                        if let Ok(d) = Decimal::try_from(ratio) {
                            map.insert(model_name.clone(), d);
                        }
                    }
                }
            }
        }

        map
    }

    /// Load provider-specific model ratio overrides
    /// Keys: "ModelRatio:provider:{provider_name}"
    fn load_provider_overrides(db: &Database) -> HashMap<String, HashMap<String, Decimal>> {
        let mut overrides = HashMap::new();

        if let Ok(settings) = SystemSettingsDao::list_all(db) {
            for setting in settings {
                if let Some(provider_name) = setting.key.strip_prefix("ModelRatio:provider:") {
                    let mut provider_map = HashMap::new();
                    if let Some(obj) = setting.value.as_object() {
                        for (model_name, value) in obj {
                            if let Some(ratio) = value.as_f64() {
                                if let Ok(d) = Decimal::try_from(ratio) {
                                    provider_map.insert(model_name.clone(), d);
                                }
                            }
                        }
                    }
                    if !provider_map.is_empty() {
                        overrides.insert(provider_name.to_string(), provider_map);
                    }
                }
            }
        }

        overrides
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_ratio_exact_match() {
        let mut map = HashMap::new();
        map.insert("gpt-4o".to_string(), dec!(15));
        map.insert("gpt-4o-*".to_string(), dec!(10));

        let engine = PricingEngine {
            model_ratios: map.clone(),
            completion_ratios: HashMap::new(),
            cache_ratios: HashMap::new(),
            cache_write_ratios: HashMap::new(),
            group_ratios: HashMap::new(),
            provider_overrides: HashMap::new(),
        };

        // Exact match
        assert_eq!(engine.get_ratio(&map, "gpt-4o", dec!(1)), dec!(15));
        // Wildcard match
        assert_eq!(engine.get_ratio(&map, "gpt-4o-2024", dec!(1)), dec!(10));
        // No match -> default
        assert_eq!(engine.get_ratio(&map, "claude-3", dec!(1)), dec!(1));
    }
}
