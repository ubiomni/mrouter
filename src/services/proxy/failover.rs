// 故障转移实现

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::models::Provider;
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

/// 故障转移管理器
pub struct FailoverManager {
    providers: Arc<RwLock<Vec<Provider>>>,
    current_index: Arc<RwLock<usize>>,
    circuit_breakers: Arc<RwLock<Vec<CircuitBreaker>>>,
}

impl FailoverManager {
    pub fn new(providers: Vec<Provider>) -> Self {
        let circuit_breakers = providers
            .iter()
            .map(|_| CircuitBreaker::new(CircuitBreakerConfig::default()))
            .collect();
        
        Self {
            providers: Arc::new(RwLock::new(providers)),
            current_index: Arc::new(RwLock::new(0)),
            circuit_breakers: Arc::new(RwLock::new(circuit_breakers)),
        }
    }
    
    /// 获取当前 Provider
    pub async fn get_current_provider(&self) -> Option<Provider> {
        let providers = self.providers.read().await;
        let index = *self.current_index.read().await;
        
        providers.get(index).cloned()
    }
    
    /// 尝试故障转移到下一个 Provider
    pub async fn failover(&self) -> Result<Option<Provider>> {
        let providers = self.providers.read().await;
        let mut current_index = self.current_index.write().await;
        let circuit_breakers = self.circuit_breakers.read().await;
        
        if providers.is_empty() {
            return Ok(None);
        }
        
        // 尝试找到下一个可用的 Provider
        let start_index = *current_index;
        let mut attempts = 0;
        
        loop {
            *current_index = (*current_index + 1) % providers.len();
            attempts += 1;
            
            // 如果尝试了所有 Provider 都不可用
            if attempts > providers.len() {
                tracing::error!("All providers are unavailable");
                return Ok(None);
            }
            
            // 检查熔断器状态
            if let Some(cb) = circuit_breakers.get(*current_index) {
                if cb.allow_request().await {
                    let provider = providers.get(*current_index).cloned();
                    tracing::info!("Failed over to provider: {:?}", provider.as_ref().map(|p| &p.name));
                    return Ok(provider);
                }
            }
            
            // 如果回到起始位置，说明没有可用的 Provider
            if *current_index == start_index {
                break;
            }
        }
        
        Ok(None)
    }
    
    /// 记录请求成功
    pub async fn record_success(&self) {
        let index = *self.current_index.read().await;
        let circuit_breakers = self.circuit_breakers.read().await;
        
        if let Some(cb) = circuit_breakers.get(index) {
            cb.record_success().await;
        }
    }
    
    /// 记录请求失败
    pub async fn record_failure(&self) {
        let index = *self.current_index.read().await;
        let circuit_breakers = self.circuit_breakers.read().await;
        
        if let Some(cb) = circuit_breakers.get(index) {
            cb.record_failure().await;
        }
    }
    
    /// 更新 Provider 列表
    pub async fn update_providers(&self, new_providers: Vec<Provider>) {
        let mut providers = self.providers.write().await;
        *providers = new_providers.clone();
        
        // 重新创建熔断器
        let new_circuit_breakers = new_providers
            .iter()
            .map(|_| CircuitBreaker::new(CircuitBreakerConfig::default()))
            .collect();
        
        let mut circuit_breakers = self.circuit_breakers.write().await;
        *circuit_breakers = new_circuit_breakers;
        
        // 重置索引
        *self.current_index.write().await = 0;
        
        tracing::info!("Updated provider list: {} providers", new_providers.len());
    }
    
    /// 获取所有 Provider 的状态
    pub async fn get_provider_states(&self) -> Vec<(String, bool)> {
        let providers = self.providers.read().await;
        let circuit_breakers = self.circuit_breakers.read().await;
        
        let mut states = Vec::new();
        
        for (i, provider) in providers.iter().enumerate() {
            if let Some(cb) = circuit_breakers.get(i) {
                let is_available = cb.allow_request().await;
                states.push((provider.name.clone(), is_available));
            }
        }
        
        states
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AppType, ProviderType};

    #[tokio::test]
    async fn test_failover() {
        let providers = vec![
            Provider::new(AppType::ClaudeCode, ProviderType::Anthropic, "Provider1".to_string(), "key1".to_string(), "url1".to_string()),
            Provider::new(AppType::ClaudeCode, ProviderType::OpenAI, "Provider2".to_string(), "key2".to_string(), "url2".to_string()),
        ];
        
        let manager = FailoverManager::new(providers);
        
        // 获取当前 Provider
        let current = manager.get_current_provider().await;
        assert!(current.is_some());
        assert_eq!(current.unwrap().name, "Provider1");
        
        // 故障转移
        let next = manager.failover().await.unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().name, "Provider2");
    }
}
