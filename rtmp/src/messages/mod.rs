mod types;
mod deserialization_errors;
mod serialization_errors;
mod message_payload;

pub use self::deserialization_errors::{MessageDeserializationError, MessageDeserializationErrorKind};
pub use self::serialization_errors::{MessageSerializationError, MessageSerializationErrorKind};
pub use self::message_payload::MessagePayload;
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
