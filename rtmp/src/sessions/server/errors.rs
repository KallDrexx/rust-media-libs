use std::fmt;
use failure::{Backtrace, Fail};
use ::chunk_io::{ChunkSerializationError, ChunkDeserializationError};
use ::messages::{MessageSerializationError, MessageDeserializationError};

/// Error state when a server session encounters and error
#[derive(Debug)]
pub struct ServerSessionError {
    pub kind: ServerSessionErrorKind,
}

/// Represents the type of error that occurred
#[derive(Debug, Fail)]
pub enum ServerSessionErrorKind {
    /// Encountered when an error occurs while deserializing the incoming byte data
    #[fail(display = "An error occurred deserializing incoming data: {}", _0)]
    ChunkDeserializationError(#[cause] ChunkDeserializationError),

    /// Encountered when an error occurs while serializing outbound messages
    #[fail(display = "An error occurred serializing outbound messages: {}", _0)]
    ChunkSerializationError(#[cause] ChunkSerializationError),

    /// Encountered when an error occurs while turning an RTMP message into an message payload
    #[fail(display = "An error occurred while attempting to turn an RTMP message into a message payload: {}", _0)]
    MessageSerializationError(#[cause] MessageSerializationError),

    /// Encountered when an error occurs while turning a message payload into an RTMP message
    #[fail(display = "An error occurred while attempting to turn a message payload into an RTMP message: {}", _0)]
    MessageDeserializationError(#[cause] MessageDeserializationError),

    /// A request id that was attempting to be accepted or rejected was not a known
    /// outstanding request.
    #[fail(display = "Attempted to accept or reject request id {} but no outstanding requests have that id", _0)]
    InvalidOutstandingRequest(u32),
}

impl fmt::Display for ServerSessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for ServerSessionError {
    fn cause(&self) -> Option<&Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<ChunkSerializationError> for ServerSessionError {
    fn from(kind: ChunkSerializationError) -> Self {
        ServerSessionError { kind: ServerSessionErrorKind::ChunkSerializationError(kind) }
    }
}

impl From<ChunkDeserializationError> for ServerSessionError {
    fn from(kind: ChunkDeserializationError) -> Self {
        ServerSessionError { kind: ServerSessionErrorKind::ChunkDeserializationError(kind) }
    }
}

impl From<MessageSerializationError> for ServerSessionError {
    fn from(kind: MessageSerializationError) -> Self {
        ServerSessionError { kind: ServerSessionErrorKind::MessageSerializationError(kind) }
    }
}

impl From<MessageDeserializationError> for ServerSessionError {
    fn from(kind: MessageDeserializationError) -> Self {
        ServerSessionError { kind: ServerSessionErrorKind::MessageDeserializationError(kind) }
    }
}