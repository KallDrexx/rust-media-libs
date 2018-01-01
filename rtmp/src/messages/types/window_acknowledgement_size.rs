use std::io::Cursor;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage};

pub fn serialize(size: u32) -> Result<Vec<u8>, MessageSerializationError> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u32::<BigEndian>(size)?;

    Ok(cursor.into_inner())
}

pub fn deserialize(data: &[u8]) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);
    let size = cursor.read_u32::<BigEndian>()?;

    Ok(RtmpMessage::WindowAcknowledgement {
        size
    })
}

#[cfg(test)]
mod tests {
    use super::{serialize, deserialize};
    use std::io::Cursor;
    use byteorder::{BigEndian, WriteBytesExt};

    use ::messages::RtmpMessage;

    #[test]
    fn can_serialize_message() {
        let size = 523;

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();
        let expected = cursor.into_inner();

        let raw_message = serialize(size).unwrap();
        assert_eq!(raw_message, expected);
    }

    #[test]
    fn can_deserialize_message() {
        let size = 532;
        let expected = RtmpMessage::WindowAcknowledgement { size };

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(size).unwrap();

        let result = deserialize(&cursor.into_inner()[..]).unwrap();
        assert_eq!(result, expected);
    }
}