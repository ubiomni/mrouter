// Provider Model DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::ProviderModel;
use chrono::Utc;

pub struct ProviderModelDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl ProviderModelDao {
    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<ProviderModel>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
             FROM provider_models WHERE id = ?1"
        )?;

        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_provider_model(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_by_provider(db: &Database, provider_id: i64) -> Result<Vec<ProviderModel>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
             FROM provider_models WHERE provider_id = ?1 ORDER BY priority DESC, model_pattern"
        )?;

        let rows = stmt.query_map(rusqlite::params![provider_id], |row| {
            Self::row_to_provider_model(row)
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_active_by_provider(db: &Database, provider_id: i64) -> Result<Vec<ProviderModel>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
             FROM provider_models WHERE provider_id = ?1 AND enabled = 1 ORDER BY priority DESC, model_pattern"
        )?;

        let rows = stmt.query_map(rusqlite::params![provider_id], |row| {
            Self::row_to_provider_model(row)
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find provider models by model pattern (across all providers in an org)
    pub fn find_by_model_pattern(db: &Database, org_id: i64, model_pattern: &str) -> Result<Vec<ProviderModel>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
             FROM provider_models WHERE org_id = ?1 AND model_pattern = ?2 AND enabled = 1"
        )?;

        let rows = stmt.query_map(rusqlite::params![org_id, model_pattern], |row| {
            Self::row_to_provider_model(row)
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// List all enabled models for an org (for /v1/models endpoint)
    pub fn list_by_org(db: &Database, org_id: i64) -> Result<Vec<ProviderModel>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
             FROM provider_models WHERE org_id = ?1 AND enabled = 1 ORDER BY priority DESC, model_pattern"
        )?;

        let rows = stmt.query_map(rusqlite::params![org_id], |row| {
            Self::row_to_provider_model(row)
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn insert(db: &Database, model: &ProviderModel) -> Result<i64> {
        let now = Utc::now().timestamp_millis();

        db.execute(
            "INSERT INTO provider_models (org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                model.org_id,
                model.provider_id,
                model.model_pattern,
                model.upstream_model,
                model.priority,
                model.weight,
                model.enabled as i32,
                now,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, model: &ProviderModel) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        db.execute(
            "UPDATE provider_models SET org_id = ?1, provider_id = ?2, model_pattern = ?3,
                    upstream_model = ?4, priority = ?5, weight = ?6, enabled = ?7, updated_at = ?8
             WHERE id = ?9",
            rusqlite::params![
                model.org_id,
                model.provider_id,
                model.model_pattern,
                model.upstream_model,
                model.priority,
                model.weight,
                model.enabled as i32,
                now,
                model.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        db.execute("DELETE FROM provider_models WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    pub fn delete_by_provider(db: &Database, provider_id: i64) -> Result<()> {
        db.execute("DELETE FROM provider_models WHERE provider_id = ?1", rusqlite::params![provider_id])?;
        Ok(())
    }

    /// Remove duplicate (provider_id, model_pattern) entries, keeping the earliest one
    pub fn dedup(db: &Database) -> Result<u32> {
        let conn = db.conn.lock().unwrap();

        // Find duplicates
        let mut stmt = conn.prepare(
            "SELECT provider_id, model_pattern, MIN(id) as min_id
             FROM provider_models
             GROUP BY provider_id, model_pattern
             HAVING COUNT(*) > 1"
        )?;

        let duplicates: Vec<(i64, String, i64)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut count = 0u32;
        for (provider_id, model_pattern, min_id) in duplicates {
            let deleted = conn.execute(
                "DELETE FROM provider_models WHERE provider_id = ?1 AND model_pattern = ?2 AND id > ?3",
                rusqlite::params![provider_id, model_pattern, min_id]
            )?;
            count += deleted as u32;
        }

        if count > 0 {
            tracing::info!("Removed {} duplicate provider_model entries", count);
        }
        Ok(count)
    }

    // --- private helpers ---

    fn row_to_provider_model(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderModel> {
        Ok(ProviderModel {
            id: row.get(0)?,
            org_id: row.get(1)?,
            provider_id: row.get(2)?,
            model_pattern: row.get(3)?,
            upstream_model: row.get(4)?,
            priority: row.get(5)?,
            weight: row.get(6)?,
            enabled: row.get::<_, i32>(7)? != 0,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl ProviderModelDao {
    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<ProviderModel>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_as::<_, (i64, i64, i64, String, Option<String>, i32, i32, i8, i64, i64)>(
                    "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
                     FROM provider_models WHERE id = ?"
                ).bind(id).fetch_optional(&db.pool).await?;

                Ok(row.map(|r| Self::tuple_to_pm(r)))
            })
        })
    }

    pub fn list_by_provider(db: &Database, provider_id: i64) -> Result<Vec<ProviderModel>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, i64, i64, String, Option<String>, i32, i32, i8, i64, i64)>(
                    "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
                     FROM provider_models WHERE provider_id = ? ORDER BY priority DESC, model_pattern"
                ).bind(provider_id).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_pm(r)).collect())
            })
        })
    }

    pub fn list_active_by_provider(db: &Database, provider_id: i64) -> Result<Vec<ProviderModel>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, i64, i64, String, Option<String>, i32, i32, i8, i64, i64)>(
                    "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
                     FROM provider_models WHERE provider_id = ? AND enabled = 1 ORDER BY priority DESC, model_pattern"
                ).bind(provider_id).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_pm(r)).collect())
            })
        })
    }

    pub fn find_by_model_pattern(db: &Database, org_id: i64, model_pattern: &str) -> Result<Vec<ProviderModel>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, i64, i64, String, Option<String>, i32, i32, i8, i64, i64)>(
                    "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
                     FROM provider_models WHERE org_id = ? AND model_pattern = ? AND enabled = 1"
                ).bind(org_id).bind(model_pattern).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_pm(r)).collect())
            })
        })
    }

    pub fn list_by_org(db: &Database, org_id: i64) -> Result<Vec<ProviderModel>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, i64, i64, String, Option<String>, i32, i32, i8, i64, i64)>(
                    "SELECT id, org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at
                     FROM provider_models WHERE org_id = ? AND enabled = 1 ORDER BY priority DESC, model_pattern"
                ).bind(org_id).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_pm(r)).collect())
            })
        })
    }

    pub fn insert(db: &Database, model: &ProviderModel) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Check if duplicate (provider_id, model_pattern) already exists
                let existing: Option<(i64,)> = sqlx::query_as(
                    "SELECT id FROM provider_models WHERE provider_id = ? AND model_pattern = ? LIMIT 1"
                )
                .bind(model.provider_id)
                .bind(&model.model_pattern)
                .fetch_optional(&db.pool).await?;

                if let Some((existing_id,)) = existing {
                    return Ok(existing_id);
                }

                let result = sqlx::query(
                    "INSERT INTO provider_models (org_id, provider_id, model_pattern, upstream_model, priority, weight, enabled, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(model.org_id)
                .bind(model.provider_id)
                .bind(&model.model_pattern)
                .bind(&model.upstream_model)
                .bind(model.priority)
                .bind(model.weight)
                .bind(model.enabled as i8)
                .bind(now)
                .bind(now)
                .execute(&db.pool).await?;

                Ok(result.last_insert_id() as i64)
            })
        })
    }

    pub fn update(db: &Database, model: &ProviderModel) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "UPDATE provider_models SET org_id = ?, provider_id = ?, model_pattern = ?,
                            upstream_model = ?, priority = ?, weight = ?, enabled = ?, updated_at = ?
                     WHERE id = ?"
                )
                .bind(model.org_id)
                .bind(model.provider_id)
                .bind(&model.model_pattern)
                .bind(&model.upstream_model)
                .bind(model.priority)
                .bind(model.weight)
                .bind(model.enabled as i8)
                .bind(now)
                .bind(model.id)
                .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM provider_models WHERE id = ?")
                    .bind(id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn delete_by_provider(db: &Database, provider_id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM provider_models WHERE provider_id = ?")
                    .bind(provider_id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    fn tuple_to_pm(r: (i64, i64, i64, String, Option<String>, i32, i32, i8, i64, i64)) -> ProviderModel {
        ProviderModel {
            id: r.0,
            org_id: r.1,
            provider_id: r.2,
            model_pattern: r.3,
            upstream_model: r.4,
            priority: r.5,
            weight: r.6,
            enabled: r.7 != 0,
            created_at: r.8,
            updated_at: r.9,
        }
    }

    /// Remove duplicate (provider_id, model_pattern) entries, keeping the earliest one
    pub fn dedup(db: &Database) -> Result<u32> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let deleted = sqlx::query(
                    "DELETE pm1 FROM provider_models pm1
                     INNER JOIN provider_models pm2
                     WHERE pm1.provider_id = pm2.provider_id
                       AND pm1.model_pattern = pm2.model_pattern
                       AND pm1.id > pm2.id"
                )
                .execute(&db.pool).await?;

                let count = deleted.rows_affected() as u32;
                if count > 0 {
                    tracing::info!("Removed {} duplicate provider_model entries", count);
                }
                Ok(count)
            })
        })
    }
}
