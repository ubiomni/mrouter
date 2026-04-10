use crate::models::Provider;

/// Authentication information for a provider
pub struct AuthInfo {
    pub strategy: AuthStrategy,
    pub token: String,
}

/// Authentication strategy for injecting credentials
pub enum AuthStrategy {
    /// x-api-key: {token}
    XApiKey,
    /// Authorization: Bearer {token}
    Bearer,
    /// x-goog-api-key: {token}
    XGoogApiKey,
    /// Custom header with optional prefix
    Custom {
        header_name: String,
        prefix: Option<String>,
    },
}

impl AuthInfo {
    /// Build AuthInfo from provider's auth_header() config
    pub fn from_provider(provider: &Provider) -> Self {
        let (header_name, header_value) = provider.auth_header();

        // Determine strategy based on header name and value pattern
        let (strategy, token) = match header_name.to_lowercase().as_str() {
            "x-api-key" => (AuthStrategy::XApiKey, provider.api_key.clone()),
            "x-goog-api-key" => (AuthStrategy::XGoogApiKey, provider.api_key.clone()),
            "authorization" => {
                // Check if the value has a "Bearer " prefix
                if let Some(token) = header_value.strip_prefix("Bearer ") {
                    (AuthStrategy::Bearer, token.to_string())
                } else {
                    // Custom prefix
                    let parts: Vec<&str> = header_value.splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        (
                            AuthStrategy::Custom {
                                header_name: header_name.clone(),
                                prefix: Some(parts[0].to_string()),
                            },
                            parts[1].to_string(),
                        )
                    } else {
                        (
                            AuthStrategy::Custom {
                                header_name: header_name.clone(),
                                prefix: None,
                            },
                            header_value,
                        )
                    }
                }
            }
            _ => {
                // Check if value contains a space (prefix + token)
                let parts: Vec<&str> = header_value.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    (
                        AuthStrategy::Custom {
                            header_name: header_name.clone(),
                            prefix: Some(parts[0].to_string()),
                        },
                        parts[1].to_string(),
                    )
                } else {
                    (
                        AuthStrategy::Custom {
                            header_name: header_name.clone(),
                            prefix: None,
                        },
                        header_value,
                    )
                }
            }
        };

        AuthInfo { strategy, token }
    }

    /// Get the header name for this auth strategy
    pub fn header_name(&self) -> &str {
        match &self.strategy {
            AuthStrategy::XApiKey => "x-api-key",
            AuthStrategy::Bearer => "authorization",
            AuthStrategy::XGoogApiKey => "x-goog-api-key",
            AuthStrategy::Custom { header_name, .. } => header_name,
        }
    }

    /// Get the full header value (with prefix if applicable)
    pub fn header_value(&self) -> String {
        match &self.strategy {
            AuthStrategy::XApiKey => self.token.clone(),
            AuthStrategy::Bearer => format!("Bearer {}", self.token),
            AuthStrategy::XGoogApiKey => self.token.clone(),
            AuthStrategy::Custom { prefix, .. } => {
                if let Some(pfx) = prefix {
                    format!("{} {}", pfx, self.token)
                } else {
                    self.token.clone()
                }
            }
        }
    }
}
