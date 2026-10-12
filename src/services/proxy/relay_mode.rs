/// RelayMode determines what operation is being requested, independent of the client path.
/// Each provider adapter uses this to construct the correct upstream URL.
#[derive(Debug, Clone, PartialEq)]
pub enum RelayMode {
    ChatCompletions,
    Completions,
    Messages,
    Embeddings,
    ImagesGenerations,
    ImagesEdits,
    AudioSpeech,
    AudioTranscriptions,
    Moderations,
    Rerank,
    Responses,
    /// Unknown path — carried as-is for passthrough
    Passthrough(String),
}

impl RelayMode {
    /// Parse relay mode from the request path.
    /// Order matters: more specific patterns first.
    pub fn from_path(path: &str) -> Self {
        if path.contains("/chat/completions") {
            Self::ChatCompletions
        } else if path.contains("/responses") {
            Self::Responses
        } else if path.contains("/messages") {
            Self::Messages
        } else if path.contains("/embeddings") {
            Self::Embeddings
        } else if path.contains("/images/edits") {
            Self::ImagesEdits
        } else if path.contains("/images/generations") {
            Self::ImagesGenerations
        } else if path.contains("/audio/speech") {
            Self::AudioSpeech
        } else if path.contains("/audio/transcriptions") {
            Self::AudioTranscriptions
        } else if path.contains("/moderations") {
            Self::Moderations
        } else if path.contains("/rerank") {
            Self::Rerank
        } else if path.contains("/completions") {
            Self::Completions
        } else {
            Self::Passthrough(path.to_string())
        }
    }
}

impl std::fmt::Display for RelayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChatCompletions => write!(f, "ChatCompletions"),
            Self::Completions => write!(f, "Completions"),
            Self::Messages => write!(f, "Messages"),
            Self::Embeddings => write!(f, "Embeddings"),
            Self::ImagesGenerations => write!(f, "ImagesGenerations"),
            Self::ImagesEdits => write!(f, "ImagesEdits"),
            Self::AudioSpeech => write!(f, "AudioSpeech"),
            Self::AudioTranscriptions => write!(f, "AudioTranscriptions"),
            Self::Moderations => write!(f, "Moderations"),
            Self::Rerank => write!(f, "Rerank"),
            Self::Responses => write!(f, "Responses"),
            Self::Passthrough(p) => write!(f, "Passthrough({})", p),
        }
    }
}
