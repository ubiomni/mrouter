// Provider Preset DAO - Endpoint-driven architecture

use anyhow::Result;
use crate::database::Database;
use crate::models::{ProviderPreset, PresetEndpoint, ProviderPresetFull};

pub struct ProviderPresetDao;

#[cfg(feature = "sqlite")]
impl ProviderPresetDao {
    /// List all active presets
    pub fn list_all(db: &Database) -> Result<Vec<ProviderPreset>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, preset_key, display_name, base_url, auth_method, endpoints,
                    category, description, sort_order, is_active, created_at, updated_at
             FROM provider_presets
             WHERE is_active = 1
             ORDER BY sort_order"
        )?;

        let rows = stmt.query_map([], |row| {
            let endpoints_json: String = row.get(5)?;
            let endpoints: Vec<PresetEndpoint> = serde_json::from_str(&endpoints_json)
                .unwrap_or_default();

            Ok(ProviderPreset {
                id: row.get(0)?,
                preset_key: row.get(1)?,
                display_name: row.get(2)?,
                base_url: row.get(3)?,
                auth_method: row.get(4)?,
                endpoints,
                category: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "llm".to_string()),
                description: row.get(7)?,
                sort_order: row.get(8)?,
                is_active: row.get::<_, i32>(9)? != 0,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get a single preset by key
    pub fn get_by_key(db: &Database, preset_key: &str) -> Result<Option<ProviderPreset>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, preset_key, display_name, base_url, auth_method, endpoints,
                    category, description, sort_order, is_active, created_at, updated_at
             FROM provider_presets
             WHERE preset_key = ?1 AND is_active = 1"
        )?;

        let preset = stmt.query_row(rusqlite::params![preset_key], |row| {
            let endpoints_json: String = row.get(5)?;
            let endpoints: Vec<PresetEndpoint> = serde_json::from_str(&endpoints_json)
                .unwrap_or_default();

            Ok(ProviderPreset {
                id: row.get(0)?,
                preset_key: row.get(1)?,
                display_name: row.get(2)?,
                base_url: row.get(3)?,
                auth_method: row.get(4)?,
                endpoints,
                category: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "llm".to_string()),
                description: row.get(7)?,
                sort_order: row.get(8)?,
                is_active: row.get::<_, i32>(9)? != 0,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        });

        match preset {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get preset for API response (full detail)
    pub fn get_full(db: &Database, preset_key: &str) -> Result<Option<ProviderPresetFull>> {
        let preset = Self::get_by_key(db, preset_key)?;
        Ok(preset.map(|p| ProviderPresetFull {
            preset_key: p.preset_key,
            display_name: p.display_name,
            base_url: p.base_url,
            auth_method: p.auth_method,
            endpoints: p.endpoints,
            category: p.category,
            description: p.description,
        }))
    }

    /// List all presets with full details
    pub fn list_all_full(db: &Database) -> Result<Vec<ProviderPresetFull>> {
        let presets = Self::list_all(db)?;
        Ok(presets.into_iter().map(|p| ProviderPresetFull {
            preset_key: p.preset_key,
            display_name: p.display_name,
            base_url: p.base_url,
            auth_method: p.auth_method,
            endpoints: p.endpoints,
            category: p.category,
            description: p.description,
        }).collect())
    }

    /// Insert a new preset
    pub fn insert(db: &Database, preset: &ProviderPreset) -> Result<i64> {
        let conn = db.conn.lock().unwrap();
        let endpoints_json = serde_json::to_string(&preset.endpoints)?;

        conn.execute(
            "INSERT INTO provider_presets
             (preset_key, display_name, base_url, auth_method, endpoints, description,
              sort_order, is_active, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                preset.preset_key,
                preset.display_name,
                preset.base_url,
                preset.auth_method,
                endpoints_json,
                preset.description,
                preset.sort_order,
                preset.is_active as i32,
                preset.created_at,
                preset.updated_at,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Update an existing preset
    pub fn update(db: &Database, preset: &ProviderPreset) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        let endpoints_json = serde_json::to_string(&preset.endpoints)?;

        conn.execute(
            "UPDATE provider_presets
             SET display_name = ?1, base_url = ?2, auth_method = ?3, endpoints = ?4,
                 description = ?5, sort_order = ?6, is_active = ?7, updated_at = ?8
             WHERE id = ?9",
            rusqlite::params![
                preset.display_name,
                preset.base_url,
                preset.auth_method,
                endpoints_json,
                preset.description,
                preset.sort_order,
                preset.is_active as i32,
                preset.updated_at,
                preset.id,
            ],
        )?;

        Ok(())
    }

    /// Delete a preset
    pub fn delete(db: &Database, id: i64) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        conn.execute("DELETE FROM provider_presets WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }
}

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl ProviderPresetDao {
    /// List all active presets
    pub fn list_all(db: &Database) -> Result<Vec<ProviderPreset>> {
        use sqlx::Row;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query(
                    "SELECT id, preset_key, display_name, base_url, auth_method, endpoints,
                            category, description, sort_order, is_active, created_at, updated_at
                     FROM provider_presets
                     WHERE is_active = 1
                     ORDER BY sort_order"
                ).fetch_all(&db.pool).await?;

                let mut presets = Vec::new();
                for row in rows {
                    let endpoints_val: serde_json::Value = row.get(5);
                    let endpoints: Vec<PresetEndpoint> = serde_json::from_value(endpoints_val)
                        .unwrap_or_default();

                    presets.push(ProviderPreset {
                        id: row.get(0),
                        preset_key: row.get(1),
                        display_name: row.get(2),
                        base_url: row.get(3),
                        auth_method: row.get(4),
                        endpoints,
                        category: row.get::<Option<String>, _>(6).unwrap_or_else(|| "llm".to_string()),
                        description: row.get(7),
                        sort_order: row.get(8),
                        is_active: row.get::<i8, _>(9) != 0,
                        created_at: row.get(10),
                        updated_at: row.get(11),
                    });
                }

                Ok(presets)
            })
        })
    }

    /// Get a single preset by key
    pub fn get_by_key(db: &Database, preset_key: &str) -> Result<Option<ProviderPreset>> {
        use sqlx::Row;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query(
                    "SELECT id, preset_key, display_name, base_url, auth_method, endpoints,
                            category, description, sort_order, is_active, created_at, updated_at
                     FROM provider_presets
                     WHERE preset_key = ? AND is_active = 1"
                ).bind(preset_key).fetch_optional(&db.pool).await?;

                if let Some(row) = row {
                    let endpoints_val: serde_json::Value = row.get(5);
                    let endpoints: Vec<PresetEndpoint> = serde_json::from_value(endpoints_val)
                        .unwrap_or_default();

                    Ok(Some(ProviderPreset {
                        id: row.get(0),
                        preset_key: row.get(1),
                        display_name: row.get(2),
                        base_url: row.get(3),
                        auth_method: row.get(4),
                        endpoints,
                        category: row.get::<Option<String>, _>(6).unwrap_or_else(|| "llm".to_string()),
                        description: row.get(7),
                        sort_order: row.get(8),
                        is_active: row.get::<i8, _>(9) != 0,
                        created_at: row.get(10),
                        updated_at: row.get(11),
                    }))
                } else {
                    Ok(None)
                }
            })
        })
    }

    /// Get preset for API response (full detail)
    pub fn get_full(db: &Database, preset_key: &str) -> Result<Option<ProviderPresetFull>> {
        let preset = Self::get_by_key(db, preset_key)?;
        Ok(preset.map(|p| ProviderPresetFull {
            preset_key: p.preset_key,
            display_name: p.display_name,
            base_url: p.base_url,
            auth_method: p.auth_method,
            endpoints: p.endpoints,
            category: p.category,
            description: p.description,
        }))
    }

    /// List all presets with full details
    pub fn list_all_full(db: &Database) -> Result<Vec<ProviderPresetFull>> {
        let presets = Self::list_all(db)?;
        Ok(presets.into_iter().map(|p| ProviderPresetFull {
            preset_key: p.preset_key,
            display_name: p.display_name,
            base_url: p.base_url,
            auth_method: p.auth_method,
            endpoints: p.endpoints,
            category: p.category,
            description: p.description,
        }).collect())
    }

    /// Insert a new preset
    pub fn insert(db: &Database, preset: &ProviderPreset) -> Result<i64> {
        let endpoints_json = serde_json::to_string(&preset.endpoints)?;

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query(
                    "INSERT INTO provider_presets
                     (preset_key, display_name, base_url, auth_method, endpoints, description,
                      sort_order, is_active, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(&preset.preset_key)
                .bind(&preset.display_name)
                .bind(&preset.base_url)
                .bind(&preset.auth_method)
                .bind(&endpoints_json)
                .bind(&preset.description)
                .bind(preset.sort_order)
                .bind(preset.is_active as i8)
                .bind(preset.created_at)
                .bind(preset.updated_at)
                .execute(&db.pool).await?;

                Ok(result.last_insert_id() as i64)
            })
        })
    }

    /// Update an existing preset
    pub fn update(db: &Database, preset: &ProviderPreset) -> Result<()> {
        let endpoints_json = serde_json::to_string(&preset.endpoints)?;

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "UPDATE provider_presets
                     SET display_name = ?, base_url = ?, auth_method = ?, endpoints = ?,
                         description = ?, sort_order = ?, is_active = ?, updated_at = ?
                     WHERE id = ?"
                )
                .bind(&preset.display_name)
                .bind(&preset.base_url)
                .bind(&preset.auth_method)
                .bind(&endpoints_json)
                .bind(&preset.description)
                .bind(preset.sort_order)
                .bind(preset.is_active as i8)
                .bind(preset.updated_at)
                .bind(preset.id)
                .execute(&db.pool).await?;

                Ok(())
            })
        })
    }

    /// Delete a preset
    pub fn delete(db: &Database, id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM provider_presets WHERE id = ?")
                    .bind(id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }
}
