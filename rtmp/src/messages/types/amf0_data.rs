use std::io::Cursor;
use rml_amf0;
use rml_amf0::Amf0Value;

use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage, RawRtmpMessage};

pub fn serialize(values: Vec<Amf0Value>) -> Result<RawRtmpMessage, MessageSerializationError> {
    let bytes = rml_amf0::serialize(&values)?;

    Ok(RawRtmpMessage{
        data: bytes,
        type_id: 18
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let values = rml_amf0::deserialize(&mut cursor)?;

    Ok(RtmpMessage::Amf0Data { values })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use rml_amf0::Amf0Value;
    use rml_amf0;

    use ::messages::{RtmpMessage};

    #[test]
    fn can_serialize_message() {
        let message = RtmpMessage::Amf0Data {
            values: vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)]
        };

        let raw_message = RtmpMessage::serialize(message).unwrap();

        let mut cursor = Cursor::new(raw_message.data);
        let result = rml_amf0::deserialize(&mut cursor).unwrap();
        let expected = vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)];

        assert_eq!(expected, result);
        assert_eq!(18, raw_message.type_id);
    }

    #[test]
    fn can_deserialize_message() {
        let values = vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)];
        let bytes = rml_amf0::serialize(&values).unwrap();

        let result = RtmpMessage::deserialize(bytes, 18).unwrap();

        let expected = RtmpMessage::Amf0Data {
            values: vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)]
        };

        assert_eq!(expected, result);
    }
}