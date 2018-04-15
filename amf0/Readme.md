This crate provides functions for the serialization and deserialization of AMF0 encoded data.

## Documentation

https://docs.rs/rml_amf0/

## Installation

This crate works with Cargo and is on [crates.io](http://crates.io).  Add it to your `Cargo.toml` like so:
```toml
[dependencies]
rml_amf0 = "0.1"
``` 

## Example

```rust
use std::io::Cursor;
use std::collections::HashMap;
use rml_amf0::{Amf0Value, serialize, deserialize};

// Put some data into the Amf0Value types
let mut properties = HashMap::new();
properties.insert("app".to_string(), Amf0Value::Number(99.0));
properties.insert("second".to_string(), Amf0Value::Utf8String("test".to_string()));

let value1 = Amf0Value::Number(32.0);
let value2 = Amf0Value::Boolean(true);
let object = Amf0Value::Object(properties);

let input = vec![value1, object, value2];

// Serialize the values into a vector of bytes
let serialized_data = serialize(&input).unwrap();

// Deserialize the vector of bytes back into Amf0Value types
let mut serialized_cursor = Cursor::new(serialized_data);
let results = deserialize(&mut serialized_cursor).unwrap();

assert_eq!(input, results);
```


