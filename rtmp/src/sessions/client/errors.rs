use std::fmt;
use failure::{Backtrace, Fail};
use ::chunk_io::{ChunkSerializationError, ChunkDeserializationError};
use ::messages::{MessageSerializationError, MessageDeserializationError};

/// Error state when a client session encounters an error
#[derive(Debug)]
pub struct ClientSessionError {
    pub kind: ClientSessionErrorKind,
}

/// Represents the type of error that occurred
#[derive(Debug, Fail)]
pub enum ClientSessionErrorKind {
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

    /// Encountered if a connection request is made while we are already connected
    #[fail(display = "A connection request was attempted while this session is already in a connected state")]
    CantConnectWhileAlreadyConnected,
}

impl fmt::Display for ClientSessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for ClientSessionError {
    fn cause(&self) -> Option<&Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<ChunkSerializationError> for ClientSessionError {
    fn from(kind: ChunkSerializationError) -> Self {
        ClientSessionError { kind: ClientSessionErrorKind::ChunkSerializationError(kind) }
    }
}

impl From<ChunkDeserializationError> for ClientSessionError {
    fn from(kind: ChunkDeserializationError) -> Self {
        ClientSessionError { kind: ClientSessionErrorKind::ChunkDeserializationError(kind) }
    }
}

impl From<MessageSerializationError> for ClientSessionError {
    fn from(kind: MessageSerializationError) -> Self {
        ClientSessionError { kind: ClientSessionErrorKind::MessageSerializationError(kind) }
    }
}

impl From<MessageDeserializationError> for ClientSessionError {
    fn from(kind: MessageDeserializationError) -> Self {
        ClientSessionError { kind: ClientSessionErrorKind::MessageDeserializationError(kind) }
    }
}