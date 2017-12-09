//! This crate provides functionality for serializing and deserializing data
//! based on the Adobe AMF0 encoding specification.  (http://wwwimages.adobe.com/content/dam/Adobe/en/devnet/amf/pdf/amf0-file-format-specification.pdf)
//!
//! # Examples
//! ```
//! use std::io::Cursor;
//! use std::collections::HashMap;
//! use rml_amf0::{Amf0Value, serialize, deserialize};
//!
//! // Put some data into the Amf0Value types
//! let mut properties = HashMap::new();
//! properties.insert("app".to_string(), Amf0Value::Number(99.0));
//! properties.insert("second".to_string(), Amf0Value::Utf8String("test".to_string()));
//!
//! let value1 = Amf0Value::Number(32.0);
//! let value2 = Amf0Value::Boolean(true);
//! let object = Amf0Value::Object(properties);
//!        
//! let input = vec![value1, object, value2];        
//!
//! // Serialize the values into a vector of bytes
//! let serialized_data = serialize(&input).unwrap();
//!
//! // Deserialize the vector of bytes back into Amf0Value types
//! let mut serialized_cursor = Cursor::new(serialized_data);
//! let results = deserialize(&mut serialized_cursor).unwrap();
//!
//! assert_eq!(input, results);
//! ```

#[macro_use] extern crate failure;
extern crate byteorder;

mod serialization;
mod deserialization;
mod errors;

pub use serialization::serialize;
pub use deserialization::deserialize;
pub use errors::{Amf0DeserializationError, Amf0SerializationError};

use std::collections::HashMap;

#[derive(PartialEq, Debug)]
pub enum Amf0Value {
    Number(f64),
    Boolean(bool),
    Utf8String(String),
    Object(HashMap<String, Amf0Value>),
    Null,
}

mod markers {
    pub const NUMBER_MARKER: u8 = 0;
    pub const BOOLEAN_MARKER: u8 = 1;
    pub const STRING_MARKER: u8 = 2;
    pub const OBJECT_MARKER: u8 = 3;
    pub const NULL_MARKER: u8 = 5;
    pub const OBJECT_END_MARKER: u8 = 9;
    pub const UTF_8_EMPTY_MARKER: u16 = 0;
}
