// Normalized stream events — the intermediate representation between
// provider-specific SSE and client-facing output format.

use serde::Serialize;

/// Token usage counters
#[derive(Debug, Clone, Default, Serialize)]
pub struct StreamUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Cache read tokens (Anthropic prompt caching)
    pub cache_read_tokens: u64,
    /// Cache creation tokens
    pub cache_creation_tokens: u64,
}

/// Why the stream ended
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum StopReason {
    /// Natural end of generation
    EndTurn,
    /// Hit max_tokens limit
    MaxTokens,
    /// Hit a stop sequence
    StopSequence,
    /// Content was filtered
    ContentFilter,
    /// Tool use requested
    ToolUse,
}

impl StopReason {
    /// From OpenAI finish_reason string
    pub fn from_openai(s: &str) -> Self {
        match s {
            "stop" => Self::EndTurn,
            "length" => Self::MaxTokens,
            "content_filter" => Self::ContentFilter,
            "tool_calls" | "function_call" => Self::ToolUse,
            _ => Self::EndTurn,
        }
    }

    /// From Anthropic stop_reason string
    pub fn from_anthropic(s: &str) -> Self {
        match s {
            "end_turn" => Self::EndTurn,
            "max_tokens" => Self::MaxTokens,
            "stop_sequence" => Self::StopSequence,
            "tool_use" => Self::ToolUse,
            _ => Self::EndTurn,
        }
    }

    /// To OpenAI finish_reason string
    pub fn to_openai(&self) -> &'static str {
        match self {
            Self::EndTurn => "stop",
            Self::MaxTokens => "length",
            Self::StopSequence => "stop",
            Self::ContentFilter => "content_filter",
            Self::ToolUse => "tool_calls",
        }
    }

    /// To Anthropic stop_reason string
    pub fn to_anthropic(&self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::MaxTokens => "max_tokens",
            Self::StopSequence => "stop_sequence",
            Self::ContentFilter => "end_turn",
            Self::ToolUse => "tool_use",
        }
    }
}

/// A normalized stream event produced by a ResponseAdaptor.
/// These are format-agnostic and can be rendered into any client format by an OutputFormatter.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Stream started — role announcement + metadata
    MessageStart {
        id: String,
        model: String,
    },

    /// Text content delta
    ContentDelta {
        index: u32,
        text: String,
    },

    /// Thinking/reasoning delta (extended thinking / chain-of-thought)
    ThinkingDelta {
        index: u32,
        text: String,
    },

    /// Tool call delta
    ToolCallDelta {
        index: u32,
        tool_call_id: String,
        name: Option<String>,
        arguments: String,
    },

    /// A content block finished
    ContentStop {
        index: u32,
    },

    /// Stream ending — stop reason and optional final usage
    MessageDelta {
        stop_reason: StopReason,
        usage: Option<StreamUsage>,
    },

    /// Final signal — stream is done
    MessageStop,

    /// Usage-only update (internal accounting, typically not forwarded to client)
    UsageUpdate(StreamUsage),

    /// Keep-alive / heartbeat
    Ping,
}

impl StreamEvent {
    /// Returns true if this event carries content the client should see
    pub fn is_client_visible(&self) -> bool {
        !matches!(self, StreamEvent::UsageUpdate(_) | StreamEvent::Ping)
    }

    /// Returns true if this event has output text (for incomplete generation detection)
    pub fn has_output_content(&self) -> bool {
        matches!(self, StreamEvent::ContentDelta { .. })
    }

    /// Returns true if this event is thinking content
    pub fn is_thinking(&self) -> bool {
        matches!(self, StreamEvent::ThinkingDelta { .. })
    }

    /// Extract usage if present
    pub fn usage(&self) -> Option<&StreamUsage> {
        match self {
            StreamEvent::MessageDelta { usage, .. } => usage.as_ref(),
            StreamEvent::UsageUpdate(u) => Some(u),
            _ => None,
        }
    }
}
