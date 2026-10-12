// User DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::{User, UserRole};
use chrono::Utc;

pub struct UserDao;

// ============================================================
//  SQLite implementation (synchronous)
// ============================================================
#[cfg(feature = "sqlite")]
impl UserDao {
    fn cols() -> &'static str {
        "id, org_id, username, password_hash, display_name, email, role, provider_group, is_active, last_login_at, created_at, updated_at"
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<User>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM users WHERE id = ?1", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        Ok(rows.next()?.map(|r| Self::row_to_user(r)).transpose()?)
    }

    pub fn get_by_username(db: &Database, org_id: i64, username: &str) -> Result<Option<User>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM users WHERE org_id = ?1 AND username = ?2", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params![org_id, username])?;
        Ok(rows.next()?.map(|r| Self::row_to_user(r)).transpose()?)
    }

    pub fn get_by_username_any(db: &Database, username: &str) -> Result<Vec<User>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM users WHERE username = ?1", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![username], |r| Self::row_to_user(r))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_by_org(db: &Database, org_id: i64) -> Result<Vec<User>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM users WHERE org_id = ?1 ORDER BY username", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![org_id], |r| Self::row_to_user(r))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_all(db: &Database) -> Result<Vec<User>> {
        let conn = db.conn.lock().unwrap();
        let sql = format!("SELECT {} FROM users ORDER BY username", Self::cols());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| Self::row_to_user(r))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn insert(db: &Database, user: &User) -> Result<i64> {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (org_id, username, password_hash, display_name, email, role, provider_group, is_active, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![user.org_id, user.username, user.password_hash, user.display_name, user.email, user.role.to_string(), user.provider_group, user.is_active, user.created_at, user.updated_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, user: &User) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        let now = Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE users SET org_id=?1, username=?2, display_name=?3, email=?4, role=?5, provider_group=?6, is_active=?7, updated_at=?8 WHERE id=?9",
            rusqlite::params![user.org_id, user.username, user.display_name, user.email, user.role.to_string(), user.provider_group, user.is_active, now, user.id],
        )?;
        Ok(())
    }

    pub fn update_password(db: &Database, id: i64, password_hash: &str) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        let now = Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![password_hash, now, id],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        conn.execute("DELETE FROM users WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    pub fn update_last_login(db: &Database, id: i64) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        let now = Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE users SET last_login_at = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![now, now, id],
        )?;
        Ok(())
    }

    fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<User> {
        let role_str: String = row.get(6)?;
        let role = role_str.parse::<UserRole>().unwrap_or(UserRole::User);
        Ok(User {
            id: row.get(0)?, org_id: row.get(1)?,
            username: row.get(2)?, password_hash: row.get(3)?,
            display_name: row.get(4)?, email: row.get(5)?,
            role, provider_group: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
            is_active: row.get(8)?, last_login_at: row.get(9)?,
            created_at: row.get(10)?, updated_at: row.get(11)?,
        })
    }
}

// ============================================================
//  MySQL implementation (async via block_in_place)
// ============================================================
#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl UserDao {
    fn cols() -> &'static str {
        "id, org_id, username, password_hash, display_name, email, role, provider_group, is_active, last_login_at, created_at, updated_at"
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<User>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM users WHERE id = ?", Self::cols());
                let row = sqlx::query_as::<_, (i64,i64,String,String,Option<String>,Option<String>,String,Option<String>,i8,Option<i64>,i64,i64)>(&sql)
                    .bind(id).fetch_optional(&db.pool).await?;
                Ok(row.map(Self::tuple_to_user))
            })
        })
    }

    pub fn get_by_username(db: &Database, org_id: i64, username: &str) -> Result<Option<User>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM users WHERE org_id = ? AND username = ?", Self::cols());
                let row = sqlx::query_as::<_, (i64,i64,String,String,Option<String>,Option<String>,String,Option<String>,i8,Option<i64>,i64,i64)>(&sql)
                    .bind(org_id).bind(username).fetch_optional(&db.pool).await?;
                Ok(row.map(Self::tuple_to_user))
            })
        })
    }

    pub fn get_by_username_any(db: &Database, username: &str) -> Result<Vec<User>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM users WHERE username = ?", Self::cols());
                let rows = sqlx::query_as::<_, (i64,i64,String,String,Option<String>,Option<String>,String,Option<String>,i8,Option<i64>,i64,i64)>(&sql)
                    .bind(username).fetch_all(&db.pool).await?;
                Ok(rows.into_iter().map(Self::tuple_to_user).collect())
            })
        })
    }

    pub fn list_by_org(db: &Database, org_id: i64) -> Result<Vec<User>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM users WHERE org_id = ? ORDER BY username", Self::cols());
                let rows = sqlx::query_as::<_, (i64,i64,String,String,Option<String>,Option<String>,String,Option<String>,i8,Option<i64>,i64,i64)>(&sql)
                    .bind(org_id).fetch_all(&db.pool).await?;
                Ok(rows.into_iter().map(Self::tuple_to_user).collect())
            })
        })
    }

    pub fn list_all(db: &Database) -> Result<Vec<User>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let sql = format!("SELECT {} FROM users ORDER BY username", Self::cols());
                let rows = sqlx::query_as::<_, (i64,i64,String,String,Option<String>,Option<String>,String,Option<String>,i8,Option<i64>,i64,i64)>(&sql)
                    .fetch_all(&db.pool).await?;
                Ok(rows.into_iter().map(Self::tuple_to_user).collect())
            })
        })
    }

    pub fn insert(db: &Database, user: &User) -> Result<i64> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query("INSERT INTO users (org_id, username, password_hash, display_name, email, role, provider_group, is_active, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
                    .bind(user.org_id).bind(&user.username).bind(&user.password_hash)
                    .bind(&user.display_name).bind(&user.email).bind(user.role.to_string())
                    .bind(&user.provider_group).bind(user.is_active as i8)
                    .bind(user.created_at).bind(user.updated_at)
                    .execute(&db.pool).await?;
                Ok(result.last_insert_id() as i64)
            })
        })
    }

    pub fn update(db: &Database, user: &User) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE users SET org_id=?, username=?, display_name=?, email=?, role=?, provider_group=?, is_active=?, updated_at=? WHERE id=?")
                    .bind(user.org_id).bind(&user.username).bind(&user.display_name).bind(&user.email)
                    .bind(user.role.to_string()).bind(&user.provider_group).bind(user.is_active as i8)
                    .bind(now).bind(user.id)
                    .execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn update_password(db: &Database, id: i64, password_hash: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                    .bind(password_hash).bind(now).bind(id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM users WHERE id = ?").bind(id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    pub fn update_last_login(db: &Database, id: i64) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?")
                    .bind(now).bind(now).bind(id).execute(&db.pool).await?;
                Ok(())
            })
        })
    }

    fn tuple_to_user(r: (i64,i64,String,String,Option<String>,Option<String>,String,Option<String>,i8,Option<i64>,i64,i64)) -> User {
        let role = r.6.parse::<UserRole>().unwrap_or(UserRole::User);
        User {
            id: r.0, org_id: r.1, username: r.2, password_hash: r.3,
            display_name: r.4, email: r.5, role, provider_group: r.7.unwrap_or_default(),
            is_active: r.8 != 0, last_login_at: r.9, created_at: r.10, updated_at: r.11,
        }
    }
}
