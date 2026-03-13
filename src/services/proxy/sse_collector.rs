//! SSE 事件收集器
//!
//! 基于回调的设计，内存高效地收集流式响应事件

use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Mutex;
use std::time::Instant;

type UsageCallback = Arc<dyn Fn(Vec<Value>, Option<u64>) + Send + Sync + 'static>;

/// SSE 使用量收集器
#[derive(Clone)]
pub struct SseCollector {
    inner: Arc<SseCollectorInner>,
}

struct SseCollectorInner {
    /// 收集的事件列表
    events: Mutex<Vec<Value>>,
    /// 首个事件时间（用于计算 TTFT）
    first_event_time: Mutex<Option<Instant>>,
    /// 请求开始时间
    start_time: Instant,
    /// 完成时的回调函数
    on_complete: UsageCallback,
    /// 是否已完成
    finished: AtomicBool,
}

impl SseCollector {
    /// 创建新的收集器
    ///
    /// # Arguments
    /// * `start_time` - 请求开始时间
    /// * `callback` - 完成时的回调函数，接收事件列表和首 token 时间（毫秒）
    pub fn new(
        start_time: Instant,
        callback: impl Fn(Vec<Value>, Option<u64>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(SseCollectorInner {
                events: Mutex::new(Vec::new()),
                first_event_time: Mutex::new(None),
                start_time,
                on_complete: Arc::new(callback),
                finished: AtomicBool::new(false),
            }),
        }
    }

    /// 推送一个 SSE 事件
    pub async fn push(&self, event: Value) {
        // 记录首个事件时间
        {
            let mut first_time = self.inner.first_event_time.lock().await;
            if first_time.is_none() {
                *first_time = Some(Instant::now());
            }
        }

        // 添加事件到列表
        let mut events = self.inner.events.lock().await;
        events.push(event);
    }

    /// 完成收集并触发回调
    pub async fn finish(&self) {
        // 确保只执行一次
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            return;
        }

        // 取出所有事件
        let events = {
            let mut guard = self.inner.events.lock().await;
            std::mem::take(&mut *guard)
        };

        // 计算首 token 时间（毫秒）
        let first_token_ms = {
            let first_time = self.inner.first_event_time.lock().await;
            first_time.map(|t| (t - self.inner.start_time).as_millis() as u64)
        };

        // 触发回调
        (self.inner.on_complete)(events, first_token_ms);
    }
}

/// 从 SSE 数据流中解析事件
///
/// 处理 SSE 格式的数据，提取 `data:` 行并解析为 JSON
pub fn parse_sse_chunk(buffer: &mut String, collector: &Option<SseCollector>) -> tokio::task::JoinHandle<()> {
    let mut parsed_events = Vec::new();

    // 查找完整的事件（以 \n\n 分隔）
    while let Some(pos) = buffer.find("\n\n") {
        let event_text = buffer[..pos].to_string();
        *buffer = buffer[pos + 2..].to_string();

        if !event_text.trim().is_empty() {
            // 提取 data 部分并尝试解析为 JSON
            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() != "[DONE]" && !data.trim().is_empty() {
                        if let Ok(json_value) = serde_json::from_str::<Value>(data) {
                            parsed_events.push(json_value);
                        }
                    }
                }
            }
        }
    }

    // 异步推送事件到收集器
    let collector_clone = collector.clone();
    tokio::spawn(async move {
        if let Some(c) = collector_clone {
            for event in parsed_events {
                c.push(event).await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    #[tokio::test]
    async fn test_sse_collector() {
        let collected = Arc::new(TokioMutex::new(Vec::new()));
        let collected_clone = collected.clone();

        let collector = SseCollector::new(Instant::now(), move |events, _ttft| {
            let collected = collected_clone.clone();
            tokio::spawn(async move {
                let mut guard = collected.lock().await;
                *guard = events;
            });
        });

        // 推送一些事件
        collector.push(json!({"type": "message_start"})).await;
        collector.push(json!({"type": "content_block_delta"})).await;
        collector.push(json!({"type": "message_stop"})).await;

        // 完成收集
        collector.finish().await;

        // 等待回调执行
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let guard = collected.lock().await;
        assert_eq!(guard.len(), 3);
    }

    #[test]
    fn test_parse_sse_chunk() {
        let mut buffer = String::from("data: {\"type\":\"message_start\"}\n\ndata: {\"type\":\"message_stop\"}\n\n");
        let collector = None;

        let handle = parse_sse_chunk(&mut buffer, &collector);
        drop(handle);

        assert!(buffer.is_empty());
    }
}
