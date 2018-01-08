mod deserialization_errors;
mod deserializer;
mod chunk_header;
mod serialization_errors;
mod serializer;

pub use self::deserialization_errors::{ChunkDeserializationError, ChunkDeserializationErrorKind};
pub use self::serialization_errors::{ChunkSerializationError, ChunkSerializationErrorKind};
pub use self::deserializer::{ChunkDeserializer};
pub use self::serializer::{ChunkSerializer};

#[cfg(test)]
mod tests {
    use super::*;
    use ::messages::MessagePayload;
    use ::time::RtmpTimestamp;

    #[test]
    fn can_deserialize_messages_serialized_by_chunk_serilalizer_struct() {
        let input1 = MessagePayload {
            timestamp: RtmpTimestamp::new(55),
            message_stream_id: 1,
            type_id: 15,
            data: vec![1, 2, 3, 4, 5, 6]
        };

        let input2 = MessagePayload {
            timestamp: RtmpTimestamp::new(65),
            message_stream_id: 1,
            type_id: 15,
            data: vec![8, 9, 10]
        };

        let input3 = MessagePayload {
            timestamp: RtmpTimestamp::new(75),
            message_stream_id: 1,
            type_id: 15,
            data: vec![1, 2, 3]
        };

        let mut serializer = ChunkSerializer::new();
        let bytes1 = serializer.serialize(&input1, false).unwrap();
        let bytes2 = serializer.serialize(&input2, false).unwrap();
        let bytes3 = serializer.serialize(&input3, false).unwrap();

        let mut deserializer = ChunkDeserializer::new();
        let output1 = deserializer.get_next_message(&bytes1).unwrap().unwrap();
        let output2 = deserializer.get_next_message(&bytes2).unwrap().unwrap();
        let output3 = deserializer.get_next_message(&bytes3).unwrap().unwrap();

        assert_eq!(output1, input1, "First message was not deserialized as expected");
        assert_eq!(output2, input2, "Second message was not deserialized as expected");
        assert_eq!(output3, input3, "Third message was not deserialized as expected");
    }
}