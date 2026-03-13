//! 成本计算模块
//!
//! 使用高精度 Decimal 类型避免浮点数精度问题

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::models::{TokenUsage, PricingConfig};

/// 成本明细
#[derive(Debug, Clone)]
pub struct CostBreakdown {
    pub input_cost: Decimal,
    pub output_cost: Decimal,
    pub cache_read_cost: Decimal,
    pub cache_creation_cost: Decimal,
    pub total_cost: Decimal,
}

impl CostBreakdown {
    /// 转换为 f64（用于数据库存储，确保非负）
    pub fn total_as_f64(&self) -> f64 {
        self.total_cost.to_string().parse::<f64>().unwrap_or(0.0).max(0.0)
    }
}

/// 成本计算器
pub struct CostCalculator;

impl CostCalculator {
    /// 计算请求成本（带详细明细）
    ///
    /// # 参数
    /// - `usage`: Token 使用量
    /// - `pricing`: 模型定价
    /// - `cost_multiplier`: 成本倍数（默认 1.0）
    ///
    /// # 计算逻辑
    /// - 避免缓存 token 重复计费
    /// - input_cost: (input_tokens - cache_read_tokens) × 输入价格
    /// - cache_read_cost: cache_read_tokens × 缓存读取价格
    /// - total_cost: 各项成本之和 × 倍率
    pub fn calculate(
        usage: &TokenUsage,
        pricing: &PricingConfig,
        cost_multiplier: Decimal,
    ) -> CostBreakdown {
        let million = dec!(1_000_000);

        // 计算实际需要按输入价格计费的 token 数（减去缓存命中部分）
        // 避免缓存 token 重复计费
        let billable_input_tokens = usage.input_tokens.saturating_sub(usage.cache_read_tokens);

        // 各项基础成本（不含倍率）
        let input_cost = Decimal::from(billable_input_tokens)
            * Decimal::from_f64_retain(pricing.input_price_per_million).unwrap_or(dec!(0))
            / million;

        let output_cost = Decimal::from(usage.output_tokens)
            * Decimal::from_f64_retain(pricing.output_price_per_million).unwrap_or(dec!(0))
            / million;

        let cache_read_cost = Decimal::from(usage.cache_read_tokens)
            * Decimal::from_f64_retain(pricing.cache_read_price_per_million).unwrap_or(dec!(0))
            / million;

        let cache_creation_cost = Decimal::from(usage.cache_creation_tokens)
            * Decimal::from_f64_retain(pricing.cache_write_price_per_million).unwrap_or(dec!(0))
            / million;

        // 总成本 = 各项基础成本之和 × 倍率（确保不为负数）
        let base_total = input_cost + output_cost + cache_read_cost + cache_creation_cost;
        let total_cost = (base_total * cost_multiplier).max(dec!(0));

        CostBreakdown {
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
        }
    }

    /// 计算成本（简化版，返回 f64）
    pub fn calculate_simple(usage: &TokenUsage, pricing: &PricingConfig) -> f64 {
        Self::calculate(usage, pricing, dec!(1.0)).total_as_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_calculation() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 100,
            cache_read_tokens: 200,
        };

        let pricing = PricingConfig {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
            cache_write_price_per_million: 3.75,
            cache_read_price_per_million: 0.30,
        };

        let breakdown = CostCalculator::calculate(&usage, &pricing, dec!(1.0));

        // input_cost: (1000 - 200) * 3.0 / 1_000_000 = 0.0024
        // output_cost: 500 * 15.0 / 1_000_000 = 0.0075
        // cache_read_cost: 200 * 0.30 / 1_000_000 = 0.00006
        // cache_creation_cost: 100 * 3.75 / 1_000_000 = 0.000375

        assert_eq!(breakdown.input_cost, dec!(0.0024));
        assert_eq!(breakdown.output_cost, dec!(0.0075));
        assert_eq!(breakdown.cache_read_cost, dec!(0.00006));
        assert_eq!(breakdown.cache_creation_cost, dec!(0.000375));
    }

    #[test]
    fn test_cost_multiplier() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        };

        let pricing = PricingConfig {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
            cache_write_price_per_million: 0.0,
            cache_read_price_per_million: 0.0,
        };

        let breakdown = CostCalculator::calculate(&usage, &pricing, dec!(1.5));

        // base_total: 0.003 + 0.0075 = 0.0105
        // total_cost: 0.0105 * 1.5 = 0.01575
        assert_eq!(breakdown.total_cost, dec!(0.01575));
    }
}
