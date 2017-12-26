use std::io;
use std::fmt;
use failure::{Backtrace, Fail};
use rml_amf0::Amf0SerializationError;

/// Error state when serialization errors occur
#[derive(Debug)]
pub struct MessageSerializationError {
    pub kind: MessageSerializationErrorKind
}

/// Enumeration that represents the various errors that may occur while trying to
/// serialize a RTMP message into a raw RTMP payload.
#[derive(Debug, Fail)]
pub enum MessageSerializationErrorKind {
    /// An invalid chunk size value was provided
    #[fail(display = "Cannot serialize a SetChunkSize message with a size of 2147483648 or greater")]
    InvalidChunkSize ,

    /// The values provided could not be serialized into valid AMF0 encoded data
    #[fail(display = "The values provided could not be serialized into valid AMF0 encoded data")]
    Amf0SerializationError(#[cause] Amf0SerializationError),

    /// Failed to read the values from the input buffer
    #[fail(display = "An IO error occurred while writing the output")]
    Io(#[cause] io::Error)
}

impl fmt::Display for MessageSerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for MessageSerializationError {
    fn cause(&self) -> Option<&Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<MessageSerializationErrorKind> for MessageSerializationError {
    fn from(kind: MessageSerializationErrorKind) -> Self {
        MessageSerializationError { kind }
    }
}

impl From<io::Error> for MessageSerializationError {
    fn from(error: io::Error) -> Self {
        MessageSerializationError { kind: MessageSerializationErrorKind::Io(error) }
    }
}

impl From<Amf0SerializationError> for MessageSerializationError {
    fn from(error: Amf0SerializationError) -> Self {
        MessageSerializationError {
            kind: MessageSerializationErrorKind::Amf0SerializationError(error)
        }
    }
}