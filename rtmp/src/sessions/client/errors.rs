use std::fmt;
use failure::{Backtrace, Fail};
use ::chunk_io::{ChunkSerializationError, ChunkDeserializationError};
use ::messages::{MessageSerializationError, MessageDeserializationError};
use ::sessions::ClientState;

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

    /// Encountered if a request is made, or a response is received for a request while the
    /// client session is not in a valid state for that purpose.
    #[fail(display = "The request could not be performed while the session is in the {:?} state", current_state)]
    SessionInInvalidState {
        current_state: ClientState
    },

    /// Encountered when the client requests a stream be created and the server rejects the command
    #[fail(display = "An attempt to create a stream on the server failed")]
    CreateStreamFailed,

    /// A response to a `createStream` request should have a numeric as the first parameter
    /// in the additional values property of the amf0 command.  This error is thrown if this is
    /// not present.  Without a stream ID we have no way to know what stream to communicate with
    /// for playback/publishing messages.
    #[fail(display = "The server sent a create stream success result without a stream id")]
    CreateStreamResponseHadNoStreamNumber,

    /// When the server sends and `onStatus` message, it is expected that the additional arguments
    /// contains a single value representing an amf0 object.  This is required because the object
    /// should have a `code` property that says the type of operation the status is for.
    #[fail(display = "The server sent an onStatus message with invalid arguments")]
    InvalidOnStatusArguments,
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