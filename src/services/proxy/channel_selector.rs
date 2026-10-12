//! 渠道选择器 — 优先级分层 + 加权随机 (对齐 new-api GetRandomSatisfiedChannel)

use std::collections::HashSet;
use rand::Rng;
use crate::models::Provider;

/// 从 provider 列表中选择一个 provider
///
/// 算法:
/// 1. 过滤掉 excluded 中的 provider
/// 2. 按 priority 升序分层（值越小越优先）
/// 3. retry_index 映射到层级
/// 4. 目标层内按 weight 加权随机
pub fn select_provider<'a>(
    providers: &'a [Provider],
    retry_index: u32,
    excluded: &HashSet<i64>,
) -> Option<&'a Provider> {
    // Filter out excluded providers
    let available: Vec<&Provider> = providers
        .iter()
        .filter(|p| !excluded.contains(&p.id))
        .collect();

    if available.is_empty() {
        return None;
    }

    // Extract unique priority levels, sorted ascending (smallest = highest priority)
    let mut priorities: Vec<i32> = available.iter().map(|p| p.priority).collect();
    priorities.sort();
    priorities.dedup();

    if priorities.is_empty() {
        return None;
    }

    // Map retry_index to priority layer (clamped)
    let layer_idx = (retry_index as usize).min(priorities.len() - 1);
    let target_priority = priorities[layer_idx];

    // Collect providers at target priority
    let layer_providers: Vec<&Provider> = available
        .iter()
        .filter(|p| p.priority == target_priority)
        .copied()
        .collect();

    if layer_providers.is_empty() {
        return None;
    }

    if layer_providers.len() == 1 {
        return Some(layer_providers[0]);
    }

    // Weighted random selection within layer
    weighted_random_select(&layer_providers)
}

/// 加权随机选择 (对齐 new-api 的 smoothing 逻辑)
fn weighted_random_select<'a>(providers: &[&'a Provider]) -> Option<&'a Provider> {
    let sum_weight: i32 = providers.iter().map(|p| p.weight).sum();

    // If all weights are 0, treat as equal weight (100 each)
    let (total_weight, smoothing_adjustment) = if sum_weight == 0 {
        (providers.len() as i32 * 100, 100i32)
    } else if (sum_weight / providers.len() as i32) < 10 {
        // When average weight is small, apply smoothing factor
        (sum_weight * 100, 0i32)
    } else {
        (sum_weight, 0i32)
    };

    let smoothing_factor = if sum_weight == 0 { 1 } else if (sum_weight / providers.len() as i32) < 10 { 100 } else { 1 };

    let mut rng = rand::thread_rng();
    let mut random_weight = rng.gen_range(0..total_weight);

    for provider in providers {
        random_weight -= provider.weight * smoothing_factor + smoothing_adjustment;
        if random_weight < 0 {
            return Some(provider);
        }
    }

    // Fallback to last provider
    providers.last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Provider;
    use chrono::Utc;

    fn make_provider(id: i64, priority: i32, weight: i32) -> Provider {
        use crate::models::ProviderStatus;
        Provider {
            id,
            org_id: 0,
            name: format!("provider-{}", id),
            display_name: None,
            preset_key: None,
            base_url: "https://api.example.com".to_string(),
            api_key: "test-key".to_string(),
            priority,
            weight,
            provider_group: "default".to_string(),
            rpm_limit: -1,
            tpm_limit: -1,
            concurrent_limit: -1,
            auth_method: "bearer".to_string(),
            custom_headers: None,
            status: ProviderStatus::Enabled,
            auto_ban: true,
            banned_until: 0,
            remark: None,
            created_at: Utc::now().timestamp_millis(),
            updated_at: Utc::now().timestamp_millis(),
            category: "llm".to_string(),
        }
    }

    #[test]
    fn test_select_highest_priority_on_retry_0() {
        let providers = vec![
            make_provider(1, 0, 100),  // highest priority
            make_provider(2, 5, 100),
            make_provider(3, 10, 100),
        ];
        let excluded = HashSet::new();

        let selected = select_provider(&providers, 0, &excluded).unwrap();
        assert_eq!(selected.id, 1);
    }

    #[test]
    fn test_select_second_priority_on_retry_1() {
        let providers = vec![
            make_provider(1, 0, 100),
            make_provider(2, 5, 100),  // second priority
            make_provider(3, 10, 100),
        ];
        let excluded = HashSet::new();

        let selected = select_provider(&providers, 1, &excluded).unwrap();
        assert_eq!(selected.id, 2);
    }

    #[test]
    fn test_clamp_retry_to_last_layer() {
        let providers = vec![
            make_provider(1, 0, 100),
            make_provider(2, 5, 100),
        ];
        let excluded = HashSet::new();

        // retry_index=10 but only 2 layers, should clamp to layer 1
        let selected = select_provider(&providers, 10, &excluded).unwrap();
        assert_eq!(selected.id, 2);
    }

    #[test]
    fn test_excluded_providers_skipped() {
        let providers = vec![
            make_provider(1, 0, 100),
            make_provider(2, 0, 100),
            make_provider(3, 5, 100),
        ];
        let mut excluded = HashSet::new();
        excluded.insert(1);
        excluded.insert(2);

        // Both priority-0 providers excluded, should fall to priority-5
        let selected = select_provider(&providers, 0, &excluded).unwrap();
        assert_eq!(selected.id, 3);
    }

    #[test]
    fn test_all_excluded_returns_none() {
        let providers = vec![
            make_provider(1, 0, 100),
        ];
        let mut excluded = HashSet::new();
        excluded.insert(1);

        assert!(select_provider(&providers, 0, &excluded).is_none());
    }

    #[test]
    fn test_weighted_distribution() {
        // Provider 1: weight=90, Provider 2: weight=10
        // After many samples, provider 1 should get ~90%
        let providers = vec![
            make_provider(1, 0, 90),
            make_provider(2, 0, 10),
        ];
        let excluded = HashSet::new();

        let mut count_1 = 0;
        let trials = 1000;
        for _ in 0..trials {
            if let Some(p) = select_provider(&providers, 0, &excluded) {
                if p.id == 1 {
                    count_1 += 1;
                }
            }
        }

        // Should be roughly 90% (allow 75-98% range for randomness)
        let ratio = count_1 as f64 / trials as f64;
        assert!(ratio > 0.75 && ratio < 0.98, "Expected ~90%, got {}%", ratio * 100.0);
    }
}
