// Per-provider response adaptors — parse upstream SSE into normalized StreamEvents

pub mod openai;
pub mod anthropic;
pub mod google;
pub mod openai_responses;

use serde_json::Value;
use crate::models::Provider;
use super::stream_event::StreamEvent;

/// Trait for parsing provider-specific SSE/response formats into normalized StreamEvents.
pub trait ResponseAdaptor: Send + Sync {
    /// Parse a single SSE data payload (the string after "data: ") into StreamEvents.
    /// May return zero events (for pings/comments) or multiple (e.g. message_start + content_block_start).
    fn parse_sse_data(&self, data: &str) -> Vec<StreamEvent>;

    /// Parse SSE lines that include "event:" type prefix (Anthropic-style).
    /// Default impl strips event prefix and delegates to parse_sse_data.
    fn parse_sse_line(&self, event_type: Option<&str>, data: &str) -> Vec<StreamEvent> {
        let _ = event_type;
        self.parse_sse_data(data)
    }

    /// Parse a non-streaming response body into StreamEvents.
    fn parse_response(&self, body: &Value) -> Vec<StreamEvent>;
}

/// Get the appropriate response adaptor based on provider's API format.
pub fn get_response_adaptor_for_format(format: crate::models::ApiFormat) -> Box<dyn ResponseAdaptor> {
    match format {
        crate::models::ApiFormat::Anthropic => Box::new(anthropic::AnthropicResponseAdaptor),
        crate::models::ApiFormat::OpenAIResponses => Box::new(openai_responses::ResponsesApiAdaptor),
        crate::models::ApiFormat::Google => Box::new(google::GoogleResponseAdaptor),
        _ => Box::new(openai::OpenAIResponseAdaptor),
    }
}

/// Get the appropriate response adaptor for a provider (legacy, defaults to OpenAI).
pub fn get_response_adaptor(_provider: &Provider) -> Box<dyn ResponseAdaptor> {
    Box::new(openai::OpenAIResponseAdaptor)
}
