use failure::{Backtrace, Fail};
use std::fmt;
use std::io;

/// Data for when an error occurs while attempting to deserialize a RTMP chunk
#[derive(Debug)]
pub struct ChunkDeserializationError {
    /// The kind of error that occurred
    pub kind: ChunkDeserializationErrorKind,
}

/// An enumeration defining all the possible errors that could occur while deserializing
/// RTMP chunks.
#[derive(Debug, Fail)]
pub enum ChunkDeserializationErrorKind {
    /// The RTMP chunk format requires that RTMP chunks that are not type 0 utilize information
    /// from the previously received chunk on that same chunk stream id.  This error occurs when a
    /// non-0 chunk is received on a stream that has not received a type 0 chunk yet.
    #[fail(
        display = "Received chunk with non-zero chunk type on csid {} prior to receiving a type 0 chunk",
        csid
    )]
    NoPreviousChunkOnStream { csid: u32 },

    /// The max chunk size does not allow chunk sizes more than 2,147,483,647 (since it's encoded in only
    /// 31 bytes of the SetChunkSize message), so this error occurs when a chunk size of greater than
    /// this value is attempted to be set
    #[fail(
        display = "Requested an invalid max chunk size of {}.  The largest chunk size possible is 2147483647",
        chunk_size
    )]
    InvalidMaxChunkSize { chunk_size: usize },

    /// An I/O error occurred while reading the input buffer
    #[fail(display = "_0")]
    Io(#[cause] io::Error),
}

impl fmt::Display for ChunkDeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for ChunkDeserializationError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<ChunkDeserializationErrorKind> for ChunkDeserializationError {
    fn from(kind: ChunkDeserializationErrorKind) -> Self {
        ChunkDeserializationError { kind }
    }
}

impl From<io::Error> for ChunkDeserializationError {
    fn from(error: io::Error) -> Self {
        ChunkDeserializationError {
            kind: ChunkDeserializationErrorKind::Io(error),
        }
    }
}
