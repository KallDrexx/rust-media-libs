use std::io::Cursor;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use ::messages::{MessageDeserializationError, MessageDeserializationErrorKind, MessageSerializationError, MessageSerializationErrorKind};
use ::messages::{RtmpMessage, RawRtmpMessage};

const MAX_SIZE: u32 = 0x80000000 - 1;

pub fn serialize(size: u32) -> Result<RawRtmpMessage, MessageSerializationError> {
    if size > MAX_SIZE {
        return Err(MessageSerializationError {kind: MessageSerializationErrorKind::InvalidChunkSize});
    }

    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u32::<BigEndian>(size)?;

    Ok(RawRtmpMessage{
        data: cursor.into_inner(),
        type_id: 1
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let size = cursor.read_u32::<BigEndian>()?;

    if size > MAX_SIZE {
        return Err(MessageDeserializationError {kind: MessageDeserializationErrorKind::InvalidMessageFormat});
    }

    Ok(RtmpMessage::SetChunkSize { size })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use byteorder::{BigEndian, WriteBytesExt};

    use ::messages::{RtmpMessage};

    #[test]
    fn can_serialize_message() {
        let size = 523;
        let message = RtmpMessage::SetChunkSize { size };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        let expected = cursor.into_inner();

        let raw_message = message.serialize().unwrap();

        assert_eq!(raw_message.data, expected);
        assert_eq!(raw_message.type_id, 1);
    }

    #[test]
    fn can_deserialize_message() {
        let size = 532;
        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();

        let result = RtmpMessage::deserialize(cursor.into_inner(), 1).unwrap();
        let expected = RtmpMessage::SetChunkSize { size };
        assert_eq!(result, expected);
    }
}