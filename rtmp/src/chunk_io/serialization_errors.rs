use std::io;
use std::fmt;
use failure::{Backtrace, Fail};
use ::messages::MessageSerializationError;

/// Error state when an error occurs while attempting to serialize an RTMP message into
/// RTMP chunks.
#[derive(Debug)]
pub struct ChunkSerializationError {
    /// The kind of error that occurred
    pub kind: ChunkSerializationErrorKind
}

/// An enumeration defining all the possible errors that could occur while serializing
/// RTMP messages into RTMP chunks.
#[derive(Debug, Fail)]
pub enum ChunkSerializationErrorKind {
    /// RTMP specification states that a message cannot be more than 16777215, even when split
    /// across multiple RTMP chunks
    #[fail(display = "The current message has a length of {} bytes, which is over the allowed size of 16777215 bytes", size)]
    MessageTooLong {
        size: u32
    },

    /// Encountered when the chunk size is set to an invalid value
    #[fail(display = "An invalid chunk size was specified.  Chunk size must be greater than 0 and less than 2147483647")]
    InvalidMaxChunkSize {
        attempted_chunk_size: u32
    },

    /// An I/O error occurred while writing the output buffer
    #[fail(display = "_0")]
    Io(#[cause] io::Error),

    /// Occurs when an error is returned when trying to create a set chunk size message
    #[fail(display = "Failed to create SetChunkSize message: _0")]
    SetChunkSizeMessageCreationFailure(#[cause] MessageSerializationError),
}

impl fmt::Display for ChunkSerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for ChunkSerializationError {
    fn cause(&self) -> Option<&Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<ChunkSerializationErrorKind> for ChunkSerializationError {
    fn from(kind: ChunkSerializationErrorKind) -> Self {
        ChunkSerializationError { kind }
    }
}

impl From<io::Error> for ChunkSerializationError {
    fn from(error: io::Error) -> Self {
        ChunkSerializationError { kind: ChunkSerializationErrorKind::Io(error) }
    }
}

impl From<MessageSerializationError> for ChunkSerializationError {
    fn from(error: MessageSerializationError) -> Self {
        ChunkSerializationError { kind: ChunkSerializationErrorKind::SetChunkSizeMessageCreationFailure(error)}
    }
}
