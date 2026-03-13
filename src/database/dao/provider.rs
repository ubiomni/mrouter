// Provider DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::{Provider, ProviderType, AppType, ApiFormat};
use chrono::Utc;

pub struct ProviderDao;

impl ProviderDao {
    /// 获取所有 Provider（不按 app_type 过滤）
    pub fn get_all_providers(db: &Database) -> Result<Vec<Provider>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, app_type, name, is_active, api_key, base_url, model, config, priority, created_at, updated_at, provider_type, sync_to_cli_tools, supported_models, enable_stats, api_format
             FROM providers ORDER BY priority, name"
        )?;

        let providers = stmt.query_map([], |row| {
            let app_type_str: String = row.get(1)?;
            let app_type = app_type_str.parse::<AppType>().unwrap_or(AppType::ClaudeCode);
            let pt_str: String = row.get(11)?;
            let provider_type = pt_str.parse::<ProviderType>().unwrap_or(ProviderType::Custom);
            let sync_to_cli_tools_str: String = row.get(12)?;
            let sync_to_cli_tools: Vec<String> = serde_json::from_str(&sync_to_cli_tools_str).unwrap_or_default();
            let supported_models: Option<Vec<String>> = row.get::<_, Option<String>>(13)?
                .and_then(|s| serde_json::from_str(&s).ok());
            let api_format: Option<ApiFormat> = row.get::<_, Option<String>>(15)?
                .and_then(|s| s.parse().ok());

            Ok(Provider {
                id: row.get(0)?,
                app_type,
                provider_type,
                name: row.get(2)?,
                is_active: row.get::<_, i32>(3)? != 0,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                model: row.get(6)?,
                config: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                priority: row.get(8)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .unwrap()
                    .with_timezone(&Utc),
                sync_to_cli_tools,
                supported_models,
                enable_stats: row.get::<_, i32>(14)? != 0,
                api_format,
            })
        })?;

        let mut result = Vec::new();
        for provider in providers {
            result.push(provider?);
        }
        Ok(result)
    }

    pub fn get_all(db: &Database, app_type: AppType) -> Result<Vec<Provider>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, app_type, name, is_active, api_key, base_url, model, config, priority, created_at, updated_at, provider_type, sync_to_cli_tools, supported_models, enable_stats, api_format
             FROM providers WHERE app_type = ?1 ORDER BY priority, name"
        )?;

        let providers = stmt.query_map([app_type.as_str()], |row| {
            let pt_str: String = row.get(11)?;
            let provider_type = pt_str.parse::<ProviderType>().unwrap_or(ProviderType::Custom);
            let sync_to_cli_tools_str: String = row.get(12)?;
            let sync_to_cli_tools: Vec<String> = serde_json::from_str(&sync_to_cli_tools_str).unwrap_or_default();
            let supported_models: Option<Vec<String>> = row.get::<_, Option<String>>(13)?
                .and_then(|s| serde_json::from_str(&s).ok());
            let api_format: Option<ApiFormat> = row.get::<_, Option<String>>(15)?
                .and_then(|s| s.parse().ok());

            Ok(Provider {
                id: row.get(0)?,
                app_type,
                provider_type,
                name: row.get(2)?,
                is_active: row.get::<_, i32>(3)? != 0,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                model: row.get(6)?,
                config: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                priority: row.get(8)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .unwrap()
                    .with_timezone(&Utc),
                sync_to_cli_tools,
                supported_models,
                enable_stats: row.get::<_, i32>(14)? != 0,
                api_format,
            })
        })?;

        let mut result = Vec::new();
        for provider in providers {
            result.push(provider?);
        }
        Ok(result)
    }

    pub fn get_active(db: &Database, app_type: AppType) -> Result<Option<Provider>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, app_type, name, is_active, api_key, base_url, model, config, priority, created_at, updated_at, provider_type, sync_to_cli_tools, supported_models, enable_stats, api_format
             FROM providers WHERE app_type = ?1 AND is_active = 1 LIMIT 1"
        )?;

        let mut rows = stmt.query([app_type.as_str()])?;
        if let Some(row) = rows.next()? {
            let pt_str: String = row.get(11)?;
            let provider_type = pt_str.parse::<ProviderType>().unwrap_or(ProviderType::Custom);
            let sync_to_cli_tools_str: String = row.get(12)?;
            let sync_to_cli_tools: Vec<String> = serde_json::from_str(&sync_to_cli_tools_str).unwrap_or_default();
            let supported_models: Option<Vec<String>> = row.get::<_, Option<String>>(13)?
                .and_then(|s| serde_json::from_str(&s).ok());
            let api_format: Option<ApiFormat> = row.get::<_, Option<String>>(15)?
                .and_then(|s| s.parse().ok());

            Ok(Some(Provider {
                id: row.get(0)?,
                app_type,
                provider_type,
                name: row.get(2)?,
                is_active: true,
                api_key: row.get(4)?,
                base_url: row.get(5)?,
                model: row.get(6)?,
                config: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                priority: row.get(8)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .unwrap()
                    .with_timezone(&Utc),
                sync_to_cli_tools,
                supported_models,
                enable_stats: row.get::<_, i32>(14)? != 0,
                api_format,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert(db: &Database, provider: &Provider) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let config_str = serde_json::to_string(&provider.config)?;
        let sync_to_cli_tools_str = serde_json::to_string(&provider.sync_to_cli_tools)?;
        let supported_models_str = provider.supported_models.as_ref()
            .map(|m| serde_json::to_string(m).ok())
            .flatten();
        let api_format_str = provider.api_format.map(|f| f.to_string());

        db.execute(
            "INSERT INTO providers (app_type, name, is_active, api_key, base_url, model, config, priority, created_at, updated_at, provider_type, sync_to_cli_tools, supported_models, enable_stats, api_format)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                provider.app_type.as_str(),
                provider.name,
                if provider.is_active { 1 } else { 0 },
                provider.api_key,
                provider.base_url,
                provider.model,
                config_str,
                provider.priority,
                now,
                now,
                provider.provider_type.as_str(),
                sync_to_cli_tools_str,
                supported_models_str,
                if provider.enable_stats { 1 } else { 0 },
                api_format_str,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, provider: &Provider) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let config_str = serde_json::to_string(&provider.config)?;
        let sync_to_cli_tools_str = serde_json::to_string(&provider.sync_to_cli_tools)?;
        let supported_models_str = provider.supported_models.as_ref()
            .map(|m| serde_json::to_string(m).ok())
            .flatten();
        let api_format_str = provider.api_format.map(|f| f.to_string());

        db.execute(
            "UPDATE providers SET name = ?1, is_active = ?2, api_key = ?3, base_url = ?4, model = ?5, config = ?6, priority = ?7, updated_at = ?8, provider_type = ?9, sync_to_cli_tools = ?10, supported_models = ?11, enable_stats = ?12, api_format = ?13
             WHERE id = ?14",
            rusqlite::params![
                provider.name,
                if provider.is_active { 1 } else { 0 },
                provider.api_key,
                provider.base_url,
                provider.model,
                config_str,
                provider.priority,
                now,
                provider.provider_type.as_str(),
                sync_to_cli_tools_str,
                supported_models_str,
                if provider.enable_stats { 1 } else { 0 },
                api_format_str,
                provider.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        db.execute("DELETE FROM providers WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_active(db: &Database, app_type: AppType, id: i64) -> Result<()> {
        // 先取消所有激活状态
        db.execute(
            "UPDATE providers SET is_active = 0 WHERE app_type = ?1",
            [app_type.as_str()],
        )?;

        // 激活指定 provider
        db.execute(
            "UPDATE providers SET is_active = 1 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }
}
