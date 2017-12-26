use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage, RawRtmpMessage};

pub fn serialize(data: Vec<u8>) -> Result<RawRtmpMessage, MessageSerializationError> {
    Ok(RawRtmpMessage{
        data,
        type_id: 8
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    Ok(RtmpMessage::AudioData {
        data
    })
}

#[cfg(test)]
mod tests {
    use ::messages::{RtmpMessage};

    #[test]
    fn can_serialize_message() {
        let message = RtmpMessage::AudioData { data: vec![1,2,3,4] };
        let expected = vec![1,2,3,4];
        let raw_message = message.serialize().unwrap();

        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 8);
    }

    #[test]
    fn can_deserialize_message() {
        let data = vec![1,2,3,4];
        let expected = RtmpMessage::AudioData { data: vec![1,2,3,4] };
        let result = RtmpMessage::deserialize(data, 8).unwrap();

        assert_eq!(result, expected);
    }
}