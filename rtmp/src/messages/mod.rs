mod types;
mod deserialization_errors;
mod serialization_errors;
mod message_payload;

pub use self::deserialization_errors::{MessageDeserializationError, MessageDeserializationErrorKind};
pub use self::serialization_errors::{MessageSerializationError, MessageSerializationErrorKind};
pub use self::message_payload::MessagePayload;
use bytes::Bytes;
use rml_amf0::Amf0Value;
use ::time::RtmpTimestamp;

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum PeerBandwidthLimitType { Hard, Soft, Dynamic }

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum UserControlEventType {
    StreamBegin,
    StreamEof,
    StreamDry,
    SetBufferLength,
    StreamIsRecorded,
    PingRequest,
    PingResponse
}

#[derive(PartialEq, Debug, Clone)]
pub enum RtmpMessage {
    Unknown { type_id: u8, data: Bytes },
    Abort { stream_id: u32 },
    Acknowledgement { sequence_number: u32 },
    Amf0Command { command_name: String, transaction_id: f64, command_object: Amf0Value, additional_arguments: Vec<Amf0Value> },
    Amf0Data { values: Vec<Amf0Value> },
    AudioData { data: Bytes },
    SetChunkSize { size: u32 },
    SetPeerBandwidth { size: u32, limit_type: PeerBandwidthLimitType },
    UserControl { event_type: UserControlEventType, stream_id: Option<u32>, buffer_length: Option<u32>, timestamp: Option<RtmpTimestamp> },
    VideoData { data: Bytes },
    WindowAcknowledgement { size: u32 }
}

impl RtmpMessage {
    pub fn into_message_payload(self, timestamp: RtmpTimestamp, message_stream_id: u32) -> Result<MessagePayload, MessageSerializationError> {
        MessagePayload::from_rtmp_message(self, timestamp, message_stream_id)
    }

    pub fn get_message_type_id(&self) -> u8 {
        match *self {
            RtmpMessage::Unknown { type_id, data: _ } => type_id,
            RtmpMessage::Abort { stream_id: _ } => 2_u8,
            RtmpMessage::Acknowledgement { sequence_number: _ } => 3_u8,
            RtmpMessage::Amf0Command { command_name: _, transaction_id: _, command_object: _, additional_arguments: _ } => 20_u8,
            RtmpMessage::Amf0Data { values: _ } => 18_u8,
            RtmpMessage::AudioData { data: _ } => 8_u8,
            RtmpMessage::SetChunkSize { size: _ } => 1_u8,
            RtmpMessage::SetPeerBandwidth { size: _, limit_type: _ } => 6_u8,
            RtmpMessage::UserControl { event_type: _, stream_id: _, buffer_length: _, timestamp: _ } => 4_u8,
            RtmpMessage::VideoData { data: _ } => 9_u8,
            RtmpMessage::WindowAcknowledgement { size: _ } => 5_u8,
        }
    }
}
