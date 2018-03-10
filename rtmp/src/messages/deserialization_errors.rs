use std::io;
use std::fmt;
use failure::{Backtrace, Fail};
use rml_amf0::Amf0DeserializationError;

/// Error state when deserialization errors occur
#[derive(Debug)]
pub struct MessageDeserializationError {
    pub kind: MessageDeserializationErrorKind
}

/// Enumeration that represents the various errors that may occur while trying to
/// deserialize a RTMP message
#[derive(Debug, Fail)]
pub enum MessageDeserializationErrorKind {
    /// The bytes or amf0 values contained in the message were not what were expected, and thus
    /// the message could not be parsed.
    #[fail(display = "The message was not encoded in an expected format")]
    InvalidMessageFormat,

    /// The bytes in the message that were expected to be AMF0 values were not properly encoded,
    /// and thus could not be read
    #[fail(display = "The message did no contain valid Amf0 encoded values: {}", _0)]
    Amf0DeserializationError(#[cause] Amf0DeserializationError),

    /// Failed to read the values from the input buffer
    #[fail(display = "An IO error occurred while reading the input: {}", _0)]
    Io(#[cause] io::Error)
}

impl fmt::Display for MessageDeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for MessageDeserializationError {
    fn cause(&self) -> Option<&Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<MessageDeserializationErrorKind> for MessageDeserializationError {
    fn from(kind: MessageDeserializationErrorKind) -> Self {
        MessageDeserializationError { kind }
    }
}

impl From<io::Error> for MessageDeserializationError {
    fn from(error: io::Error) -> Self {
        MessageDeserializationError { kind: MessageDeserializationErrorKind::Io(error) }
    }
}

impl From<Amf0DeserializationError> for MessageDeserializationError {
    fn from(error: Amf0DeserializationError) -> Self {
        MessageDeserializationError {
            kind: MessageDeserializationErrorKind::Amf0DeserializationError(error)
        }
    }
}