// Provider DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::Provider;
use chrono::Utc;

pub struct ProviderDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl ProviderDao {
    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Provider>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                    priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                    auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category
             FROM providers WHERE id = ?1"
        )?;

        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_provider(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_by_name(db: &Database, name: &str) -> Result<Option<Provider>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                    priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                    auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category
             FROM providers WHERE name = ?1"
        )?;

        let mut rows = stmt.query(rusqlite::params![name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_provider(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_active(db: &Database, org_id: i64) -> Result<Vec<Provider>> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = db.conn.lock().unwrap();
        // Include active providers + auto-banned providers whose cooldown has expired
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                    priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                    auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category
             FROM providers WHERE org_id = ?1 AND (status = 1 OR (status = 3 AND banned_until > 0 AND banned_until <= ?2))
             ORDER BY priority, weight DESC, name"
        )?;

        let rows = stmt.query_map(rusqlite::params![org_id, now], |row| Self::row_to_provider(row))?;
        let mut result = Vec::new();
        for row in rows {
            let mut provider = row?;
            // Lazy recovery: mark cooldown-expired auto-disabled providers as enabled again
            if provider.status == crate::models::ProviderStatus::AutoDisabled && provider.banned_until > 0 && provider.banned_until <= now {
                provider.status = crate::models::ProviderStatus::Enabled;
                provider.banned_until = 0;
                // Fire-and-forget DB update
                let _ = db.execute(
                    "UPDATE providers SET status = 1, banned_until = 0, updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, provider.id],
                );
                tracing::info!("[AutoBan] Lazy recovery: provider #{} '{}' cooldown expired, re-enabled", provider.id, provider.name);
            }
            result.push(provider);
        }
        Ok(result)
    }

    pub fn get_all(db: &Database, org_id: i64) -> Result<Vec<Provider>> {
        let conn = db.conn.lock().unwrap();
        let (sql, params): (&str, Vec<rusqlite::types::Value>) = if org_id <= 0 {
            (
                "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                        priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                        auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category
                 FROM providers ORDER BY priority, weight DESC, name",
                vec![],
            )
        } else {
            (
                "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                        priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                        auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category
                 FROM providers WHERE org_id = ?1 ORDER BY priority, weight DESC, name",
                vec![rusqlite::types::Value::Integer(org_id)],
            )
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| Self::row_to_provider(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_by_category(db: &Database, category: &str) -> Result<Vec<Provider>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                    priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                    auth_method, custom_headers, is_active, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category
             FROM providers WHERE category = ?1 ORDER BY priority, weight DESC, name"
        )?;

        let rows = stmt.query_map(rusqlite::params![category], |row| Self::row_to_provider(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn insert(db: &Database, provider: &Provider) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        let custom_headers_json = provider.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        let encrypted_api_key = crate::crypto::encrypt(&provider.api_key)?;

        db.execute(
            "INSERT INTO providers (org_id, name, display_name, preset_key, base_url, api_key,
                    priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                    auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            rusqlite::params![
                provider.org_id,
                provider.name,
                provider.display_name,
                provider.preset_key,
                provider.base_url,
                encrypted_api_key,
                provider.priority,
                provider.weight,
                provider.provider_group,
                provider.rpm_limit,
                provider.tpm_limit,
                provider.concurrent_limit,
                provider.auth_method,
                custom_headers_json,
                provider.status as i32,
                provider.auto_ban as i32,
                provider.banned_until,
                provider.remark,
                now,
                now,
                provider.category,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, provider: &Provider) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let custom_headers_json = provider.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        let encrypted_api_key = crate::crypto::encrypt(&provider.api_key)?;

        db.execute(
            "UPDATE providers SET org_id = ?1, name = ?2, display_name = ?3, preset_key = ?4, base_url = ?5,
                    api_key = ?6, priority = ?7, weight = ?8, provider_group = ?9, rpm_limit = ?10,
                    tpm_limit = ?11, concurrent_limit = ?12, auth_method = ?13, custom_headers = ?14,
                    status = ?15, auto_ban = ?16, banned_until = ?17, remark = ?18, updated_at = ?19, category = ?20
             WHERE id = ?21",
            rusqlite::params![
                provider.org_id,
                provider.name,
                provider.display_name,
                provider.preset_key,
                provider.base_url,
                encrypted_api_key,
                provider.priority,
                provider.weight,
                provider.provider_group,
                provider.rpm_limit,
                provider.tpm_limit,
                provider.concurrent_limit,
                provider.auth_method,
                custom_headers_json,
                provider.status as i32,
                provider.auto_ban as i32,
                provider.banned_until,
                provider.remark,
                now,
                provider.category,
                provider.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        db.execute("DELETE FROM providers WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    pub fn delete_health(db: &Database, provider_id: i64) -> Result<()> {
        db.execute("DELETE FROM provider_health WHERE provider_id = ?1", rusqlite::params![provider_id])?;
        Ok(())
    }

    pub fn set_status(db: &Database, id: i64, status: crate::models::ProviderStatus) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        db.execute(
            "UPDATE providers SET status = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![status as i32, now, id],
        )?;
        Ok(())
    }

    /// Check if a provider name already exists
    pub fn name_exists(db: &Database, name: &str) -> Result<bool> {
        let conn = db.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Check if a provider name exists excluding a specific id
    pub fn name_exists_excluding(db: &Database, name: &str, exclude_id: i64) -> Result<bool> {
        let conn = db.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE name = ?1 AND id != ?2",
            rusqlite::params![name, exclude_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Migrate plaintext api_keys to encrypted format
    pub fn migrate_encrypt_keys(db: &Database) -> Result<u32> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, api_key FROM providers WHERE api_key != '' AND api_key NOT LIKE 'enc:v1:%'"
        )?;

        let rows: Vec<(i64, String)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;

        drop(stmt);

        let mut count = 0u32;
        for (id, plaintext) in rows {
            if let Ok(encrypted) = crate::crypto::encrypt(&plaintext) {
                conn.execute(
                    "UPDATE providers SET api_key = ?1 WHERE id = ?2",
                    rusqlite::params![encrypted, id],
                )?;
                count += 1;
            }
        }

        if count > 0 {
            tracing::info!("Migrated {} provider api_key(s) to encrypted format", count);
        }
        Ok(count)
    }

    /// Create a provider from a JSON value (API request body)
    pub fn create_from_json(db: &Database, json: &serde_json::Value) -> Result<i64> {
        let provider = Self::json_to_provider(json, None)?;
        Self::insert(db, &provider)
    }

    /// Update a provider from a JSON value (API request body)
    pub fn update_from_json(db: &Database, id: i64, json: &serde_json::Value) -> Result<()> {
        let existing = Self::get_by_id(db, id)?
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", id))?;
        let provider = Self::json_to_provider(json, Some(existing))?;
        Self::update(db, &Provider { id, ..provider })
    }

    // --- private helpers ---

    fn json_to_provider(json: &serde_json::Value, existing: Option<Provider>) -> Result<Provider> {
        use crate::models::ProviderStatus;
        let base = existing.unwrap_or_else(|| {
            Provider::new(1, String::new(), String::new(), String::new())
        });

        let status = json.get("status")
            .and_then(|v| v.as_i64())
            .map(|s| ProviderStatus::from_i32(s as i32))
            .unwrap_or(base.status);

        Ok(Provider {
            id: base.id,
            org_id: json.get("org_id").and_then(|v| v.as_i64()).unwrap_or(base.org_id),
            name: json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.name),
            display_name: json.get("display_name").and_then(|v| v.as_str()).map(|s| s.to_string()).or(base.display_name),
            preset_key: json.get("preset_key").and_then(|v| v.as_str()).map(|s| s.to_string()).or(base.preset_key),
            base_url: json.get("base_url").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.base_url),
            api_key: json.get("api_key").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or(base.api_key),
            priority: json.get("priority").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.priority),
            weight: json.get("weight").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.weight),
            provider_group: json.get("provider_group").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.provider_group),
            rpm_limit: json.get("rpm_limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.rpm_limit),
            tpm_limit: json.get("tpm_limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.tpm_limit),
            concurrent_limit: json.get("concurrent_limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.concurrent_limit),
            auth_method: json.get("auth_method").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.auth_method),
            custom_headers: json.get("custom_headers").cloned().or(base.custom_headers),
            status,
            auto_ban: json.get("auto_ban").and_then(|v| v.as_bool()).unwrap_or(base.auto_ban),
            banned_until: json.get("banned_until").and_then(|v| v.as_i64()).unwrap_or(base.banned_until),
            remark: json.get("remark").and_then(|v| v.as_str()).map(|s| s.to_string()).or(base.remark),
            category: json.get("category").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.category),
            created_at: base.created_at,
            updated_at: Utc::now().timestamp_millis(),
        })
    }

    fn row_to_provider(row: &rusqlite::Row<'_>) -> rusqlite::Result<Provider> {
        use crate::models::ProviderStatus;
        // SELECT column order:
        // 0:id, 1:org_id, 2:name, 3:display_name, 4:preset_key, 5:base_url, 6:api_key,
        // 7:priority, 8:weight, 9:provider_group, 10:rpm_limit, 11:tpm_limit,
        // 12:concurrent_limit, 13:auth_method, 14:custom_headers, 15:status,
        // 16:auto_ban, 17:banned_until, 18:remark, 19:created_at, 20:updated_at, 21:category
        let custom_headers: Option<serde_json::Value> = row.get::<_, Option<String>>(14)?
            .and_then(|s| serde_json::from_str(&s).ok());

        let raw_api_key: String = row.get(6)?;
        let api_key = crate::crypto::decrypt_if_encrypted(&raw_api_key)
            .unwrap_or_else(|_| raw_api_key);

        Ok(Provider {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            display_name: row.get(3)?,
            preset_key: row.get(4)?,
            base_url: row.get(5)?,
            api_key,
            priority: row.get(7)?,
            weight: row.get(8)?,
            provider_group: row.get(9)?,
            rpm_limit: row.get(10)?,
            tpm_limit: row.get(11)?,
            concurrent_limit: row.get(12)?,
            auth_method: row.get(13)?,
            custom_headers,
            status: ProviderStatus::from_i32(row.get(15)?),
            auto_ban: row.get::<_, i32>(16)? != 0,
            banned_until: row.get(17)?,
            remark: row.get(18)?,
            category: row.get::<_, Option<String>>(21)?.unwrap_or_else(|| "llm".to_string()),
            created_at: row.get(19)?,
            updated_at: row.get(20)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl ProviderDao {
    fn row_to_provider_mysql(row: sqlx::mysql::MySqlRow) -> Provider {
        use sqlx::Row;
        use crate::models::ProviderStatus;
        let custom_headers: Option<serde_json::Value> = row.try_get::<serde_json::Value, _>("custom_headers").ok()
            .and_then(|v| if v.is_null() { None } else { Some(v) });
        let status: i32 = row.get("status");
        let auto_ban: i8 = row.get("auto_ban");

        let raw_api_key: String = row.get("api_key");
        let api_key = crate::crypto::decrypt_if_encrypted(&raw_api_key)
            .unwrap_or_else(|_| raw_api_key);

        Provider {
            id: row.get("id"),
            org_id: row.get("org_id"),
            name: row.get("name"),
            display_name: row.get("display_name"),
            preset_key: row.get("preset_key"),
            base_url: row.get("base_url"),
            api_key,
            priority: row.get("priority"),
            weight: row.get("weight"),
            provider_group: row.get::<Option<String>, _>("provider_group").unwrap_or_default(),
            rpm_limit: row.get("rpm_limit"),
            tpm_limit: row.get("tpm_limit"),
            concurrent_limit: row.get("concurrent_limit"),
            auth_method: row.get("auth_method"),
            custom_headers,
            status: ProviderStatus::from_i32(status),
            auto_ban: auto_ban != 0,
            banned_until: row.get("banned_until"),
            remark: row.get("remark"),
            category: row.try_get::<String, _>("category").unwrap_or_else(|_| "llm".to_string()),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    const SELECT_COLS: &'static str = "SELECT id, org_id, name, display_name, preset_key, base_url, api_key,
                            priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                            auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, COALESCE(category,'llm') as category FROM providers";

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Provider>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} WHERE id = ?", Self::SELECT_COLS);
                let row = sqlx::query(&sql)
                    .bind(id)
                    .map(Self::row_to_provider_mysql)
                    .fetch_optional(&db.pool).await?;
                Ok(row)
            })
        })
    }

    pub fn get_by_name(db: &Database, name: &str) -> Result<Option<Provider>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} WHERE name = ?", Self::SELECT_COLS);
                let row = sqlx::query(&sql)
                    .bind(name)
                    .map(Self::row_to_provider_mysql)
                    .fetch_optional(&db.pool).await?;
                Ok(row)
            })
        })
    }

    pub fn get_all_active(db: &Database, org_id: i64) -> Result<Vec<Provider>> {
        use crate::models::ProviderStatus;
        let now = chrono::Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Include active + cooldown-expired auto-disabled providers
                let sql = format!(
                    "{} WHERE org_id = ? AND (status = 1 OR (status = 3 AND banned_until > 0 AND banned_until <= ?)) ORDER BY priority, weight DESC, name",
                    Self::SELECT_COLS
                );
                let rows = sqlx::query(&sql)
                    .bind(org_id)
                    .bind(now)
                    .map(Self::row_to_provider_mysql)
                    .fetch_all(&db.pool).await?;

                // Lazy recovery for cooldown-expired auto-disabled providers
                let mut result = Vec::with_capacity(rows.len());
                for mut provider in rows {
                    if provider.status == ProviderStatus::AutoDisabled && provider.banned_until > 0 && provider.banned_until <= now {
                        provider.status = ProviderStatus::Enabled;
                        provider.banned_until = 0;
                        // Fire-and-forget DB update
                        let _ = sqlx::query("UPDATE providers SET status = 1, banned_until = 0, updated_at = ? WHERE id = ?")
                            .bind(now).bind(provider.id)
                            .execute(&db.pool).await;
                        tracing::info!("[AutoBan] Lazy recovery: provider #{} '{}' cooldown expired, re-enabled", provider.id, provider.name);
                    }
                    result.push(provider);
                }
                Ok(result)
            })
        })
    }

    pub fn get_all(db: &Database, org_id: i64) -> Result<Vec<Provider>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = if org_id <= 0 {
                    format!("{} ORDER BY priority, weight DESC, name", Self::SELECT_COLS)
                } else {
                    format!("{} WHERE org_id = ? ORDER BY priority, weight DESC, name", Self::SELECT_COLS)
                };

                let rows = if org_id <= 0 {
                    sqlx::query(&sql)
                        .map(Self::row_to_provider_mysql)
                        .fetch_all(&db.pool).await?
                } else {
                    sqlx::query(&sql)
                        .bind(org_id)
                        .map(Self::row_to_provider_mysql)
                        .fetch_all(&db.pool).await?
                };
                Ok(rows)
            })
        })
    }

    pub fn list_by_category(db: &Database, category: &str) -> Result<Vec<Provider>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} WHERE category = ? ORDER BY priority, weight DESC, name", Self::SELECT_COLS);
                let rows = sqlx::query(&sql)
                    .bind(category)
                    .map(Self::row_to_provider_mysql)
                    .fetch_all(&db.pool).await?;
                Ok(rows)
            })
        })
    }

    pub fn insert(db: &Database, provider: &Provider) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        let custom_headers_json = provider.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        let encrypted_key = crate::crypto::encrypt(&provider.api_key)
            .unwrap_or_else(|_| provider.api_key.clone());

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query(
                    "INSERT INTO providers (org_id, name, display_name, preset_key, base_url, api_key,
                            priority, weight, provider_group, rpm_limit, tpm_limit, concurrent_limit,
                            auth_method, custom_headers, status, auto_ban, banned_until, remark, created_at, updated_at, category)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(provider.org_id)
                .bind(&provider.name)
                .bind(&provider.display_name)
                .bind(&provider.preset_key)
                .bind(&provider.base_url)
                .bind(&encrypted_key)
                .bind(provider.priority)
                .bind(provider.weight)
                .bind(&provider.provider_group)
                .bind(provider.rpm_limit)
                .bind(provider.tpm_limit)
                .bind(provider.concurrent_limit)
                .bind(&provider.auth_method)
                .bind(&custom_headers_json)
                .bind(provider.status as i32)
                .bind(provider.auto_ban as i8)
                .bind(provider.banned_until)
                .bind(&provider.remark)
                .bind(now)
                .bind(now)
                .bind(&provider.category)
                .execute(&db.pool).await?;

                Ok(result.last_insert_id() as i64)
            })
        })
    }

    pub fn update(db: &Database, provider: &Provider) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let custom_headers_json = provider.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        let encrypted_key = crate::crypto::encrypt(&provider.api_key)
            .unwrap_or_else(|_| provider.api_key.clone());

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "UPDATE providers SET org_id = ?, name = ?, display_name = ?, preset_key = ?,
                            base_url = ?, api_key = ?, priority = ?, weight = ?,
                            provider_group = ?, rpm_limit = ?, tpm_limit = ?, concurrent_limit = ?,
                            auth_method = ?, custom_headers = ?, status = ?, auto_ban = ?,
                            banned_until = ?, remark = ?, updated_at = ?, category = ?
                     WHERE id = ?"
                )
                .bind(provider.org_id)
                .bind(&provider.name)
                .bind(&provider.display_name)
                .bind(&provider.preset_key)
                .bind(&provider.base_url)
                .bind(&encrypted_key)
                .bind(provider.priority)
                .bind(provider.weight)
                .bind(&provider.provider_group)
                .bind(provider.rpm_limit)
                .bind(provider.tpm_limit)
                .bind(provider.concurrent_limit)
                .bind(&provider.auth_method)
                .bind(&custom_headers_json)
                .bind(provider.status as i32)
                .bind(provider.auto_ban as i8)
                .bind(provider.banned_until)
                .bind(&provider.remark)
                .bind(now)
                .bind(&provider.category)
                .bind(provider.id)
                .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM providers WHERE id = ?")
                    .bind(id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn delete_health(db: &Database, provider_id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM provider_health WHERE provider_id = ?")
                    .bind(provider_id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn set_status(db: &Database, id: i64, status: crate::models::ProviderStatus) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE providers SET status = ?, updated_at = ? WHERE id = ?")
                    .bind(status as i32).bind(now).bind(id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn name_exists(db: &Database, name: &str) -> Result<bool> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM providers WHERE name = ?")
                    .bind(name).fetch_one(&db.pool).await?;
                Ok(count.0 > 0)
            })
        })
    }

    pub fn name_exists_excluding(db: &Database, name: &str, exclude_id: i64) -> Result<bool> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM providers WHERE name = ? AND id != ?")
                    .bind(name).bind(exclude_id).fetch_one(&db.pool).await?;
                Ok(count.0 > 0)
            })
        })
    }

    pub fn create_from_json(db: &Database, json: &serde_json::Value) -> Result<i64> {
        let provider = Self::json_to_provider_mysql(json, None)?;
        Self::insert(db, &provider)
    }

    pub fn update_from_json(db: &Database, id: i64, json: &serde_json::Value) -> Result<Provider> {
        let existing = Self::get_by_id(db, id)?
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", id))?;
        let provider = Self::json_to_provider_mysql(json, Some(existing))?;
        let updated = Provider { id, ..provider };
        Self::update(db, &updated)?;
        Ok(updated)
    }

    fn json_to_provider_mysql(json: &serde_json::Value, existing: Option<Provider>) -> Result<Provider> {
        let base = existing.unwrap_or_else(|| {
            Provider::new(1, String::new(), String::new(), String::new())
        });

        Ok(Provider {
            id: base.id,
            org_id: json.get("org_id").and_then(|v| v.as_i64()).unwrap_or(base.org_id),
            name: json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.name),
            display_name: json.get("display_name").and_then(|v| v.as_str()).map(|s| s.to_string()).or(base.display_name),
            preset_key: json.get("preset_key").and_then(|v| v.as_str()).map(|s| s.to_string()).or(base.preset_key),
            base_url: json.get("base_url").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.base_url),
            api_key: json.get("api_key").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or(base.api_key),
            priority: json.get("priority").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.priority),
            weight: json.get("weight").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.weight),
            provider_group: json.get("provider_group").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.provider_group),
            rpm_limit: json.get("rpm_limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.rpm_limit),
            tpm_limit: json.get("tpm_limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.tpm_limit),
            concurrent_limit: json.get("concurrent_limit").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(base.concurrent_limit),
            auth_method: json.get("auth_method").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.auth_method),
            custom_headers: json.get("custom_headers").cloned().or(base.custom_headers),
            status: json.get("status")
                .and_then(|v| v.as_i64())
                .map(|v| crate::models::ProviderStatus::from_i32(v as i32))
                .unwrap_or(base.status),
            auto_ban: json.get("auto_ban").and_then(|v| v.as_bool()).unwrap_or(base.auto_ban),
            banned_until: json.get("banned_until").and_then(|v| v.as_i64()).unwrap_or(base.banned_until),
            remark: json.get("remark").and_then(|v| v.as_str()).map(|s| s.to_string()).or(base.remark),
            category: json.get("category").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(base.category),
            created_at: base.created_at,
            updated_at: Utc::now().timestamp_millis(),
        })
    }

    /// Migrate plaintext api_keys to encrypted format
    pub fn migrate_encrypt_keys(db: &Database) -> Result<u32> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows: Vec<(i64, String)> = sqlx::query_as(
                    "SELECT id, api_key FROM providers WHERE api_key != '' AND api_key NOT LIKE 'enc:v1:%'"
                )
                .fetch_all(&db.pool).await?;

                let mut count = 0u32;
                for (id, plaintext) in &rows {
                    if let Ok(encrypted) = crate::crypto::encrypt(plaintext) {
                        sqlx::query("UPDATE providers SET api_key = ? WHERE id = ?")
                            .bind(&encrypted)
                            .bind(id)
                            .execute(&db.pool).await?;
                        count += 1;
                    }
                }
                if count > 0 {
                    tracing::info!("Migrated {} provider api_key(s) to encrypted format", count);
                }
                Ok(count)
            })
        })
    }
}
