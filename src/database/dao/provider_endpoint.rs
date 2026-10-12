use crate::database::Database;
use crate::models::provider_endpoint::ProviderEndpoint;
use anyhow::Result;

#[cfg(feature = "sqlite")]
use rusqlite::OptionalExtension;

pub struct ProviderEndpointDao;

#[cfg(feature = "sqlite")]
impl ProviderEndpointDao {
    /// Get endpoint by provider_id and endpoint type
    pub fn get_by_provider_and_type(
        db: &Database,
        provider_id: i64,
        endpoint_type: &str,
    ) -> Result<Option<ProviderEndpoint>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider_id, endpoint_type, api_format, endpoint_path, http_method,
             required_params, optional_params, auth_location, custom_headers, enabled,
             created_at, updated_at
             FROM provider_endpoints
             WHERE provider_id = ?1 AND endpoint_type = ?2 AND enabled = 1"
        )?;

        let endpoint = stmt.query_row(
            rusqlite::params![provider_id, endpoint_type],
            |row| Self::row_to_endpoint(row)
        ).optional()?;

        Ok(endpoint)
    }

    /// Get best endpoint for provider — prefer matching api_format, fallback to any
    pub fn get_best_endpoint(
        db: &Database,
        provider_id: i64,
        endpoint_type: &str,
        preferred_format: &str,
    ) -> Result<Option<ProviderEndpoint>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider_id, endpoint_type, api_format, endpoint_path, http_method,
             required_params, optional_params, auth_location, custom_headers, enabled,
             created_at, updated_at
             FROM provider_endpoints
             WHERE provider_id = ?1 AND endpoint_type = ?2 AND enabled = 1
             ORDER BY (LOWER(api_format) = LOWER(?3)) DESC
             LIMIT 1"
        )?;

        let endpoint = stmt.query_row(
            rusqlite::params![provider_id, endpoint_type, preferred_format],
            |row| Self::row_to_endpoint(row)
        ).optional()?;

        Ok(endpoint)
    }

    /// Get all endpoints for a provider
    pub fn list_by_provider(db: &Database, provider_id: i64) -> Result<Vec<ProviderEndpoint>> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider_id, endpoint_type, api_format, endpoint_path, http_method,
             required_params, optional_params, auth_location, custom_headers, enabled,
             created_at, updated_at
             FROM provider_endpoints
             WHERE provider_id = ?1 AND enabled = 1"
        )?;

        let endpoints = stmt.query_map(rusqlite::params![provider_id], |row| {
            Self::row_to_endpoint(row)
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(endpoints)
    }

    /// Insert a new endpoint
    pub fn insert(db: &Database, endpoint: &ProviderEndpoint) -> Result<()> {
        let conn = db.conn.lock().unwrap();

        let required_params_json = endpoint.required_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let optional_params_json = endpoint.optional_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let custom_headers_json = endpoint.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();

        conn.execute(
            "INSERT INTO provider_endpoints
             (provider_id, endpoint_type, api_format, endpoint_path, http_method,
              required_params, optional_params, auth_location, custom_headers, enabled,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                endpoint.provider_id,
                endpoint.endpoint_type,
                endpoint.api_format,
                endpoint.endpoint_path,
                endpoint.http_method,
                required_params_json,
                optional_params_json,
                endpoint.auth_location,
                custom_headers_json,
                endpoint.enabled,
                endpoint.created_at,
                endpoint.updated_at,
            ],
        )?;

        Ok(())
    }

    /// Update an endpoint
    pub fn update(db: &Database, endpoint: &ProviderEndpoint) -> Result<()> {
        let conn = db.conn.lock().unwrap();

        let required_params_json = endpoint.required_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let optional_params_json = endpoint.optional_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let custom_headers_json = endpoint.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();

        conn.execute(
            "UPDATE provider_endpoints
             SET api_format = ?1, endpoint_path = ?2, http_method = ?3,
                 required_params = ?4, optional_params = ?5, auth_location = ?6,
                 custom_headers = ?7, enabled = ?8, updated_at = ?9
             WHERE provider_id = ?10 AND endpoint_type = ?11",
            rusqlite::params![
                endpoint.api_format,
                endpoint.endpoint_path,
                endpoint.http_method,
                required_params_json,
                optional_params_json,
                endpoint.auth_location,
                custom_headers_json,
                endpoint.enabled,
                endpoint.updated_at,
                endpoint.provider_id,
                endpoint.endpoint_type,
            ],
        )?;

        Ok(())
    }

    /// Delete an endpoint
    pub fn delete(db: &Database, provider_id: i64, endpoint_type: &str) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM provider_endpoints WHERE provider_id = ?1 AND endpoint_type = ?2",
            rusqlite::params![provider_id, endpoint_type],
        )?;
        Ok(())
    }

    /// Delete an endpoint by its id
    pub fn delete_by_id(db: &Database, id: i64) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM provider_endpoints WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    /// Delete all endpoints for a provider
    pub fn delete_by_provider(db: &Database, provider_id: i64) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM provider_endpoints WHERE provider_id = ?1",
            rusqlite::params![provider_id],
        )?;
        Ok(())
    }

    /// Update an endpoint by id from JSON body
    pub fn update_from_json(db: &Database, id: i64, body: &serde_json::Value) -> Result<()> {
        let conn = db.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();

        let mut sets = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(v) = body.get("api_format").or(body.get("format")).and_then(|v| v.as_str()) {
            sets.push("api_format = ?");
            values.push(Box::new(v.to_string()));
        }
        if let Some(v) = body.get("endpoint_path").or(body.get("path")).and_then(|v| v.as_str()) {
            sets.push("endpoint_path = ?");
            values.push(Box::new(v.to_string()));
        }
        if let Some(v) = body.get("endpoint_type").and_then(|v| v.as_str()) {
            sets.push("endpoint_type = ?");
            values.push(Box::new(v.to_string()));
        }
        if let Some(v) = body.get("enabled").and_then(|v| v.as_bool()) {
            sets.push("enabled = ?");
            values.push(Box::new(v as i32));
        }

        if sets.is_empty() {
            return Ok(());
        }

        sets.push("updated_at = ?");
        values.push(Box::new(now));
        values.push(Box::new(id));

        let sql = format!("UPDATE provider_endpoints SET {} WHERE id = ?", sets.join(", "));
        conn.execute(&sql, rusqlite::params_from_iter(values.iter().map(|v| v.as_ref())))?;
        Ok(())
    }

    fn row_to_endpoint(row: &rusqlite::Row<'_>) -> Result<ProviderEndpoint, rusqlite::Error> {
        let required_params: Option<serde_json::Value> = row.get::<_, Option<String>>(6)?
            .and_then(|s| serde_json::from_str(&s).ok());
        let optional_params: Option<serde_json::Value> = row.get::<_, Option<String>>(7)?
            .and_then(|s| serde_json::from_str(&s).ok());
        let custom_headers: Option<serde_json::Value> = row.get::<_, Option<String>>(9)?
            .and_then(|s| serde_json::from_str(&s).ok());

        Ok(ProviderEndpoint {
            id: row.get(0)?,
            provider_id: row.get(1)?,
            endpoint_type: row.get(2)?,
            api_format: row.get(3)?,
            endpoint_path: row.get(4)?,
            http_method: row.get(5)?,
            required_params,
            optional_params,
            auth_location: row.get(8)?,
            custom_headers,
            enabled: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }
}

#[cfg(all(feature = "mysql", not(feature = "sqlite")))]
impl ProviderEndpointDao {
    /// Get endpoint by provider_id and endpoint type
    pub fn get_by_provider_and_type(
        db: &Database,
        provider_id: i64,
        endpoint_type: &str,
    ) -> Result<Option<ProviderEndpoint>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query(
                    "SELECT id, provider_id, endpoint_type, api_format, endpoint_path, http_method,
                     required_params, optional_params, auth_location, custom_headers, enabled,
                     created_at, updated_at
                     FROM provider_endpoints
                     WHERE provider_id = ? AND endpoint_type = ? AND enabled = 1"
                )
                .bind(provider_id)
                .bind(endpoint_type)
                .map(|row: sqlx::mysql::MySqlRow| Self::row_to_endpoint_mysql(row))
                .fetch_optional(&db.pool)
                .await?;
                Ok(row)
            })
        })
    }

    /// Get best endpoint for provider — prefer matching api_format, fallback to any
    pub fn get_best_endpoint(
        db: &Database,
        provider_id: i64,
        endpoint_type: &str,
        preferred_format: &str,
    ) -> Result<Option<ProviderEndpoint>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query(
                    "SELECT id, provider_id, endpoint_type, api_format, endpoint_path, http_method,
                     required_params, optional_params, auth_location, custom_headers, enabled,
                     created_at, updated_at
                     FROM provider_endpoints
                     WHERE provider_id = ? AND endpoint_type = ? AND enabled = 1
                     ORDER BY (LOWER(api_format) = LOWER(?)) DESC
                     LIMIT 1"
                )
                .bind(provider_id)
                .bind(endpoint_type)
                .bind(preferred_format)
                .map(|row: sqlx::mysql::MySqlRow| Self::row_to_endpoint_mysql(row))
                .fetch_optional(&db.pool)
                .await?;
                Ok(row)
            })
        })
    }

    /// Get all endpoints for a provider
    pub fn list_by_provider(db: &Database, provider_id: i64) -> Result<Vec<ProviderEndpoint>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query(
                    "SELECT id, provider_id, endpoint_type, api_format, endpoint_path, http_method,
                     required_params, optional_params, auth_location, custom_headers, enabled,
                     created_at, updated_at
                     FROM provider_endpoints
                     WHERE provider_id = ? AND enabled = 1"
                )
                .bind(provider_id)
                .map(|row: sqlx::mysql::MySqlRow| Self::row_to_endpoint_mysql(row))
                .fetch_all(&db.pool)
                .await?;
                Ok(rows)
            })
        })
    }

    /// Insert a new endpoint
    pub fn insert(db: &Database, endpoint: &ProviderEndpoint) -> Result<()> {
        let required_params_json = endpoint.required_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let optional_params_json = endpoint.optional_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let custom_headers_json = endpoint.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "INSERT INTO provider_endpoints
                     (provider_id, endpoint_type, api_format, endpoint_path, http_method,
                      required_params, optional_params, auth_location, custom_headers, enabled,
                      created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(endpoint.provider_id)
                .bind(&endpoint.endpoint_type)
                .bind(&endpoint.api_format)
                .bind(&endpoint.endpoint_path)
                .bind(&endpoint.http_method)
                .bind(&required_params_json)
                .bind(&optional_params_json)
                .bind(&endpoint.auth_location)
                .bind(&custom_headers_json)
                .bind(endpoint.enabled as i8)
                .bind(endpoint.created_at)
                .bind(endpoint.updated_at)
                .execute(&db.pool)
                .await?;
                Ok(())
            })
        })
    }

    /// Update an endpoint
    pub fn update(db: &Database, endpoint: &ProviderEndpoint) -> Result<()> {
        let required_params_json = endpoint.required_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let optional_params_json = endpoint.optional_params.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();
        let custom_headers_json = endpoint.custom_headers.as_ref()
            .map(|v| serde_json::to_string(v).ok())
            .flatten();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "UPDATE provider_endpoints
                     SET api_format = ?, endpoint_path = ?, http_method = ?,
                         required_params = ?, optional_params = ?, auth_location = ?,
                         custom_headers = ?, enabled = ?, updated_at = ?
                     WHERE provider_id = ? AND endpoint_type = ?"
                )
                .bind(&endpoint.api_format)
                .bind(&endpoint.endpoint_path)
                .bind(&endpoint.http_method)
                .bind(&required_params_json)
                .bind(&optional_params_json)
                .bind(&endpoint.auth_location)
                .bind(&custom_headers_json)
                .bind(endpoint.enabled as i8)
                .bind(endpoint.updated_at)
                .bind(endpoint.provider_id)
                .bind(&endpoint.endpoint_type)
                .execute(&db.pool)
                .await?;
                Ok(())
            })
        })
    }

    /// Delete an endpoint
    pub fn delete(db: &Database, provider_id: i64, endpoint_type: &str) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "DELETE FROM provider_endpoints WHERE provider_id = ? AND endpoint_type = ?"
                )
                .bind(provider_id)
                .bind(endpoint_type)
                .execute(&db.pool)
                .await?;
                Ok(())
            })
        })
    }

    fn row_to_endpoint_mysql(row: sqlx::mysql::MySqlRow) -> ProviderEndpoint {
        use sqlx::Row;
        let required_params: Option<serde_json::Value> = row.get::<Option<String>, _>("required_params")
            .and_then(|s| serde_json::from_str(&s).ok());
        let optional_params: Option<serde_json::Value> = row.get::<Option<String>, _>("optional_params")
            .and_then(|s| serde_json::from_str(&s).ok());
        let custom_headers: Option<serde_json::Value> = row.get::<Option<String>, _>("custom_headers")
            .and_then(|s| serde_json::from_str(&s).ok());
        let enabled: i8 = row.get("enabled");

        ProviderEndpoint {
            id: row.get("id"),
            provider_id: row.get("provider_id"),
            endpoint_type: row.get("endpoint_type"),
            api_format: row.get("api_format"),
            endpoint_path: row.get("endpoint_path"),
            http_method: row.get("http_method"),
            required_params,
            optional_params,
            auth_location: row.get("auth_location"),
            custom_headers,
            enabled: enabled != 0,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    /// Delete an endpoint by its id
    pub fn delete_by_id(db: &Database, id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM provider_endpoints WHERE id = ?")
                    .bind(id)
                    .execute(&db.pool)
                    .await?;
                Ok(())
            })
        })
    }

    /// Delete all endpoints for a provider
    pub fn delete_by_provider(db: &Database, provider_id: i64) -> Result<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("DELETE FROM provider_endpoints WHERE provider_id = ?")
                    .bind(provider_id)
                    .execute(&db.pool)
                    .await?;
                Ok(())
            })
        })
    }

    /// Update an endpoint by id from JSON body
    pub fn update_from_json(db: &Database, id: i64, body: &serde_json::Value) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Build dynamic SET clause from provided fields
                let mut sets = Vec::new();
                let mut binds: Vec<String> = Vec::new();

                if let Some(v) = body.get("api_format").or(body.get("format")).and_then(|v| v.as_str()) {
                    sets.push("api_format = ?");
                    binds.push(v.to_string());
                }
                if let Some(v) = body.get("endpoint_path").or(body.get("path")).and_then(|v| v.as_str()) {
                    sets.push("endpoint_path = ?");
                    binds.push(v.to_string());
                }
                if let Some(v) = body.get("endpoint_type").and_then(|v| v.as_str()) {
                    sets.push("endpoint_type = ?");
                    binds.push(v.to_string());
                }
                if let Some(v) = body.get("enabled").and_then(|v| v.as_bool()) {
                    sets.push("enabled = ?");
                    binds.push(if v { "1".to_string() } else { "0".to_string() });
                }

                if sets.is_empty() {
                    return Ok(());
                }

                sets.push("updated_at = ?");
                binds.push(now.to_string());

                let sql = format!("UPDATE provider_endpoints SET {} WHERE id = ?", sets.join(", "));
                let mut query = sqlx::query(&sql);
                for b in &binds {
                    query = query.bind(b);
                }
                query = query.bind(id);
                query.execute(&db.pool).await?;
                Ok(())
            })
        })
    }
}
