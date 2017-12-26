mod types;
mod deserialization_errors;
mod serialization_errors;

pub use self::deserialization_errors::{MessageDeserializationError, MessageDeserializationErrorKind};
pub use self::serialization_errors::{MessageSerializationError, MessageSerializationErrorKind};
use rml_amf0::Amf0Value;
use ::time::RtmpTimestamp;

#[derive(Eq, PartialEq, Debug)]
pub enum PeerBandwidthLimitType { Hard, Soft, Dynamic }

#[derive(Eq, PartialEq, Debug)]
pub enum UserControlEventType {
    StreamBegin,
    StreamEof,
    StreamDry,
    SetBufferLength,
    StreamIsRecorded,
    PingRequest,
    PingResponse
}

#[derive(PartialEq, Debug)]
pub enum RtmpMessage {
    Unknown { type_id: u8, data: Vec<u8> },
    Abort { stream_id: u32 },
    Acknowledgement { sequence_number: u32 },
    Amf0Command { command_name: String, transaction_id: f64, command_object: Amf0Value, additional_arguments: Vec<Amf0Value> },
    Amf0Data { values: Vec<Amf0Value> },
    AudioData { data: Vec<u8> },
    SetChunkSize { size: u32 },
    SetPeerBandwidth { size: u32, limit_type: PeerBandwidthLimitType },
    UserControl { event_type: UserControlEventType, stream_id: Option<u32>, buffer_length: Option<u32>, timestamp: Option<RtmpTimestamp> },
    VideoData { data: Vec<u8> },
    WindowAcknowledgement { size: u32 }
}

#[derive(PartialEq, Debug)]
pub struct RawRtmpMessage {
    pub data: Vec<u8>,
    pub type_id: u8
}

impl RtmpMessage {
    pub fn serialize(self) -> Result<RawRtmpMessage, MessageSerializationError> {
        match self {
            RtmpMessage::Unknown { type_id, data }
            => Ok(RawRtmpMessage { type_id, data }),

            RtmpMessage::Abort { stream_id }
            => types::abort::serialize(stream_id),

            RtmpMessage::Acknowledgement { sequence_number }
            => types::acknowledgement::serialize(sequence_number),

            RtmpMessage::Amf0Command { command_name, transaction_id, command_object, additional_arguments }
            => types::amf0_command::serialize(command_name, transaction_id, command_object, additional_arguments),

            RtmpMessage::Amf0Data { values }
            => types::amf0_data::serialize(values),

            RtmpMessage::AudioData { data }
            => types::audio_data::serialize(data),

            RtmpMessage::SetChunkSize { size }
            => types::set_chunk_size::serialize(size),

            RtmpMessage::SetPeerBandwidth { size, limit_type }
            => types::set_peer_bandwidth::serialize(limit_type, size),

            RtmpMessage::UserControl { event_type, stream_id, buffer_length, timestamp }
            => types::user_control::serialize(event_type, stream_id, buffer_length, timestamp),

            RtmpMessage::VideoData { data }
            => types::video_data::serialize(data),

            RtmpMessage::WindowAcknowledgement { size }
            => types::window_acknowledgement_size::serialize(size),
        }
    }

    pub fn deserialize(bytes: Vec<u8>, type_id: u8) -> Result<Self, MessageDeserializationError> {
        match type_id {
            1 => types::set_chunk_size::deserialize(bytes),
            2 => types::abort::deserialize(bytes),
            3 => types::acknowledgement::deserialize(bytes),
            4 => types::user_control::deserialize(bytes),
            5 => types::window_acknowledgement_size::deserialize(bytes),
            6 => types::set_peer_bandwidth::deserialize(bytes),
            8 => types::audio_data::deserialize(bytes),
            9 => types::video_data::deserialize(bytes),
            18 => types::amf0_data::deserialize(bytes),
            20 => types::amf0_command::deserialize(bytes),
            _ => Ok(RtmpMessage::Unknown { type_id, data: bytes })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RtmpMessage, RawRtmpMessage};

    #[test]
    fn can_serialize_unknown_message() {
        let message = RtmpMessage::Unknown { type_id: 29, data: vec![1,2,3,4] };
        let expected = RawRtmpMessage { type_id: 29, data: vec![1,2,3,4] };

        let result = message.serialize().unwrap();

        assert_eq!(expected, result);
    }

    #[test]
    fn can_deserialize_unknown_message() {
        let id = 255;
        let expected = RtmpMessage::Unknown { type_id: id, data: vec![1,2,3,4] };
        let result = RtmpMessage::deserialize(vec![1,2,3,4], id).unwrap();
        assert_eq!(expected, result);
    }
}