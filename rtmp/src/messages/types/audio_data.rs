use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage};

pub fn serialize(data: Vec<u8>) -> Result<Vec<u8>, MessageSerializationError> {
    Ok(data)
}

pub fn deserialize(data: &[u8]) -> Result<RtmpMessage, MessageDeserializationError> {
    Ok(RtmpMessage::AudioData {
        data: data.to_vec()
    })
}

#[cfg(test)]
mod tests {
    use super::{serialize, deserialize};
    use ::messages::{RtmpMessage};

    #[test]
    fn can_serialize_message() {
        let expected = vec![1,2,3,4];
        let raw_message = serialize(vec![1,2,3,4]).unwrap();

        assert_eq!(raw_message, expected);
    }

    #[test]
    fn can_deserialize_message() {
        let data = vec![1,2,3,4];
        let expected = RtmpMessage::AudioData { data: vec![1,2,3,4] };
        let result = deserialize(&data[..]).unwrap();

        assert_eq!(result, expected);
    }
}