use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_AI_STREAM_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTaskKind {
    InlineCompletion,
    RewriteSelection,
    RewriteBlocks,
}

/// Host-provided model metadata rendered by Cditor's model selector.
///
/// `id` is the stable value returned to the provider in every request.
/// `display_name` is the primary row label, while `provider_name` and
/// `description` form the secondary line shown in the dropdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModelDescriptor {
    pub id: String,
    pub display_name: String,
    pub provider_name: String,
    pub description: Option<String>,
}

impl AiModelDescriptor {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        provider_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            provider_name: provider_name.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn subtitle(&self) -> String {
        match self
            .description
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            Some(description) if !self.provider_name.is_empty() => {
                format!("{} · {description}", self.provider_name)
            }
            Some(description) => description.to_owned(),
            None => self.provider_name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderRequest {
    pub request_id: u64,
    pub task: AiTaskKind,
    /// The model selected in Cditor's UI. Providers that expose no model
    /// catalog receive `None` and may keep their own internal default.
    pub model_id: Option<String>,
    pub instruction: String,
    pub selected_text: String,
    pub prefix: String,
    pub suffix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiStreamEvent {
    Delta { request_id: u64, text: String },
    Done { request_id: u64 },
    Error { request_id: u64, message: String },
}

pub type AiStreamSender = async_channel::Sender<AiStreamEvent>;

impl AiStreamEvent {
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Delta { request_id, .. }
            | Self::Done { request_id }
            | Self::Error { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Default)]
pub struct AiCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl AiCancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl fmt::Debug for AiCancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiCancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProviderError {
    Cancelled,
    ChannelClosed,
    InvalidConfiguration(String),
    Request(String),
    Protocol(String),
}

impl fmt::Display for AiProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("AI request was cancelled"),
            Self::ChannelClosed => formatter.write_str("AI stream receiver was closed"),
            Self::InvalidConfiguration(message)
            | Self::Request(message)
            | Self::Protocol(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AiProviderError {}

pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;

    /// Models offered to the editor UI. A host can aggregate models from
    /// multiple backends behind one provider and route by `model_id`.
    fn models(&self) -> Vec<AiModelDescriptor> {
        Vec::new()
    }

    /// Initial selection. Invalid or missing ids fall back to the first model.
    fn default_model_id(&self) -> Option<String> {
        None
    }

    /// This method is blocking and must be invoked on a background executor.
    fn stream(
        &self,
        request: AiProviderRequest,
        sender: AiStreamSender,
        cancellation: AiCancellationToken,
    ) -> Result<(), AiProviderError>;
}

pub fn bounded_ai_stream(
    capacity: usize,
) -> (
    async_channel::Sender<AiStreamEvent>,
    async_channel::Receiver<AiStreamEvent>,
) {
    async_channel::bounded(capacity.max(1))
}

pub(crate) fn send_stream_event(
    sender: &async_channel::Sender<AiStreamEvent>,
    event: AiStreamEvent,
    cancellation: &AiCancellationToken,
) -> Result<(), AiProviderError> {
    if cancellation.is_cancelled() {
        return Err(AiProviderError::Cancelled);
    }
    sender
        .send_blocking(event)
        .map_err(|_| AiProviderError::ChannelClosed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_token_is_shared_between_clones() {
        let token = AiCancellationToken::default();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn bounded_stream_never_accepts_zero_capacity() {
        let (sender, receiver) = bounded_ai_stream(0);
        sender
            .send_blocking(AiStreamEvent::Done { request_id: 7 })
            .unwrap();
        assert_eq!(
            receiver.recv_blocking().unwrap(),
            AiStreamEvent::Done { request_id: 7 }
        );
    }
}
