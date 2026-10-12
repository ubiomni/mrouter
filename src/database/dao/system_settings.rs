// System Settings DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::SystemSetting;
use chrono::Utc;

pub struct SystemSettingsDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl SystemSettingsDao {
    pub fn get(db: &Database, key: &str) -> Result<Option<SystemSetting>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, value, description, updated_at
             FROM system_settings WHERE key = ?1"
        )?;

        let mut rows = stmt.query(rusqlite::params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_setting(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_all(db: &Database) -> Result<Vec<SystemSetting>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, value, description, updated_at
             FROM system_settings ORDER BY key"
        )?;

        let rows = stmt.query_map([], |row| Self::row_to_setting(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Insert or update a setting (upsert)
    pub fn set(db: &Database, setting: &SystemSetting) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let value_json = serde_json::to_string(&setting.value).unwrap_or_default();

        db.execute(
            "INSERT INTO system_settings (key, value, description, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET value = ?2, description = ?3, updated_at = ?4",
            rusqlite::params![
                setting.key,
                value_json,
                setting.description,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, key: &str) -> Result<()> {
        db.execute("DELETE FROM system_settings WHERE key = ?1", rusqlite::params![key])?;
        Ok(())
    }

    // --- private helpers ---

    fn row_to_setting(row: &rusqlite::Row<'_>) -> rusqlite::Result<SystemSetting> {
        let value_str: String = row.get(1)?;
        let value: serde_json::Value = serde_json::from_str(&value_str)
            .unwrap_or(serde_json::Value::String(value_str));

        Ok(SystemSetting {
            key: row.get(0)?,
            value,
            description: row.get(2)?,
            updated_at: row.get(3)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl SystemSettingsDao {
    pub fn get(db: &Database, key: &str) -> Result<Option<SystemSetting>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_as::<_, (String, String, Option<String>, i64)>(
                    "SELECT `key`, value, description, updated_at
                     FROM system_settings WHERE `key` = ?"
                ).bind(key).fetch_optional(&db.pool).await?;

                Ok(row.map(|r| Self::tuple_to_setting(r)))
            })
        })
    }

    pub fn list_all(db: &Database) -> Result<Vec<SystemSetting>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, (String, String, Option<String>, i64)>(
                    "SELECT `key`, value, description, updated_at
                     FROM system_settings ORDER BY `key`"
                ).fetch_all(&db.pool).await?;

                Ok(rows.into_iter().map(|r| Self::tuple_to_setting(r)).collect())
            })
        })
    }

    pub fn set(db: &Database, setting: &SystemSetting) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let value_json = serde_json::to_string(&setting.value).unwrap_or_default();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut tx = db.pool.begin().await?;

                let exists: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM system_settings WHERE `key` = ?"
                ).bind(&setting.key).fetch_one(&mut *tx).await?;

                if exists.0 > 0 {
                    sqlx::query(
                        "UPDATE system_settings SET value = ?, description = ?, updated_at = ? WHERE `key` = ?"
                    )
                    .bind(&value_json)
                    .bind(&setting.description)
                    .bind(now)
                    .bind(&setting.key)
                    .execute(&mut *tx).await?;
                } else {
                    sqlx::query(
                        "INSERT INTO system_settings (`key`, value, description, updated_at) VALUES (?, ?, ?, ?)"
                    )
                    .bind(&setting.key)
                    .bind(&value_json)
                    .bind(&setting.description)
                    .bind(now)
                    .execute(&mut *tx).await?;
                }

                tx.commit().await?;
                Ok(())
            })
        })
    }

    pub fn delete(db: &Database, key: &str) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM system_settings WHERE `key` = ?")
                    .bind(key)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    fn tuple_to_setting(r: (String, String, Option<String>, i64)) -> SystemSetting {
        let value: serde_json::Value = serde_json::from_str(&r.1)
            .unwrap_or(serde_json::Value::String(r.1));
        SystemSetting {
            key: r.0,
            value,
            description: r.2,
            updated_at: r.3,
        }
    }
}
