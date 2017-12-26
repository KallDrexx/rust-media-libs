use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage, RawRtmpMessage};

pub fn serialize(bytes: Vec<u8>) -> Result<RawRtmpMessage, MessageSerializationError> {
    Ok(RawRtmpMessage{
        data: bytes,
        type_id: 9
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    Ok(RtmpMessage::VideoData {
        data
    })
}

#[cfg(test)]
mod tests {
    use ::messages::RtmpMessage;

    #[test]
    fn can_serialize_message() {
        let message = RtmpMessage::VideoData { data: vec![1,2,3,4] };
        let expected = vec![1,2,3,4];

        let raw_message = message.serialize().unwrap();

        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 9);
    }

    #[test]
    fn can_deserialize_message() {
        let data = vec![1,2,3,4];
        let expected = RtmpMessage::VideoData { data: vec![1,2,3,4] };

        let result = RtmpMessage::deserialize(data, 9).unwrap();
        assert_eq!(result, expected);
    }
}