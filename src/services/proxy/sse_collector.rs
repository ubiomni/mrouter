//! SSE 事件收集器（参考 cc-switch SseUsageCollector）
//!
//! 通过 Arc 内部共享，stream 和 background task 各持一份 clone。
//! stream 内调用 push()，stream 结束后 background task 调用 finish() 取出数据。

use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tokio::sync::Mutex;

/// SSE 使用量收集器
#[derive(Clone)]
pub struct SseUsageCollector {
    inner: Arc<SseUsageCollectorInner>,
}

struct SseUsageCollectorInner {
    events: Mutex<Vec<Value>>,
    first_event_time: Mutex<Option<Instant>>,
    start_time: Instant,
    finished: AtomicBool,
}

impl SseUsageCollector {
    pub fn new(start_time: Instant) -> Self {
        Self {
            inner: Arc::new(SseUsageCollectorInner {
                events: Mutex::new(Vec::new()),
                first_event_time: Mutex::new(None),
                start_time,
                finished: AtomicBool::new(false),
            }),
        }
    }

    /// 推送一个 SSE JSON 事件。首次 push 记录 TTFT 基准时间。
    pub async fn push(&self, event: Value) {
        {
            let mut first_time = self.inner.first_event_time.lock().await;
            if first_time.is_none() {
                *first_time = Some(Instant::now());
            }
        }
        let mut events = self.inner.events.lock().await;
        events.push(event);
    }

    /// 消费所有收集的事件，返回 (events, ttft_ms)。
    /// AtomicBool 保证只执行一次，重复调用返回空。
    pub async fn finish(&self) -> (Vec<Value>, Option<u64>) {
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            return (Vec::new(), None);
        }

        let events = {
            let mut guard = self.inner.events.lock().await;
            std::mem::take(&mut *guard)
        };

        let first_token_ms = {
            let first_time = self.inner.first_event_time.lock().await;
            first_time.map(|t| (t - self.inner.start_time).as_millis() as u64)
        };

        (events, first_token_ms)
    }
}

/// 从 buffer 中提取最早的完整 SSE 块（`\n\n` 或 `\r\n\r\n` 分隔）。
/// 提取后从 buffer 中移除该块及分隔符。
pub fn take_sse_block(buffer: &mut String) -> Option<String> {
    let mut best: Option<(usize, usize)> = None;
    for (delimiter, len) in [("\r\n\r\n", 4usize), ("\n\n", 2usize)] {
        if let Some(pos) = buffer.find(delimiter) {
            if best.is_none() || pos < best.unwrap().0 {
                best = Some((pos, len));
            }
        }
    }
    let (pos, len) = best?;
    let block = buffer[..pos].to_string();
    buffer.drain(..pos + len);
    Some(block)
}

/// 提取 SSE 字段值。支持 `field: value` 和 `field:value` 两种格式。
pub fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("{}: ", field))
        .or_else(|| line.strip_prefix(&format!("{}:", field)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_collector_push_and_finish() {
        let collector = SseUsageCollector::new(Instant::now());

        collector.push(json!({"type": "message_start"})).await;
        collector.push(json!({"type": "content_block_delta"})).await;
        collector.push(json!({"type": "message_stop"})).await;

        let (events, ttft_ms) = collector.finish().await;
        assert_eq!(events.len(), 3);
        assert!(ttft_ms.is_some());
    }

    #[tokio::test]
    async fn test_finish_only_once() {
        let collector = SseUsageCollector::new(Instant::now());
        collector.push(json!({"type": "test"})).await;

        let (events1, _) = collector.finish().await;
        assert_eq!(events1.len(), 1);

        // Second finish returns empty
        let (events2, _) = collector.finish().await;
        assert!(events2.is_empty());
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let collector = SseUsageCollector::new(Instant::now());
        let clone = collector.clone();

        collector.push(json!({"a": 1})).await;
        clone.push(json!({"b": 2})).await;

        let (events, _) = collector.finish().await;
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_take_sse_block_lf() {
        let mut buf = "data: hello\n\ndata: world\n\n".to_string();
        assert_eq!(take_sse_block(&mut buf), Some("data: hello".to_string()));
        assert_eq!(take_sse_block(&mut buf), Some("data: world".to_string()));
        assert_eq!(take_sse_block(&mut buf), None);
    }

    #[test]
    fn test_take_sse_block_crlf() {
        let mut buf = "data: hello\r\n\r\ndata: world\r\n\r\n".to_string();
        assert_eq!(take_sse_block(&mut buf), Some("data: hello".to_string()));
        assert_eq!(take_sse_block(&mut buf), Some("data: world".to_string()));
    }

    #[test]
    fn test_take_sse_block_incomplete() {
        let mut buf = "data: partial".to_string();
        assert_eq!(take_sse_block(&mut buf), None);
        assert_eq!(buf, "data: partial");
    }

    #[test]
    fn test_strip_sse_field() {
        assert_eq!(strip_sse_field("data: hello", "data"), Some("hello"));
        assert_eq!(strip_sse_field("data:hello", "data"), Some("hello"));
        assert_eq!(strip_sse_field("event: message", "data"), None);
        assert_eq!(strip_sse_field("data: ", "data"), Some(""));
    }
}
