use std::io::Cursor;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use ::messages::{MessageDeserializationError, MessageSerializationError, MessageDeserializationErrorKind};
use ::messages::{RtmpMessage, RawRtmpMessage, PeerBandwidthLimitType};

pub fn serialize(limit_type: PeerBandwidthLimitType, size: u32) -> Result<RawRtmpMessage, MessageSerializationError> {
    let type_id = match limit_type {
        PeerBandwidthLimitType::Hard => 0,
        PeerBandwidthLimitType::Soft => 1,
        PeerBandwidthLimitType::Dynamic => 2
    };

    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u32::<BigEndian>(size)?;
    cursor.write_u8(type_id)?;

    Ok(RawRtmpMessage{
        data: cursor.into_inner(),
        type_id: 6
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let size = cursor.read_u32::<BigEndian>()?;
    let limit_type = match cursor.read_u8()? {
        0 => PeerBandwidthLimitType::Hard,
        1 => PeerBandwidthLimitType::Soft,
        2 => PeerBandwidthLimitType::Dynamic,
        _ => return Err(MessageDeserializationError {kind: MessageDeserializationErrorKind::InvalidMessageFormat})
    };

    Ok(RtmpMessage::SetPeerBandwidth {
        size,
        limit_type
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use byteorder::{BigEndian, WriteBytesExt};

    use ::messages::{RtmpMessage, PeerBandwidthLimitType};

    #[test]
    fn can_serialize_message_with_soft_limit_type() {
        let size = 523;
        let message = RtmpMessage::SetPeerBandwidth { size, limit_type: PeerBandwidthLimitType::Soft };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        cursor.write_u8(1).unwrap();
        let expected = cursor.into_inner();

        let raw_message = message.serialize().unwrap();
        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 6);
    }

    #[test]
    fn can_serialize_message_with_hard_limit_type() {
        let size = 523;
        let message = RtmpMessage::SetPeerBandwidth { size, limit_type: PeerBandwidthLimitType::Hard };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        cursor.write_u8(0).unwrap();
        let expected = cursor.into_inner();

        let raw_message = message.serialize().unwrap();
        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 6);
    }

    #[test]
    fn can_serialize_message_with_dynamic_limit_type() {
        let size = 523;
        let message = RtmpMessage::SetPeerBandwidth { size, limit_type: PeerBandwidthLimitType::Dynamic };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        cursor.write_u8(2).unwrap();
        let expected = cursor.into_inner();

        let raw_message = message.serialize().unwrap();
        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 6);
    }

    #[test]
    fn can_deserialize_message_with_hard_limit_type() {
        let size = 523;
        let expected = RtmpMessage::SetPeerBandwidth { size, limit_type: PeerBandwidthLimitType::Hard };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        cursor.write_u8(0).unwrap();
        let data = cursor.into_inner();

        let result = RtmpMessage::deserialize(data, 6).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn can_deserialize_message_with_soft_limit_type() {
        let size = 523;
        let expected = RtmpMessage::SetPeerBandwidth { size, limit_type: PeerBandwidthLimitType::Soft };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        cursor.write_u8(1).unwrap();
        let data = cursor.into_inner();

        let result = RtmpMessage::deserialize(data, 6).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn can_deserialize_message_with_dynamic_limit_type() {
        let size = 523;
        let expected = RtmpMessage::SetPeerBandwidth { size, limit_type: PeerBandwidthLimitType::Dynamic };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        cursor.write_u8(2).unwrap();
        let data = cursor.into_inner();

        let result = RtmpMessage::deserialize(data, 6).unwrap();
        assert_eq!(result, expected);
    }
}