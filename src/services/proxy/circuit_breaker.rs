// 熔断器实现

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,      // 正常状态
    Open,        // 熔断状态
    HalfOpen,    // 半开状态（测试恢复）
}

/// 熔断器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,      // 失败阈值
    pub success_threshold: u32,      // 成功阈值（半开状态）
    pub timeout: Duration,           // 熔断超时时间
    pub half_open_timeout: Duration, // 半开状态超时
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            half_open_timeout: Duration::from_secs(30),
        }
    }
}

/// 熔断器
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
    provider_name: String,  // Provider 名称，用于日志
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self::new_with_name(config, "Unknown".to_string())
    }

    pub fn new_with_name(config: CircuitBreakerConfig, provider_name: String) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
            provider_name,
        }
    }
    
    /// 检查是否允许请求通过
    pub async fn allow_request(&self) -> bool {
        let mut state = self.state.write().await;

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 检查是否应该进入半开状态
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.config.timeout {
                        *state = CircuitState::HalfOpen;
                        *self.success_count.write().await = 0;
                        tracing::info!("[CB:{}] Circuit breaker entering half-open state", self.provider_name);
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }
    
    /// 记录成功
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match *state {
            CircuitState::Closed => {
                // 重置失败计数
                let old_count = *self.failure_count.read().await;
                if old_count > 0 {
                    *self.failure_count.write().await = 0;
                    tracing::debug!("[CB:{}] Reset failure count (was: {})", self.provider_name, old_count);
                }
            }
            CircuitState::HalfOpen => {
                let mut success_count = self.success_count.write().await;
                *success_count += 1;

                if *success_count >= self.config.success_threshold {
                    *state = CircuitState::Closed;
                    *self.failure_count.write().await = 0;
                    tracing::info!("[CB:{}] Circuit breaker closed (recovered after {} successes)",
                        self.provider_name, *success_count);
                } else {
                    tracing::debug!("[CB:{}] Success in half-open state ({}/{})",
                        self.provider_name, *success_count, self.config.success_threshold);
                }
            }
            CircuitState::Open => {}
        }
    }
    
    /// 记录失败
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        let mut failure_count = self.failure_count.write().await;

        *failure_count += 1;
        *self.last_failure_time.write().await = Some(Instant::now());

        match *state {
            CircuitState::Closed => {
                if *failure_count >= self.config.failure_threshold {
                    *state = CircuitState::Open;
                    tracing::warn!("[CB:{}] Circuit breaker opened (failures: {}/{})",
                        self.provider_name, *failure_count, self.config.failure_threshold);
                } else {
                    tracing::debug!("[CB:{}] Failure recorded ({}/{})",
                        self.provider_name, *failure_count, self.config.failure_threshold);
                }
            }
            CircuitState::HalfOpen => {
                *state = CircuitState::Open;
                *self.success_count.write().await = 0;
                tracing::warn!("[CB:{}] Circuit breaker re-opened (failure in half-open state)",
                    self.provider_name);
            }
            CircuitState::Open => {
                tracing::debug!("[CB:{}] Failure recorded in open state (count: {})",
                    self.provider_name, *failure_count);
            }
        }
    }
    
    /// 获取当前状态
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }
    
    /// 重置熔断器
    pub async fn reset(&self) {
        let old_state = *self.state.read().await;
        let old_failures = *self.failure_count.read().await;

        *self.state.write().await = CircuitState::Closed;
        *self.failure_count.write().await = 0;
        *self.success_count.write().await = 0;
        *self.last_failure_time.write().await = None;

        tracing::info!("[CB:{}] Circuit breaker reset (was: {:?}, failures: {})",
            self.provider_name, old_state, old_failures);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            half_open_timeout: Duration::from_millis(50),
        };
        
        let cb = CircuitBreaker::new(config);
        
        // 初始状态应该是 Closed
        assert_eq!(cb.get_state().await, CircuitState::Closed);
        assert!(cb.allow_request().await);
        
        // 记录失败
        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_failure().await;
        
        // 应该进入 Open 状态
        assert_eq!(cb.get_state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);
        
        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // 应该进入 HalfOpen 状态
        assert!(cb.allow_request().await);
        assert_eq!(cb.get_state().await, CircuitState::HalfOpen);
        
        // 记录成功
        cb.record_success().await;
        cb.record_success().await;
        
        // 应该恢复到 Closed 状态
        assert_eq!(cb.get_state().await, CircuitState::Closed);
    }
}
