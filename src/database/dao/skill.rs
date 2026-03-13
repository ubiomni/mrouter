// Skill DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::{Skill, SkillRepo};

pub struct SkillDao;

impl SkillDao {
    pub fn get_all(db: &Database) -> Result<Vec<Skill>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, repo_id, path, description,
                    enabled_claude, enabled_codex, enabled_gemini, enabled_opencode
             FROM skills ORDER BY name"
        )?;

        let skills = stmt.query_map([], |row| {
            Ok(Skill {
                id: row.get(0)?,
                name: row.get(1)?,
                repo_id: row.get(2)?,
                path: row.get(3)?,
                description: row.get(4)?,
                enabled_claude: row.get::<_, i32>(5)? != 0,
                enabled_codex: row.get::<_, i32>(6)? != 0,
                enabled_gemini: row.get::<_, i32>(7)? != 0,
                enabled_opencode: row.get::<_, i32>(8)? != 0,
            })
        })?;

        let mut result = Vec::new();
        for skill in skills {
            result.push(skill?);
        }
        Ok(result)
    }

    pub fn get_by_repo(db: &Database, repo_id: i64) -> Result<Vec<Skill>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, repo_id, path, description,
                    enabled_claude, enabled_codex, enabled_gemini, enabled_opencode
             FROM skills WHERE repo_id = ?1 ORDER BY name"
        )?;

        let skills = stmt.query_map([repo_id], |row| {
            Ok(Skill {
                id: row.get(0)?,
                name: row.get(1)?,
                repo_id: row.get(2)?,
                path: row.get(3)?,
                description: row.get(4)?,
                enabled_claude: row.get::<_, i32>(5)? != 0,
                enabled_codex: row.get::<_, i32>(6)? != 0,
                enabled_gemini: row.get::<_, i32>(7)? != 0,
                enabled_opencode: row.get::<_, i32>(8)? != 0,
            })
        })?;

        let mut result = Vec::new();
        for skill in skills {
            result.push(skill?);
        }
        Ok(result)
    }

    pub fn insert(db: &Database, skill: &Skill) -> Result<i64> {
        db.execute(
            "INSERT INTO skills (name, repo_id, path, description, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                skill.name,
                skill.repo_id,
                skill.path,
                skill.description,
                if skill.enabled_claude { 1 } else { 0 },
                if skill.enabled_codex { 1 } else { 0 },
                if skill.enabled_gemini { 1 } else { 0 },
                if skill.enabled_opencode { 1 } else { 0 },
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, skill: &Skill) -> Result<()> {
        db.execute(
            "UPDATE skills SET name = ?1, description = ?2,
             enabled_claude = ?3, enabled_codex = ?4, enabled_gemini = ?5, enabled_opencode = ?6
             WHERE id = ?7",
            rusqlite::params![
                skill.name,
                skill.description,
                if skill.enabled_claude { 1 } else { 0 },
                if skill.enabled_codex { 1 } else { 0 },
                if skill.enabled_gemini { 1 } else { 0 },
                if skill.enabled_opencode { 1 } else { 0 },
                skill.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        db.execute("DELETE FROM skills WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn delete_by_repo(db: &Database, repo_id: i64) -> Result<()> {
        db.execute("DELETE FROM skills WHERE repo_id = ?1", [repo_id])?;
        Ok(())
    }
}

pub struct SkillRepoDao;

impl SkillRepoDao {
    pub fn get_all(db: &Database) -> Result<Vec<SkillRepo>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, branch, local_path, last_synced
             FROM skill_repos ORDER BY name"
        )?;

        let repos = stmt.query_map([], |row| {
            Ok(SkillRepo {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                branch: row.get(3)?,
                local_path: row.get(4)?,
                last_synced: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for repo in repos {
            result.push(repo?);
        }
        Ok(result)
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<SkillRepo>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, branch, local_path, last_synced
             FROM skill_repos WHERE id = ?1"
        )?;

        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SkillRepo {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                branch: row.get(3)?,
                local_path: row.get(4)?,
                last_synced: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert(db: &Database, repo: &SkillRepo) -> Result<i64> {
        db.execute(
            "INSERT INTO skill_repos (name, url, branch, local_path, last_synced)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                repo.name,
                repo.url,
                repo.branch,
                repo.local_path,
                repo.last_synced,
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update_synced(db: &Database, id: i64) -> Result<()> {
        db.execute(
            "UPDATE skill_repos SET last_synced = datetime('now') WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        // 先删除关联的 skills
        SkillDao::delete_by_repo(db, id)?;
        db.execute("DELETE FROM skill_repos WHERE id = ?1", [id])?;
        Ok(())
    }
}
