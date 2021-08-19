use failure::{Backtrace, Fail};
use messages::MessageSerializationError;
use std::fmt;
use std::io;

/// Data for when an error occurs while attempting to serialize an RTMP message into RTMP chunks.
#[derive(Debug)]
pub struct ChunkSerializationError {
    /// The kind of error that occurred
    pub kind: ChunkSerializationErrorKind,
}

/// An enumeration defining all the possible errors that could occur while serializing
/// RTMP messages into RTMP chunks.
#[derive(Debug, Fail)]
pub enum ChunkSerializationErrorKind {
    /// Te RTMP specification states that a message cannot be more than 16,777,215 bytes, even
    /// when split across multiple RTMP chunks.  This error is returned if an RTMP message is passed
    /// in that is larger than this amount.
    #[fail(
        display = "The current message has a length of {} bytes, which is over the allowed size of 16777215 bytes",
        size
    )]
    MessageTooLong { size: u32 },

    /// The RTMP spec does not allow chunk sizes more than 2,147,483,647 (since it's encoded in only
    /// 31 bytes of the SetChunkSize message), so this error occurs when a chunk size of greater than
    /// this value is attempted to be set
    #[fail(
        display = "An invalid chunk size was specified.  Chunk size must be greater than 0 and less than 2147483647"
    )]
    InvalidMaxChunkSize { attempted_chunk_size: u32 },

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
    fn cause(&self) -> Option<&dyn Fail> {
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
        ChunkSerializationError {
            kind: ChunkSerializationErrorKind::Io(error),
        }
    }
}

impl From<MessageSerializationError> for ChunkSerializationError {
    fn from(error: MessageSerializationError) -> Self {
        ChunkSerializationError {
            kind: ChunkSerializationErrorKind::SetChunkSizeMessageCreationFailure(error),
        }
    }
}
