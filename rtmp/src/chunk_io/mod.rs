mod deserialization_errors;
mod deserializer;
mod chunk_header;
mod serialization_errors;
mod serializer;

pub use self::deserialization_errors::{ChunkDeserializationError, ChunkDeserializationErrorKind};
pub use self::serialization_errors::{ChunkSerializationError, ChunkSerializationErrorKind};
pub use self::deserializer::{ChunkDeserializer};
pub use self::serializer::{ChunkSerializer, Packet};

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use ::messages::MessagePayload;
    use ::time::RtmpTimestamp;

    #[test]
    fn can_deserialize_messages_serialized_by_chunk_serializer_struct() {
        let input1 = MessagePayload {
            timestamp: RtmpTimestamp::new(55),
            message_stream_id: 1,
            type_id: 15,
            data: Bytes::from(vec![1, 2, 3, 4, 5, 6]),
        };

        let input2 = MessagePayload {
            timestamp: RtmpTimestamp::new(65),
            message_stream_id: 1,
            type_id: 15,
            data: Bytes::from(vec![8, 9, 10]),
        };

        let input3 = MessagePayload {
            timestamp: RtmpTimestamp::new(75),
            message_stream_id: 1,
            type_id: 15,
            data: Bytes::from(vec![1, 2, 3]),
        };

        let mut serializer = ChunkSerializer::new();
        let packet1 = serializer.serialize(&input1, false, false).unwrap();
        let packet2 = serializer.serialize(&input2, false, false).unwrap();
        let packet3 = serializer.serialize(&input3, false, false).unwrap();

        let mut deserializer = ChunkDeserializer::new();
        let output1 = deserializer.get_next_message(&packet1.bytes).unwrap().unwrap();
        let output2 = deserializer.get_next_message(&packet2.bytes).unwrap().unwrap();
        let output3 = deserializer.get_next_message(&packet3.bytes).unwrap().unwrap();

        assert_eq!(output1, input1, "First message was not deserialized as expected");
        assert_eq!(output2, input2, "Second message was not deserialized as expected");
        assert_eq!(output3, input3, "Third message was not deserialized as expected");
    }

    #[test]
    fn can_deserialize_messages_serialized_with_decreasing_time() {
        let input1 = MessagePayload {
            timestamp: RtmpTimestamp::new(65),
            message_stream_id: 1,
            type_id: 15,
            data: Bytes::from(vec![1, 2, 3, 4, 5, 6]),
        };

        let input2 = MessagePayload {
            timestamp: RtmpTimestamp::new(55),
            message_stream_id: 1,
            type_id: 15,
            data: Bytes::from(vec![8, 9, 10]),
        };

        let input3 = MessagePayload {
            timestamp: RtmpTimestamp::new(45),
            message_stream_id: 1,
            type_id: 15,
            data: Bytes::from(vec![1, 2, 3]),
        };

        let mut serializer = ChunkSerializer::new();
        let packet1 = serializer.serialize(&input1, false, false).unwrap();
        let packet2 = serializer.serialize(&input2, false, false).unwrap();
        let packet3 = serializer.serialize(&input3, false, false).unwrap();

        let mut deserializer = ChunkDeserializer::new();
        let output1 = deserializer.get_next_message(&packet1.bytes).unwrap().unwrap();
        let output2 = deserializer.get_next_message(&packet2.bytes).unwrap().unwrap();
        let output3 = deserializer.get_next_message(&packet3.bytes).unwrap().unwrap();

        assert_eq!(output1, input1, "First message was not deserialized as expected");
        assert_eq!(output2, input2, "Second message was not deserialized as expected");
        assert_eq!(output3, input3, "Third message was not deserialized as expected");
    }
}
