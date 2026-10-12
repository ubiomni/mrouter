// API Key DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::ApiKey;
use chrono::Utc;

pub struct ApiKeyDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl ApiKeyDao {
    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<ApiKey>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_hash, key_prefix, provider_group, quota_limit,
                    used_tokens, rpm_limit, is_active, expires_at, last_used_at, created_at, updated_at,
                    COALESCE(key_suffix, '') as key_suffix, COALESCE(tpm_limit, 0) as tpm_limit,
                    encrypted_key, quota_group, model_limits
             FROM api_keys WHERE id = ?1"
        )?;

        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_api_key(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_by_hash(db: &Database, key_hash: &str) -> Result<Option<ApiKey>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_hash, key_prefix, provider_group, quota_limit,
                    used_tokens, rpm_limit, is_active, expires_at, last_used_at, created_at, updated_at,
                    COALESCE(key_suffix, '') as key_suffix, COALESCE(tpm_limit, 0) as tpm_limit,
                    encrypted_key, quota_group, model_limits
             FROM api_keys WHERE key_hash = ?1"
        )?;

        let mut rows = stmt.query(rusqlite::params![key_hash])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_api_key(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_by_user(db: &Database, user_id: i64) -> Result<Vec<ApiKey>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_hash, key_prefix, provider_group, quota_limit,
                    used_tokens, rpm_limit, is_active, expires_at, last_used_at, created_at, updated_at,
                    COALESCE(key_suffix, '') as key_suffix, COALESCE(tpm_limit, 0) as tpm_limit,
                    encrypted_key, quota_group, model_limits
             FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC"
        )?;

        let rows = stmt.query_map(rusqlite::params![user_id], |row| Self::row_to_api_key(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_all(db: &Database) -> Result<Vec<ApiKey>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_hash, key_prefix, provider_group, quota_limit,
                    used_tokens, rpm_limit, is_active, expires_at, last_used_at, created_at, updated_at,
                    COALESCE(key_suffix, '') as key_suffix, COALESCE(tpm_limit, 0) as tpm_limit,
                    encrypted_key, quota_group, model_limits
             FROM api_keys ORDER BY created_at DESC"
        )?;

        let rows = stmt.query_map([], |row| Self::row_to_api_key(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn insert(db: &Database, key: &ApiKey) -> Result<i64> {
        let now = Utc::now().timestamp_millis();

        db.execute(
            "INSERT INTO api_keys (user_id, name, key_hash, key_prefix, key_suffix, encrypted_key, provider_group, quota_limit,
                    quota_group, used_tokens, rpm_limit, tpm_limit, is_active, expires_at, last_used_at, created_at, updated_at, model_limits)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            rusqlite::params![
                key.user_id,
                key.name,
                key.key_hash,
                key.key_prefix,
                key.key_suffix,
                key.encrypted_key,
                key.provider_group,
                key.quota_limit,
                key.quota_group,
                key.used_tokens,
                key.rpm_limit,
                key.tpm_limit,
                key.is_active as i32,
                key.expires_at,
                key.last_used_at,
                now,
                now,
                key.model_limits,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, key: &ApiKey) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        db.execute(
            "UPDATE api_keys SET user_id = ?1, name = ?2, key_hash = ?3, key_prefix = ?4,
                    key_suffix = ?5, provider_group = ?6, quota_limit = ?7, used_tokens = ?8, rpm_limit = ?9,
                    tpm_limit = ?10, is_active = ?11, expires_at = ?12, last_used_at = ?13, updated_at = ?14,
                    quota_group = ?15, model_limits = ?16
             WHERE id = ?17",
            rusqlite::params![
                key.user_id,
                key.name,
                key.key_hash,
                key.key_prefix,
                key.key_suffix,
                key.provider_group,
                key.quota_limit,
                key.used_tokens,
                key.rpm_limit,
                key.tpm_limit,
                key.is_active as i32,
                key.expires_at,
                key.last_used_at,
                now,
                key.quota_group,
                key.model_limits,
                key.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        db.execute("DELETE FROM api_keys WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    pub fn increment_used_tokens(db: &Database, id: i64, tokens: i64) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        db.execute(
            "UPDATE api_keys SET used_tokens = used_tokens + ?1, last_used_at = ?2, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![tokens, now, id],
        )?;
        Ok(())
    }

    /// Update last_used_at timestamp (called on each API key usage).
    pub fn update_last_used(db: &Database, id: i64) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        db.execute(
            "UPDATE api_keys SET last_used_at = ?1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )?;
        Ok(())
    }

    pub fn set_active(db: &Database, id: i64, active: bool) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        db.execute(
            "UPDATE api_keys SET is_active = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![active as i32, now, id],
        )?;
        Ok(())
    }

    // --- private helpers ---

    fn row_to_api_key(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKey> {
        Ok(ApiKey {
            id: row.get(0)?,
            user_id: row.get(1)?,
            name: row.get(2)?,
            key_hash: row.get(3)?,
            key_prefix: row.get(4)?,
            provider_group: row.get(5)?,
            quota_limit: row.get(6)?,
            used_tokens: row.get(7)?,
            rpm_limit: row.get(8)?,
            is_active: row.get::<_, i32>(9)? != 0,
            expires_at: row.get(10)?,
            last_used_at: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
            key_suffix: row.get(14)?,
            tpm_limit: row.get(15)?,
            encrypted_key: row.get(16)?,
            quota_group: row.get(17)?,
            model_limits: row.get(18)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl ApiKeyDao {
    const SELECT_COLS: &'static str = "SELECT id, user_id, name, key_hash, key_prefix, key_suffix, provider_group, quota_limit,
                            used_tokens, rpm_limit, COALESCE(tpm_limit, 0) as tpm_limit, is_active, expires_at, last_used_at, created_at, updated_at,
                            encrypted_key, quota_group, model_limits FROM api_keys";

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<ApiKey>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} WHERE id = ?", Self::SELECT_COLS);
                let row = sqlx::query(&sql)
                    .bind(id)
                    .map(|r: sqlx::mysql::MySqlRow| Self::row_to_api_key_mysql(r))
                    .fetch_optional(&db.pool).await?;
                Ok(row)
            })
        })
    }

    pub fn get_by_hash(db: &Database, key_hash: &str) -> Result<Option<ApiKey>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} WHERE key_hash = ?", Self::SELECT_COLS);
                let row = sqlx::query(&sql)
                    .bind(key_hash)
                    .map(|r: sqlx::mysql::MySqlRow| Self::row_to_api_key_mysql(r))
                    .fetch_optional(&db.pool).await?;
                Ok(row)
            })
        })
    }

    pub fn list_by_user(db: &Database, user_id: i64) -> Result<Vec<ApiKey>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} WHERE user_id = ? ORDER BY created_at DESC", Self::SELECT_COLS);
                let rows = sqlx::query(&sql)
                    .bind(user_id)
                    .map(|r: sqlx::mysql::MySqlRow| Self::row_to_api_key_mysql(r))
                    .fetch_all(&db.pool).await?;
                Ok(rows)
            })
        })
    }

    pub fn list_all(db: &Database) -> Result<Vec<ApiKey>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("{} ORDER BY created_at DESC", Self::SELECT_COLS);
                let rows = sqlx::query(&sql)
                    .map(|r: sqlx::mysql::MySqlRow| Self::row_to_api_key_mysql(r))
                    .fetch_all(&db.pool).await?;
                Ok(rows)
            })
        })
    }

    pub fn insert(db: &Database, key: &ApiKey) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query(
                    "INSERT INTO api_keys (user_id, name, key_hash, key_prefix, key_suffix, encrypted_key, provider_group, quota_limit,
                            quota_group, used_tokens, rpm_limit, tpm_limit, is_active, expires_at, last_used_at, created_at, updated_at, model_limits)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(key.user_id)
                .bind(&key.name)
                .bind(&key.key_hash)
                .bind(&key.key_prefix)
                .bind(&key.key_suffix)
                .bind(&key.encrypted_key)
                .bind(&key.provider_group)
                .bind(key.quota_limit)
                .bind(&key.quota_group)
                .bind(key.used_tokens)
                .bind(key.rpm_limit)
                .bind(key.tpm_limit)
                .bind(key.is_active as i8)
                .bind(key.expires_at)
                .bind(key.last_used_at)
                .bind(now)
                .bind(now)
                .bind(&key.model_limits)
                .execute(&db.pool).await?;

                Ok(result.last_insert_id() as i64)
            })
        })
    }

    pub fn update(db: &Database, key: &ApiKey) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "UPDATE api_keys SET user_id = ?, name = ?, key_hash = ?, key_prefix = ?,
                            provider_group = ?, quota_limit = ?, quota_group = ?, used_tokens = ?, rpm_limit = ?,
                            is_active = ?, expires_at = ?, last_used_at = ?, updated_at = ?, model_limits = ?
                     WHERE id = ?"
                )
                .bind(key.user_id)
                .bind(&key.name)
                .bind(&key.key_hash)
                .bind(&key.key_prefix)
                .bind(&key.provider_group)
                .bind(key.quota_limit)
                .bind(&key.quota_group)
                .bind(key.used_tokens)
                .bind(key.rpm_limit)
                .bind(key.is_active as i8)
                .bind(key.expires_at)
                .bind(key.last_used_at)
                .bind(now)
                .bind(&key.model_limits)
                .bind(key.id)
                .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM api_keys WHERE id = ?")
                    .bind(id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn increment_used_tokens(db: &Database, id: i64, tokens: i64) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE api_keys SET used_tokens = used_tokens + ?, last_used_at = ?, updated_at = ? WHERE id = ?")
                    .bind(tokens).bind(now).bind(now).bind(id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    /// Update last_used_at timestamp (called on each API key usage).
    pub fn update_last_used(db: &Database, id: i64) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE api_keys SET last_used_at = ?, updated_at = ? WHERE id = ?")
                    .bind(now).bind(now).bind(id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn set_active(db: &Database, id: i64, active: bool) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE api_keys SET is_active = ?, updated_at = ? WHERE id = ?")
                    .bind(active as i8).bind(now).bind(id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    fn row_to_api_key_mysql(row: sqlx::mysql::MySqlRow) -> ApiKey {
        use sqlx::Row;
        ApiKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            name: row.get("name"),
            key_hash: row.get("key_hash"),
            key_prefix: row.get("key_prefix"),
            key_suffix: row.get::<Option<String>, _>("key_suffix").unwrap_or_default(),
            encrypted_key: row.get("encrypted_key"),
            provider_group: row.get::<Option<String>, _>("provider_group").unwrap_or_default(),
            quota_limit: row.get("quota_limit"),
            quota_group: row.get("quota_group"),
            used_tokens: row.get("used_tokens"),
            rpm_limit: row.get("rpm_limit"),
            tpm_limit: row.get("tpm_limit"),
            model_limits: row.get("model_limits"),
            is_active: row.get::<i8, _>("is_active") != 0,
            expires_at: row.get("expires_at"),
            last_used_at: row.get("last_used_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }
}
