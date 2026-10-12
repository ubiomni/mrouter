//! 请求日志记录器
//!
//! 负责记录每个代理请求的详细信息

use crate::database::Database;
use crate::database::dao::StatsDao;
use crate::models::{RequestLog, TokenUsage, BillingType};
use chrono::Utc;
use anyhow::Result;

/// 请求日志记录器
pub struct RequestLogger<'a> {
    db: &'a Database,
}

impl<'a> RequestLogger<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 记录成功的请求
    pub fn log_request(&self, log: &RequestLog) -> Result<i64> {
        StatsDao::insert_request_log(self.db, log)
    }

    /// 记录失败的请求
    pub fn log_error(
        &self,
        provider_id: i64,
        request_time: i64,
        duration_ms: i64,
        status_code: i32,
        error_message: String,
        request_path: Option<String>,
        request_method: Option<String>,
        session_id: Option<String>,
    ) -> Result<i64> {
        let log = RequestLog {
            id: 0,
            provider_id,
            request_time,
            response_time: Some(Utc::now().timestamp_millis()),
            duration_ms: Some(duration_ms),
            first_token_ms: None,
            status_code: Some(status_code),
            model: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            total_tokens: 0,
            estimated_cost: 0.0,
            error_message: Some(error_message),
            request_path,
            request_method,
            session_id,
            token_id: None,
            token_name: None,
            user_id: None,
            api_key_id: None,
            api_key_name: None,
            provider_name: None,
            client_ip: None,
            request_id: None,
            retry_count: 0,
            billing_type: BillingType::Token,
            task_type: None,
            task_count: 0,
        };

        self.log_request(&log)
    }
}

/// Builder pattern for constructing RequestLog
pub struct RequestLogBuilder {
    log: RequestLog,
}

impl RequestLogBuilder {
    pub fn new(provider_id: i64, request_time: i64) -> Self {
        Self {
            log: RequestLog {
                id: 0,
                provider_id,
                request_time,
                response_time: None,
                duration_ms: None,
                first_token_ms: None,
                status_code: None,
                model: None,
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
                estimated_cost: 0.0,
                error_message: None,
                request_path: None,
                request_method: None,
                session_id: None,
                token_id: None,
                token_name: None,
                user_id: None,
                api_key_id: None,
                api_key_name: None,
                provider_name: None,
                client_ip: None,
                request_id: None,
                retry_count: 0,
                billing_type: BillingType::Token,
                task_type: None,
                task_count: 0,
            },
        }
    }

    pub fn response_time(mut self, time: i64) -> Self {
        self.log.response_time = Some(time);
        self
    }

    pub fn duration_ms(mut self, ms: i64) -> Self {
        self.log.duration_ms = Some(ms);
        self
    }

    pub fn first_token_ms(mut self, ms: i64) -> Self {
        self.log.first_token_ms = Some(ms);
        self
    }

    pub fn status_code(mut self, code: i32) -> Self {
        self.log.status_code = Some(code);
        self
    }

    pub fn model(mut self, model: String) -> Self {
        self.log.model = Some(model);
        self
    }

    pub fn usage(mut self, usage: &TokenUsage) -> Self {
        self.log.input_tokens = usage.input_tokens;
        self.log.output_tokens = usage.output_tokens;
        self.log.cache_creation_tokens = usage.cache_creation_tokens;
        self.log.cache_read_tokens = usage.cache_read_tokens;
        self.log.total_tokens = usage.total_tokens();
        self
    }

    pub fn cost(mut self, cost: f64) -> Self {
        self.log.estimated_cost = cost;
        self
    }

    pub fn error_message(mut self, msg: String) -> Self {
        self.log.error_message = Some(msg);
        self
    }

    pub fn request_path(mut self, path: String) -> Self {
        self.log.request_path = Some(path);
        self
    }

    pub fn request_method(mut self, method: String) -> Self {
        self.log.request_method = Some(method);
        self
    }

    pub fn session_id(mut self, id: String) -> Self {
        self.log.session_id = Some(id);
        self
    }

    pub fn token_info(mut self, token_id: i64, token_name: String) -> Self {
        self.log.token_id = Some(token_id);
        self.log.token_name = Some(token_name);
        self
    }

    pub fn user_id(mut self, id: i64) -> Self {
        self.log.user_id = Some(id);
        self
    }

    pub fn api_key_info(mut self, id: i64, name: String) -> Self {
        self.log.api_key_id = Some(id);
        self.log.api_key_name = Some(name);
        self
    }

    pub fn provider_name(mut self, name: String) -> Self {
        self.log.provider_name = Some(name);
        self
    }

    pub fn client_ip(mut self, ip: String) -> Self {
        self.log.client_ip = Some(ip);
        self
    }

    pub fn request_id(mut self, id: String) -> Self {
        self.log.request_id = Some(id);
        self
    }

    pub fn retry_count(mut self, count: i32) -> Self {
        self.log.retry_count = count;
        self
    }

    pub fn billing_type(mut self, billing_type: BillingType) -> Self {
        self.log.billing_type = billing_type;
        self
    }

    pub fn task_info(mut self, task_type: String, task_count: i64) -> Self {
        self.log.task_type = Some(task_type);
        self.log.task_count = task_count;
        self
    }

    pub fn build(self) -> RequestLog {
        self.log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_log_builder() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_tokens: 10,
            cache_read_tokens: 20,
        };

        let log = RequestLogBuilder::new(1, Utc::now().timestamp_millis())
            .duration_ms(1000)
            .status_code(200)
            .model("claude-sonnet-4".to_string())
            .usage(&usage)
            .cost(0.005)
            .request_path("/v1/messages".to_string())
            .request_method("POST".to_string())
            .build();

        assert_eq!(log.provider_id, 1);
        assert_eq!(log.duration_ms, Some(1000));
        assert_eq!(log.status_code, Some(200));
        assert_eq!(log.input_tokens, 100);
        assert_eq!(log.output_tokens, 50);
        assert_eq!(log.total_tokens, 180);
        assert_eq!(log.estimated_cost, 0.005);
    }
}
