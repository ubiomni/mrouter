// Organization DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::Organization;
use chrono::Utc;

pub struct OrganizationDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl OrganizationDao {
    fn cols() -> &'static str {
        "id, name, display_name, is_active, settings, created_at, updated_at"
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Organization>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM organizations WHERE id = ?1", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        Ok(rows.next()?.map(|r| Self::row_to_org(r)).transpose()?)
    }

    pub fn get_default(db: &Database) -> Result<Option<Organization>> {
        Self::get_by_id(db, 1)
    }

    pub fn list_all(db: &Database) -> Result<Vec<Organization>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM organizations ORDER BY id", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| Self::row_to_org(r))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn insert(db: &Database, org: &Organization) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        let settings_json = org.settings.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        db.execute(
            "INSERT INTO organizations (name, display_name, is_active, settings, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![org.name, org.display_name, org.is_active as i32, settings_json, now, now],
        )?;
        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, org: &Organization) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let settings_json = org.settings.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        db.execute(
            "UPDATE organizations SET name=?1, display_name=?2, is_active=?3, settings=?4, updated_at=?5 WHERE id=?6",
            rusqlite::params![org.name, org.display_name, org.is_active as i32, settings_json, now, org.id],
        )?;
        Ok(())
    }

    fn row_to_org(row: &rusqlite::Row) -> rusqlite::Result<Organization> {
        let settings_str: Option<String> = row.get(4)?;
        let settings: Option<serde_json::Value> = settings_str.and_then(|s| serde_json::from_str(&s).ok());
        Ok(Organization {
            id: row.get(0)?, name: row.get(1)?, display_name: row.get(2)?,
            is_active: row.get::<_, bool>(3)?, settings,
            created_at: row.get(5)?, updated_at: row.get(6)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl OrganizationDao {
    fn cols() -> &'static str {
        "id, name, display_name, is_active, settings, created_at, updated_at"
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Organization>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM organizations WHERE id = ?", Self::cols());
                let row = sqlx::query_as::<_, (i64,String,Option<String>,i8,Option<String>,i64,i64)>(&sql)
                    .bind(id).fetch_optional(&db.pool).await?;
                Ok(row.map(Self::tuple_to_org))
            })
        })
    }

    pub fn get_default(db: &Database) -> Result<Option<Organization>> { Self::get_by_id(db, 1) }

    pub fn list_all(db: &Database) -> Result<Vec<Organization>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM organizations ORDER BY id", Self::cols());
                let rows = sqlx::query_as::<_, (i64,String,Option<String>,i8,Option<String>,i64,i64)>(&sql)
                    .fetch_all(&db.pool).await?;
                Ok(rows.into_iter().map(Self::tuple_to_org).collect())
            })
        })
    }

    pub fn insert(db: &Database, org: &Organization) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        let settings_json = org.settings.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query("INSERT INTO organizations (name,display_name,is_active,settings,created_at,updated_at) VALUES (?,?,?,?,?,?)")
                    .bind(&org.name).bind(&org.display_name).bind(org.is_active as i8)
                    .bind(&settings_json).bind(now).bind(now)
                    .execute(&db.pool).await?;
                Ok(result.last_insert_id() as i64)
            })
        })
    }

    pub fn update(db: &Database, org: &Organization) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let settings_json = org.settings.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE organizations SET name=?,display_name=?,is_active=?,settings=?,updated_at=? WHERE id=?")
                    .bind(&org.name).bind(&org.display_name).bind(org.is_active as i8)
                    .bind(&settings_json).bind(now).bind(org.id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    fn tuple_to_org(r: (i64,String,Option<String>,i8,Option<String>,i64,i64)) -> Organization {
        let settings: Option<serde_json::Value> = r.4.and_then(|s| serde_json::from_str(&s).ok());
        Organization {
            id: r.0, name: r.1, display_name: r.2, is_active: r.3 != 0, settings,
            created_at: r.5, updated_at: r.6,
        }
    }
}
