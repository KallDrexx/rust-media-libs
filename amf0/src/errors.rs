use std::{io, string};

#[derive(Debug,Fail)]
pub enum Amf0DeserializationError {
    #[fail(display = "Encountered unknown marker: {}", marker)]
    UnknownMarker{
        marker: u8
    },

    #[fail(display = "Unexpected empty object propery name")]
    UnexpectedEmptyObjectPropertyName,

    #[fail(display = "Hit end of the byte buffer but was expecting more data")]
    UnexpectedEof,

    #[fail(display = "Failed to read byte buffer: {}", _0)]
    BufferReadError(#[cause] io::Error),

    #[fail(display = "Failed to read a utf8 string from the byte buffer: {}", _0)]
    StringParseError(#[cause] string::FromUtf8Error)
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

#[derive(Debug, Fail)]
pub enum Amf0SerializationError {
    #[fail(display = "String length greater than 65,535")]
    NormalStringTooLong,

    #[fail(display = "Failed to write to byte buffer")]
    BufferWriteError(#[cause] io::Error)
}

impl From<io::Error> for Amf0SerializationError {
    fn from(error: io::Error) -> Self {
        Amf0SerializationError::BufferWriteError(error)
    }
}