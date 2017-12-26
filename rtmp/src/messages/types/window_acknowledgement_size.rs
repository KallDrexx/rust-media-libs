use std::io::Cursor;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage, RawRtmpMessage};

pub fn serialize(size: u32) -> Result<RawRtmpMessage, MessageSerializationError> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u32::<BigEndian>(size)?;

    Ok(RawRtmpMessage{
        data: cursor.into_inner(),
        type_id: 5
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let size = cursor.read_u32::<BigEndian>()?;

    Ok(RtmpMessage::WindowAcknowledgement {
        size
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use byteorder::{BigEndian, WriteBytesExt};

    use ::messages::RtmpMessage;

    #[test]
    fn can_serialize_message() {
        let size = 523;
        let message = RtmpMessage::WindowAcknowledgement { size };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        let expected = cursor.into_inner();

        let raw_message = message.serialize().unwrap();
        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 5);
    }

    #[test]
    fn can_deserialize_message() {
        let size = 532;
        let expected = RtmpMessage::WindowAcknowledgement { size };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();

        let result = RtmpMessage::deserialize(cursor.into_inner(), 5).unwrap();
        assert_eq!(result, expected);
    }
}