use std::io::Cursor;
use rml_amf0;
use rml_amf0::Amf0Value;

use ::messages::{MessageDeserializationErrorKind, MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage, RawRtmpMessage};

pub fn serialize(command_name: String,
                 transaction_id: f64,
                 command_object: Amf0Value,
                 mut additional_arguments: Vec<Amf0Value>) -> Result<RawRtmpMessage, MessageSerializationError> {
    let mut values = vec![
        Amf0Value::Utf8String(command_name),
        Amf0Value::Number(transaction_id),
        command_object
    ];

    values.append(&mut additional_arguments);
    let bytes = rml_amf0::serialize(&values)?;

    Ok(RawRtmpMessage{
        data: bytes,
        type_id: 20
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let mut arguments = rml_amf0::deserialize(&mut cursor)?;

    let command_name: String;
    let transaction_id: f64;
    let command_object: Amf0Value;
    {
        let mut arg_iterator = arguments.drain(..3);

        command_name = match arg_iterator.next().ok_or(MessageDeserializationErrorKind::InvalidMessageFormat)? {
            Amf0Value::Utf8String(value) => value,
            _ => return Err(MessageDeserializationError { kind: MessageDeserializationErrorKind::InvalidMessageFormat })
        };

        transaction_id = match arg_iterator.next().ok_or(MessageDeserializationErrorKind::InvalidMessageFormat)? {
            Amf0Value::Number(value) => value,
            _ => return Err(MessageDeserializationError { kind: MessageDeserializationErrorKind::InvalidMessageFormat })
        };

        command_object = arg_iterator.next().ok_or(MessageDeserializationErrorKind::InvalidMessageFormat)?;
    }

    Ok(RtmpMessage::Amf0Command {
        command_name,
        transaction_id,
        command_object,
        additional_arguments: arguments
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::collections::HashMap;
    use rml_amf0::Amf0Value;
    use rml_amf0;

    use ::messages::RtmpMessage;

    #[test]
    fn can_serialize_message() {
        let mut properties1 = HashMap::new();
        properties1.insert("prop1".to_string(), Amf0Value::Utf8String("abc".to_string()));
        properties1.insert("prop2".to_string(), Amf0Value::Null);

        let mut properties2 = HashMap::new();
        properties2.insert("prop1".to_string(), Amf0Value::Utf8String("abc".to_string()));
        properties2.insert("prop2".to_string(), Amf0Value::Null);

        let message = RtmpMessage::Amf0Command {
            command_name: "test".to_string(),
            transaction_id: 23.0,
            command_object: Amf0Value::Object(properties1),
            additional_arguments: vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)]
        };

        let raw_message = message.serialize().unwrap();
        let mut cursor = Cursor::new(raw_message.data);
        let result = rml_amf0::deserialize(&mut cursor).unwrap();

        let expected = vec![
            Amf0Value::Utf8String("test".to_string()),
            Amf0Value::Number(23.0),
            Amf0Value::Object(properties2),
            Amf0Value::Boolean(true),
            Amf0Value::Number(52.0)
        ];

        assert_eq!(expected, result);
        assert_eq!(20, raw_message.type_id);
    }

    #[test]
    fn can_deserialize_message() {
        let mut properties1 = HashMap::new();
        properties1.insert("prop1".to_string(), Amf0Value::Utf8String("abc".to_string()));
        properties1.insert("prop2".to_string(), Amf0Value::Null);

        let mut properties2 = HashMap::new();
        properties2.insert("prop1".to_string(), Amf0Value::Utf8String("abc".to_string()));
        properties2.insert("prop2".to_string(), Amf0Value::Null);

        let values = vec![
            Amf0Value::Utf8String("test".to_string()),
            Amf0Value::Number(23.0),
            Amf0Value::Object(properties1),
            Amf0Value::Boolean(true),
            Amf0Value::Number(52.0)
        ];

        let bytes = rml_amf0::serialize(&values).unwrap();

        let expected = RtmpMessage::Amf0Command {
            command_name: "test".to_string(),
            transaction_id: 23.0,
            command_object: Amf0Value::Object(properties2),
            additional_arguments: vec![Amf0Value::Boolean(true), Amf0Value::Number(52.0)]
        };

        let result = RtmpMessage::deserialize(bytes, 20).unwrap();

        assert_eq!(expected, result);
    }
}