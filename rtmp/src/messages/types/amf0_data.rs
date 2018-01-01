use std::io::Cursor;
use rml_amf0;
use rml_amf0::Amf0Value;

use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage};

pub fn serialize(values: Vec<Amf0Value>) -> Result<Vec<u8>, MessageSerializationError> {
    let bytes = rml_amf0::serialize(&values)?;

    Ok(bytes)
}

pub fn deserialize(data: &[u8]) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let values = rml_amf0::deserialize(&mut cursor)?;

    Ok(RtmpMessage::Amf0Data { values })
}

#[cfg(test)]
mod tests {
    use super::{serialize, deserialize};
    use std::io::Cursor;
    use rml_amf0::Amf0Value;
    use rml_amf0;

    use ::messages::{RtmpMessage};

    #[test]
    fn can_serialize_message() {
        let raw_message = serialize(vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)]).unwrap();

        let mut cursor = Cursor::new(raw_message);
        let result = rml_amf0::deserialize(&mut cursor).unwrap();
        let expected = vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)];

        assert_eq!(expected, result);
    }

    #[test]
    fn can_deserialize_message() {
        let values = vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)];
        let bytes = rml_amf0::serialize(&values).unwrap();

        let result = deserialize(&bytes[..]).unwrap();

        let expected = RtmpMessage::Amf0Data {
            values: vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)]
        };

        assert_eq!(expected, result);
    }
}