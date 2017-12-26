use std::io::Cursor;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage, RawRtmpMessage};

pub fn serialize(sequence_number: u32) -> Result<RawRtmpMessage, MessageSerializationError> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u32::<BigEndian>(sequence_number)?;

    Ok(RawRtmpMessage{
        data: cursor.into_inner(),
        type_id: 3
    })
}

pub fn deserialize(data: Vec<u8>) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);

    Ok(RtmpMessage::Acknowledgement {
        sequence_number: cursor.read_u32::<BigEndian>()?
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use byteorder::{BigEndian, WriteBytesExt};

    use ::messages::{RtmpMessage, RawRtmpMessage};

    #[test]
    fn can_serialize_message() {
        let number = 523;
        let message = RtmpMessage::Acknowledgement { sequence_number: number };

        let result = message.serialize().unwrap();

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(number).unwrap();
        let expected = RawRtmpMessage { type_id: 3, data: cursor.into_inner() };

        assert_eq!(expected, result);
    }

    #[test]
    fn can_deserialize_message() {
        let number = 532;
        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(number).unwrap();

        let result = RtmpMessage::deserialize(cursor.into_inner(), 3).unwrap();

        let expected = RtmpMessage::Acknowledgement { sequence_number: number };
        assert_eq!(expected, result);
    }
}