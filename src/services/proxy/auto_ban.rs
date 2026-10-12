//! 自动禁用/恢复逻辑 (对齐 new-api ShouldDisableChannel / DisableChannel / EnableChannel)

use crate::config::RetryConfig;
use crate::database::Database;
use crate::models::Provider;
use crate::services::proxy::error::ProxyError;

/// 判断是否应该自动禁用 provider
pub fn should_disable_provider(error: &ProxyError, provider: &Provider, config: &RetryConfig) -> bool {
    if !config.auto_disable_enabled {
        return false;
    }
    if !provider.auto_ban {
        return false;
    }

    // Skip-retry errors should not trigger auto-ban (aligned with new-api IsSkipRetryError)
    if should_skip_retry(error, config) {
        return false;
    }

    let error_str = format!("{:?}", error);

    // Check HTTP status code
    if let Some(status_code) = extract_status_code(&error_str) {
        if config.auto_disable_status_codes.contains(&status_code) {
            tracing::info!(
                "[AutoBan] Provider '{}' triggered auto-disable: HTTP {}",
                provider.name, status_code
            );
            return true;
        }
    }

    // Check keywords in error message (case-insensitive)
    let error_lower = error_str.to_lowercase();
    for keyword in &config.auto_disable_keywords {
        if error_lower.contains(&keyword.to_lowercase()) {
            tracing::info!(
                "[AutoBan] Provider '{}' triggered auto-disable: keyword match '{}'",
                provider.name, keyword
            );
            return true;
        }
    }

    false
}

/// 判断错误是否应该跳过重试（直接返回给客户端）
pub fn should_skip_retry(error: &ProxyError, config: &RetryConfig) -> bool {
    let error_str = format!("{:?}", error);

    if let Some(status_code) = extract_status_code(&error_str) {
        if config.skip_retry_status_codes.contains(&status_code) {
            return true;
        }
    }

    false
}

/// 判断是否应该重试
///
/// 逻辑 (对齐 new-api shouldRetry):
/// - 连接错误 → 始终重试
/// - skip_retry 状态码 → 不重试
/// - 2xx → 不重试（不应该走到这里）
/// - 其他 → 重试
pub fn should_retry(error: &ProxyError, config: &RetryConfig) -> bool {
    match error {
        ProxyError::UpstreamError(msg) => {
            // Skip retry codes take precedence
            if let Some(status_code) = extract_status_code(msg) {
                if config.skip_retry_status_codes.contains(&status_code) {
                    return false;
                }
                // 2xx should not retry (shouldn't happen, but guard)
                if (200..300).contains(&(status_code as u32)) {
                    return false;
                }
            }
            true
        }
        ProxyError::RequestError(_) => {
            // Connection/build errors — always retry
            true
        }
        ProxyError::NoProvider | ProxyError::ProviderNotFound(_) => false,
        _ => true,
    }
}

/// 禁用 provider (写入 DB), duration_secs=0 表示永久禁用
pub fn disable_provider(db: &Database, provider_id: i64, reason: &str, duration_secs: u64) {
    use crate::models::ProviderStatus;
    let now = chrono::Utc::now().timestamp_millis();
    let banned_until = if duration_secs > 0 {
        now + (duration_secs as i64 * 1000)
    } else {
        0 // permanent until manual recovery
    };
    tracing::warn!(
        "[AutoBan] Disabling provider #{}: {} (cooldown={}s)",
        provider_id, reason, duration_secs
    );
    #[cfg(feature = "sqlite")]
    {
        if let Err(e) = db.execute(
            "UPDATE providers SET status = ?1, banned_until = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![ProviderStatus::AutoDisabled as i32, banned_until, now, provider_id],
        ) {
            tracing::error!("[AutoBan] Failed to disable provider #{}: {}", provider_id, e);
        }
    }
    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        if let Err(e) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE providers SET status = ?, banned_until = ?, updated_at = ? WHERE id = ?")
                    .bind(ProviderStatus::AutoDisabled as i32).bind(banned_until).bind(now).bind(provider_id)
                    .execute(&db.pool).await?;
                Ok::<_, anyhow::Error>(())
            })
        }) {
            tracing::error!("[AutoBan] Failed to disable provider #{}: {}", provider_id, e);
        }
    }
}

/// 恢复 provider (写入 DB)
pub fn enable_provider(db: &Database, provider_id: i64) {
    use crate::models::ProviderStatus;
    tracing::info!(
        "[AutoBan] Re-enabling auto-disabled provider #{}",
        provider_id
    );
    let now = chrono::Utc::now().timestamp_millis();
    #[cfg(feature = "sqlite")]
    {
        if let Err(e) = db.execute(
            "UPDATE providers SET status = ?1, banned_until = 0, updated_at = ?2 WHERE id = ?3 AND status = ?4",
            rusqlite::params![ProviderStatus::Enabled as i32, now, provider_id, ProviderStatus::AutoDisabled as i32],
        ) {
            tracing::error!("[AutoBan] Failed to enable provider #{}: {}", provider_id, e);
        }
    }
    #[cfg(all(feature = "mysql", not(feature = "sqlite")))]
    {
        if let Err(e) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query("UPDATE providers SET status = ?, banned_until = 0, updated_at = ? WHERE id = ? AND status = ?")
                    .bind(ProviderStatus::Enabled as i32).bind(now).bind(provider_id).bind(ProviderStatus::AutoDisabled as i32)
                    .execute(&db.pool).await?;
                Ok::<_, anyhow::Error>(())
            })
        }) {
            tracing::error!("[AutoBan] Failed to enable provider #{}: {}", provider_id, e);
        }
    }
}

/// 检查 provider 是否已过 cooldown，如果是则自动恢复 (lazy recovery)
/// 返回 true 表示 provider 可用（已恢复或本来就 active）
#[allow(dead_code)]
pub fn check_and_recover(db: &Database, provider: &Provider) -> bool {
    use crate::models::ProviderStatus;
    if provider.status == ProviderStatus::Enabled {
        return true;
    }
    // banned_until == 0 means permanently banned (manual recovery only)
    if provider.status == ProviderStatus::ManuallyDisabled || provider.banned_until == 0 {
        return false;
    }
    let now = chrono::Utc::now().timestamp_millis();
    if now >= provider.banned_until {
        enable_provider(db, provider.id);
        true
    } else {
        false
    }
}

/// 从错误字符串中提取 HTTP 状态码
fn extract_status_code(error_str: &str) -> Option<u16> {
    // Pattern: "HTTP 401" or "HTTP 429"
    if let Some(start) = error_str.find("HTTP ") {
        let after = &error_str[start + 5..];
        let code_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        code_str.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_status_code() {
        assert_eq!(extract_status_code("HTTP 401 from provider 'x': unauthorized"), Some(401));
        assert_eq!(extract_status_code("HTTP 500 internal server error"), Some(500));
        assert_eq!(extract_status_code("connection timeout"), None);
        assert_eq!(extract_status_code("HTTP abc"), None);
    }

    #[test]
    fn test_should_skip_retry() {
        let config = RetryConfig::default();
        let err_400 = ProxyError::UpstreamError("HTTP 400 from provider 'x': bad request".to_string());
        let err_500 = ProxyError::UpstreamError("HTTP 500 from provider 'x': internal".to_string());
        let err_504 = ProxyError::UpstreamError("HTTP 504 from provider 'x': gateway timeout".to_string());

        assert!(should_skip_retry(&err_400, &config));
        assert!(!should_skip_retry(&err_500, &config));
        assert!(should_skip_retry(&err_504, &config));
    }
}
