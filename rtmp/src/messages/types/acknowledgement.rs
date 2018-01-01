use std::io::Cursor;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

use ::messages::{MessageDeserializationError, MessageSerializationError};
use ::messages::{RtmpMessage};

pub fn serialize(sequence_number: u32) -> Result<Vec<u8>, MessageSerializationError> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u32::<BigEndian>(sequence_number)?;

    Ok(cursor.into_inner())
}

pub fn deserialize(data: &[u8]) -> Result<RtmpMessage, MessageDeserializationError> {
    let mut cursor = Cursor::new(data);

    Ok(RtmpMessage::Acknowledgement {
        sequence_number: cursor.read_u32::<BigEndian>()?
    })
}

#[cfg(test)]
mod tests {
    use super::{serialize, deserialize};
    use std::io::Cursor;
    use byteorder::{BigEndian, WriteBytesExt};

    use ::messages::{RtmpMessage};

    #[test]
    fn can_serialize_message() {
        let number = 523;
        let result = serialize(number).unwrap();

        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(number).unwrap();

        assert_eq!(cursor.into_inner(), result);
    }

    #[test]
    fn can_deserialize_message() {
        let number = 532;
        let mut cursor = Cursor::new(Vec::new());
        cursor.write_u32::<BigEndian>(number).unwrap();

        let result = deserialize(&cursor.into_inner()[..]).unwrap();

        let expected = RtmpMessage::Acknowledgement { sequence_number: number };
        assert_eq!(expected, result);
    }
}