use std::{io, string};

/// Errors that can occur during the deserialization process
#[derive(Debug, Fail)]
pub enum Amf0DeserializationError {
    /// Every Amf0 value starts with a marker byte describing the type of value that was
    /// encoded.  For example a marker of `0x00` is a number, `0x01` is a string, etc..
    ///
    /// This error is encountered when we see a maker value that we do not recognize.
    #[fail(display = "Encountered unknown marker: {}", marker)]
    UnknownMarker { marker: u8 },

    /// Object properties consist of a name and value pair.  It is expected that every property
    /// has a valid string name, and if the name is empty this error is raised.
    #[fail(display = "Unexpected empty object property name")]
    UnexpectedEmptyObjectPropertyName,

    /// This occurs when we are expecting more data but hit the end of the buffer
    /// (e.g. we are reading an object property but there was no property value).
    #[fail(display = "Hit end of the byte buffer but was expecting more data")]
    UnexpectedEof,

    /// An I/O Error occurred while reading the data buffer
    #[fail(display = "Failed to read byte buffer: {}", _0)]
    BufferReadError(#[cause] io::Error),

    /// Strings in AMF0 are UTF-8 encoded, so if the bytes read are not valid
    /// UTF-8 this error will be raised.
    #[fail(display = "Failed to read a utf8 string from the byte buffer: {}", _0)]
    StringParseError(#[cause] string::FromUtf8Error),
}

// Since an IO error can only be thrown while reading the buffer, auto-conversion should work
impl From<io::Error> for Amf0DeserializationError {
    fn from(error: io::Error) -> Self {
        Amf0DeserializationError::BufferReadError(error)
    }
}

impl From<string::FromUtf8Error> for Amf0DeserializationError {
    fn from(error: string::FromUtf8Error) -> Self {
        Amf0DeserializationError::StringParseError(error)
    }
}

/// Errors raised during to the serialization process
#[derive(Debug, Fail)]
pub enum Amf0SerializationError {
    /// Amf0 strings cannot be more than 65,535 characters, so if a string was provided
    /// with a larger length than this than this error is raised.
    #[fail(display = "String length greater than 65,535")]
    NormalStringTooLong,

    /// An I/O error occurred while writing to the output buffer.
    #[fail(display = "Failed to write to byte buffer")]
    BufferWriteError(#[cause] io::Error),
}

impl From<io::Error> for Amf0SerializationError {
    fn from(error: io::Error) -> Self {
        Amf0SerializationError::BufferWriteError(error)
    }
}
