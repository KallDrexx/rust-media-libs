use ::time::RtmpTimestamp;
use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::RtmpMessage;
use super::types;

/// Represents a raw RTMP message
#[derive(PartialEq, Debug)]
pub struct MessagePayload {
    pub timestamp: RtmpTimestamp,
    pub type_id: u8,
    pub message_stream_id: u32,
    pub data: Vec<u8>,
}

impl MessagePayload {
    pub fn new() -> MessagePayload {
        MessagePayload {
            timestamp: RtmpTimestamp::new(0),
            message_stream_id: 0,
            type_id: 0,
            data: Vec::new(),
        }
    }

    pub fn to_rtmp_message(&self) -> Result<RtmpMessage, MessageDeserializationError> {
        match self.type_id {
            1 => types::set_chunk_size::deserialize(&self.data[..]),
            2 => types::abort::deserialize(&self.data[..]),
            3 => types::acknowledgement::deserialize(&self.data[..]),
            4 => types::user_control::deserialize(&self.data[..]),
            5 => types::window_acknowledgement_size::deserialize(&self.data[..]),
            6 => types::set_peer_bandwidth::deserialize(&self.data[..]),
            8 => types::audio_data::deserialize(&self.data[..]),
            9 => types::video_data::deserialize(&self.data[..]),
            18 => types::amf0_data::deserialize(&self.data[..]),
            20 => types::amf0_command::deserialize(&self.data[..]),
            _ => Ok(RtmpMessage::Unknown { type_id: self.type_id, data: self.data.clone() })
        }
    }

    pub fn from_rtmp_message(message: RtmpMessage, timestamp: RtmpTimestamp, message_stream_id: u32) -> Result<MessagePayload, MessageSerializationError> {
        let type_id = get_message_type_id(&message);

        let bytes = match message {
            RtmpMessage::Unknown { type_id: _, data }
            => data,

            RtmpMessage::Abort { stream_id }
            => types::abort::serialize(stream_id)?,

            RtmpMessage::Acknowledgement { sequence_number }
            => types::acknowledgement::serialize(sequence_number)?,

            RtmpMessage::Amf0Command { command_name, transaction_id, command_object, additional_arguments }
            => types::amf0_command::serialize(command_name, transaction_id, command_object, additional_arguments)?,

            RtmpMessage::Amf0Data { values }
            => types::amf0_data::serialize(values)?,

            RtmpMessage::AudioData { data }
            => types::audio_data::serialize(data)?,

            RtmpMessage::SetChunkSize { size }
            => types::set_chunk_size::serialize(size)?,

            RtmpMessage::SetPeerBandwidth { size, limit_type }
            => types::set_peer_bandwidth::serialize(limit_type, size)?,

            RtmpMessage::UserControl { event_type, stream_id, buffer_length, timestamp }
            => types::user_control::serialize(event_type, stream_id, buffer_length, timestamp)?,

            RtmpMessage::VideoData { data }
            => types::video_data::serialize(data)?,

            RtmpMessage::WindowAcknowledgement { size }
            => types::window_acknowledgement_size::serialize(size)?,
        };

        Ok(MessagePayload {
            data: bytes,
            type_id,
            message_stream_id,
            timestamp
        })
    }
}

fn get_message_type_id(message: &RtmpMessage) -> u8 {
    match *message {
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

#[cfg(test)]
mod tests {
    use super::{RtmpMessage, MessagePayload};
    use ::messages::{PeerBandwidthLimitType, UserControlEventType};
    use ::time::RtmpTimestamp;
    use rml_amf0::Amf0Value;

    #[test]
    fn can_get_payload_from_abort_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::Abort { stream_id: 23 };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 2, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_acknowledgement_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::Acknowledgement { sequence_number: 23 };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 3, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_amf0_command_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::Amf0Command {
            command_name: "test".to_string(),
            command_object: Amf0Value::Null,
            transaction_id: 23.0,
            additional_arguments: vec![]
        };

        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 20, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_amf0_data_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::Amf0Data { values: vec![Amf0Value::Number(23.0)] };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 18, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_audio_data_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::AudioData { data: vec![33_u8] };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 8, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_set_chunk_size_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::SetChunkSize { size: 33 };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 1, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_set_peer_bandwidth_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::SetPeerBandwidth { size:33, limit_type: PeerBandwidthLimitType::Hard };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 6, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_user_control_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::UserControl { event_type: UserControlEventType::StreamBegin,
            stream_id: Some(33),
            timestamp: None,
            buffer_length: None
        };

        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 4, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_video_data_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::VideoData { data: vec![23_u8] };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 9, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_window_acknowledgement_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::WindowAcknowledgement { size: 23 };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 5, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_payload_from_unknown_message() {
        let timestamp = RtmpTimestamp::new(55);
        let stream_id = 52;
        let message = RtmpMessage::Unknown { type_id: 33, data: vec![23_u8] };
        let result = MessagePayload::from_rtmp_message(message, timestamp, stream_id).unwrap();

        assert_ne!(result.data.len(), 0, "Empty payload data seen");
        assert_eq!(result.type_id, 33, "Incorrect type id");
        assert_eq!(result.message_stream_id, stream_id, "Incorrect message stream id");
        assert_eq!(result.timestamp, 55, "Incorrect timestamp");
    }

    #[test]
    fn can_get_rtmp_message_for_abort_payload() {
        let message = RtmpMessage::Abort { stream_id: 15 };
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_acknowledgement_payload() {
        let message = RtmpMessage::Acknowledgement { sequence_number:15 };
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_amf0_command_payload() {
        let message = RtmpMessage::Amf0Command {
            command_name: "test".to_string(),
            transaction_id: 15.0,
            command_object: Amf0Value::Number(23.0),
            additional_arguments: vec![Amf0Value::Null]
        };

        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_amf0_data_payload() {
        let message = RtmpMessage::Amf0Data { values: vec![Amf0Value::Number(23.3)]};
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_audio_data_payload() {
        let message = RtmpMessage::AudioData { data: vec![3_u8]};
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_set_chunk_size_payload() {
        let message = RtmpMessage::SetChunkSize { size: 15 };
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_set_peer_bandwidth_payload() {
        let message = RtmpMessage::SetPeerBandwidth {size: 15, limit_type: PeerBandwidthLimitType::Hard};
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_user_control_payload() {
        let message = RtmpMessage::UserControl {
            stream_id: Some(15),
            buffer_length: None,
            timestamp: None,
            event_type: UserControlEventType::StreamBegin
        };

        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_video_data_payload() {
        let message = RtmpMessage::VideoData {data: vec![3_u8]};
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }

    #[test]
    fn can_get_rtmp_message_for_window_acknowledgement_payload() {
        let message = RtmpMessage::WindowAcknowledgement {size:25};
        let payload = MessagePayload::from_rtmp_message(message.clone(), RtmpTimestamp::new(0), 15).unwrap();
        let result = payload.to_rtmp_message().unwrap();

        assert_eq!(result, message);
    }
}

