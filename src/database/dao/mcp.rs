// MCP DAO

use anyhow::Result;
use crate::database::Database;
use crate::models::McpServer;

pub struct McpDao;

impl McpDao {
    pub fn get_all(db: &Database) -> Result<Vec<McpServer>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args, env, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode, enabled_openclaw
             FROM mcp_servers ORDER BY name"
        )?;

        let servers = stmt.query_map([], |row| {
            let args_str: String = row.get(3)?;
            let args: Vec<String> = serde_json::from_str(&args_str).unwrap_or_default();
            let env_str: Option<String> = row.get(4)?;
            let env = env_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(McpServer {
                id: row.get(0)?,
                name: row.get(1)?,
                command: row.get(2)?,
                args,
                env,
                enabled_claude: row.get::<_, i32>(5)? != 0,
                enabled_codex: row.get::<_, i32>(6)? != 0,
                enabled_gemini: row.get::<_, i32>(7)? != 0,
                enabled_opencode: row.get::<_, i32>(8)? != 0,
                enabled_openclaw: row.get::<_, i32>(9)? != 0,
            })
        })?;

        let mut result = Vec::new();
        for server in servers {
            result.push(server?);
        }
        Ok(result)
    }

    pub fn insert(db: &Database, server: &McpServer) -> Result<i64> {
        let args_str = serde_json::to_string(&server.args)?;
        let env_str = server.env.as_ref().map(|e| serde_json::to_string(e).ok()).flatten();
        
        db.execute(
            "INSERT INTO mcp_servers (name, command, args, env, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode, enabled_openclaw)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                server.name,
                server.command,
                args_str,
                env_str,
                if server.enabled_claude { 1 } else { 0 },
                if server.enabled_codex { 1 } else { 0 },
                if server.enabled_gemini { 1 } else { 0 },
                if server.enabled_opencode { 1 } else { 0 },
                if server.enabled_openclaw { 1 } else { 0 },
            ],
        )?;

        let conn = db.conn.lock().unwrap();
        Ok(conn.last_insert_rowid())
    }

    pub fn update(db: &Database, server: &McpServer) -> Result<()> {
        let args_str = serde_json::to_string(&server.args)?;
        let env_str = server.env.as_ref().map(|e| serde_json::to_string(e).ok()).flatten();
        
        db.execute(
            "UPDATE mcp_servers SET name = ?1, command = ?2, args = ?3, env = ?4, enabled_claude = ?5, enabled_codex = ?6, enabled_gemini = ?7, enabled_opencode = ?8, enabled_openclaw = ?9
             WHERE id = ?10",
            rusqlite::params![
                server.name,
                server.command,
                args_str,
                env_str,
                if server.enabled_claude { 1 } else { 0 },
                if server.enabled_codex { 1 } else { 0 },
                if server.enabled_gemini { 1 } else { 0 },
                if server.enabled_opencode { 1 } else { 0 },
                if server.enabled_openclaw { 1 } else { 0 },
                server.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Database, id: i64) -> Result<()> {
        db.execute("DELETE FROM mcp_servers WHERE id = ?1", [id])?;
        Ok(())
    }
}
