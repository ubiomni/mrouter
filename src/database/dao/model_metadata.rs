// Model Metadata DAO — models_metadata 表

use anyhow::Result;
use crate::database::Database;
use crate::models::ModelMetadata;

pub struct ModelMetadataDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl ModelMetadataDao {
    pub fn get(db: &Database, org_id: i64, model_name: &str) -> Result<Option<ModelMetadata>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
             FROM models_metadata WHERE org_id = ?1 AND model_name = ?2"
        )?;

        let mut rows = stmt.query(rusqlite::params![org_id, model_name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_metadata(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_all(db: &Database) -> Result<Vec<ModelMetadata>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
             FROM models_metadata ORDER BY sort_order, model_name"
        )?;

        let rows = stmt.query_map([], |row| Self::row_to_metadata(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_by_org(db: &Database, org_id: i64) -> Result<Vec<ModelMetadata>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
             FROM models_metadata WHERE org_id = ?1 ORDER BY sort_order, model_name"
        )?;

        let rows = stmt.query_map(rusqlite::params![org_id], |row| Self::row_to_metadata(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_by_vendor(db: &Database, org_id: i64, vendor: &str) -> Result<Vec<ModelMetadata>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
             FROM models_metadata WHERE org_id = ?1 AND vendor = ?2 ORDER BY sort_order, model_name"
        )?;

        let rows = stmt.query_map(rusqlite::params![org_id, vendor], |row| Self::row_to_metadata(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Insert or update model metadata (upsert by org_id + model_name)
    pub fn upsert(db: &Database, meta: &ModelMetadata) -> Result<()> {
        let capabilities_json = serde_json::to_string(&meta.capabilities).unwrap_or_else(|_| "[]".to_string());

        db.execute(
            "INSERT INTO models_metadata (org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(org_id, model_name) DO UPDATE SET
                display_name = ?3, vendor = ?4, model_group = ?5, icon = ?6, tags = ?7,
                capabilities = ?8, is_visible = ?9, sort_order = ?10",
            rusqlite::params![
                meta.org_id,
                meta.model_name,
                meta.display_name,
                meta.vendor,
                meta.model_group,
                meta.icon,
                meta.tags,
                capabilities_json,
                meta.is_visible as i32,
                meta.sort_order,
            ],
        )?;

        Ok(())
    }

    pub fn delete(db: &Database, org_id: i64, model_name: &str) -> Result<()> {
        db.execute(
            "DELETE FROM models_metadata WHERE org_id = ?1 AND model_name = ?2",
            rusqlite::params![org_id, model_name],
        )?;
        Ok(())
    }

    // --- private helpers ---

    fn row_to_metadata(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelMetadata> {
        let capabilities: Vec<String> = row.get::<_, Option<String>>(7)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Ok(ModelMetadata {
            org_id: row.get(0)?,
            model_name: row.get(1)?,
            display_name: row.get(2)?,
            vendor: row.get(3)?,
            model_group: row.get(4)?,
            icon: row.get(5)?,
            tags: row.get(6)?,
            capabilities,
            is_visible: row.get::<_, i32>(8)? != 0,
            sort_order: row.get(9)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl ModelMetadataDao {
    pub fn get(db: &Database, org_id: i64, model_name: &str) -> Result<Option<ModelMetadata>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i8, i32)>(
                    "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
                     FROM models_metadata WHERE org_id = ? AND model_name = ?"
                ).bind(org_id).bind(model_name).fetch_optional(&db.pool).await?;

                Ok(row.map(|r| Self::tuple_to_metadata(r)))
            })
        })
    }

    pub fn list_all(db: &Database) -> Result<Vec<ModelMetadata>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i8, i32)>(
                    "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
                     FROM models_metadata ORDER BY sort_order, model_name"
                ).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_metadata(r)).collect())
            })
        })
    }

    pub fn list_by_org(db: &Database, org_id: i64) -> Result<Vec<ModelMetadata>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i8, i32)>(
                    "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
                     FROM models_metadata WHERE org_id = ? ORDER BY sort_order, model_name"
                ).bind(org_id).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_metadata(r)).collect())
            })
        })
    }

    pub fn list_by_vendor(db: &Database, org_id: i64, vendor: &str) -> Result<Vec<ModelMetadata>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i8, i32)>(
                    "SELECT org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order
                     FROM models_metadata WHERE org_id = ? AND vendor = ? ORDER BY sort_order, model_name"
                ).bind(org_id).bind(vendor).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_metadata(r)).collect())
            })
        })
    }

    pub fn upsert(db: &Database, meta: &ModelMetadata) -> Result<()> {
        let capabilities_json = serde_json::to_string(&meta.capabilities).unwrap_or_else(|_| "[]".to_string());

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut tx = db.pool.begin().await?;

                let exists: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM models_metadata WHERE org_id = ? AND model_name = ?"
                ).bind(meta.org_id).bind(&meta.model_name).fetch_one(&mut *tx).await?;

                if exists.0 > 0 {
                    sqlx::query(
                        "UPDATE models_metadata SET display_name = ?, vendor = ?, model_group = ?,
                                icon = ?, tags = ?, capabilities = ?, is_visible = ?, sort_order = ?
                         WHERE org_id = ? AND model_name = ?"
                    )
                    .bind(&meta.display_name)
                    .bind(&meta.vendor)
                    .bind(&meta.model_group)
                    .bind(&meta.icon)
                    .bind(&meta.tags)
                    .bind(&capabilities_json)
                    .bind(meta.is_visible as i8)
                    .bind(meta.sort_order)
                    .bind(meta.org_id)
                    .bind(&meta.model_name)
                    .execute(&mut *tx).await?;
                } else {
                    sqlx::query(
                        "INSERT INTO models_metadata (org_id, model_name, display_name, vendor, model_group, icon, tags, capabilities, is_visible, sort_order)
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                    )
                    .bind(meta.org_id)
                    .bind(&meta.model_name)
                    .bind(&meta.display_name)
                    .bind(&meta.vendor)
                    .bind(&meta.model_group)
                    .bind(&meta.icon)
                    .bind(&meta.tags)
                    .bind(&capabilities_json)
                    .bind(meta.is_visible as i8)
                    .bind(meta.sort_order)
                    .execute(&mut *tx).await?;
                }

                tx.commit().await?;
                Ok(())
            })
        })
    }

    pub fn delete(db: &Database, org_id: i64, model_name: &str) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM models_metadata WHERE org_id = ? AND model_name = ?")
                    .bind(org_id).bind(model_name)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    fn tuple_to_metadata(r: (i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i8, i32)) -> ModelMetadata {
        let capabilities: Vec<String> = r.7
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        ModelMetadata {
            org_id: r.0,
            model_name: r.1,
            display_name: r.2,
            vendor: r.3,
            model_group: r.4,
            icon: r.5,
            tags: r.6,
            capabilities,
            is_visible: r.8 != 0,
            sort_order: r.9,
        }
    }
}
