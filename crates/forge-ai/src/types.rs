//! Provider-neutral request/response types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// One piece of message content. Only text is sent today; the enum leaves room for images
/// and tool calls without changing the request shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentPart {
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

impl Message {
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentPart::Text { text: text.into() }],
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::text(Role::User, text)
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::text(Role::Assistant, text)
    }

    /// All text parts joined (what text-only APIs receive).
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .map(|p| match p {
                ContentPart::Text { text } => text.as_str(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model id; `None` uses the provider's default model.
    pub model: Option<String>,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stop: Vec<String>,
}

impl CompletionRequest {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            ..Self::default()
        }
    }
}

/// A fill-in-the-middle request: the model writes what goes between `prompt` and `suffix`
/// (inline code completion with local code models).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FimRequest {
    /// Model id; `None` uses the provider's default model.
    pub model: Option<String>,
    /// Text before the caret (with any context header).
    pub prompt: String,
    /// Text after the caret.
    pub suffix: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stop: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub text: String,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

/// One streamed event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    /// A chunk of generated text.
    Text { text: String },
    /// Generation finished; always the last item of a successful stream.
    Finish {
        stop_reason: Option<String>,
        usage: Option<Usage>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
}
