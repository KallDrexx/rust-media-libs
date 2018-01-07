use std::collections::{HashMap};
use std::cmp::min;
use std::io::{Cursor, Write};
use byteorder::{BigEndian, WriteBytesExt, LittleEndian};
use ::messages::{MessagePayload, RtmpMessage};
use ::chunk_io::{ChunkSerializationError, ChunkSerializationErrorKind};
use ::time::RtmpTimestamp;
use super::chunk_header::{ChunkHeader, ChunkHeaderFormat};

const INITIAL_MAX_CHUNK_SIZE: u32 = 128;
const MAX_INITIAL_TIMESTAMP: u32 = 16777215;

/// Allows serializing RTMP messages into RTMP chunks.
///
/// Due to the nature of the RTMP chunking protocol, the same serializer should be used
/// for all messages that need to be sent to the same peer.
pub struct ChunkSerializer {
    previous_headers: HashMap<u32, ChunkHeader>,
    max_chunk_size: u32,
}

impl ChunkSerializer {
    pub fn new() -> ChunkSerializer {
        ChunkSerializer {
            max_chunk_size: INITIAL_MAX_CHUNK_SIZE,
            previous_headers: HashMap::new(),
        }
    }

    pub fn set_max_chunk_size(&mut self, new_size: u32, time: RtmpTimestamp) -> Result<Vec<u8>, ChunkSerializationError> {
        let set_chunk_size_message = RtmpMessage::SetChunkSize {size: new_size};
        let message_payload = MessagePayload::from_rtmp_message(set_chunk_size_message, time, 0)?;
        let bytes = self.serialize(&message_payload, true)?;

        self.max_chunk_size = new_size;
        Ok(bytes)
    }

    pub fn serialize(&mut self, message: &MessagePayload, force_uncompressed: bool) -> Result<Vec<u8>, ChunkSerializationError> {
        if message.data.len() > 16777215 {
            return Err(ChunkSerializationError{kind: ChunkSerializationErrorKind::MessageTooLong {size: message.data.len() as u32}});
        }

        let mut bytes = Cursor::new(Vec::new());

        // Since a message may have a payload greater than one chunk allows, we must
        // split the payload into slices that don't exceed the max chunk length
        let mut slices = Vec::<&[u8]>::new();
        let mut iteration = 0;
        loop {
            let start_index = iteration * self.max_chunk_size as usize;
            if start_index >= message.data.len() {
                break;
            }

            let remaining_length = message.data.len() - start_index;
            let end_index = min(start_index + self.max_chunk_size as usize, start_index + remaining_length);

            slices.push(&message.data[start_index..end_index]);

            iteration = iteration + 1;
        }

        for slice in slices.into_iter() {
            self.add_chunk(&mut bytes, force_uncompressed, message, slice)?;
        }

        Ok(bytes.into_inner())
    }

    fn add_chunk(&mut self, bytes: &mut Cursor<Vec<u8>>,  force_uncompressed: bool, message: &MessagePayload, data_to_write: &[u8]) -> Result<(), ChunkSerializationError> {
        let mut header = ChunkHeader {
            chunk_stream_id: get_csid_for_message_type(message.type_id),
            timestamp: message.timestamp,
            timestamp_delta: 0,
            message_type_id: message.type_id,
            message_stream_id: message.message_stream_id,
            message_length: message.data.len() as u32
        };

        let header_format = if force_uncompressed {
            ChunkHeaderFormat::Full
        } else {
            match self.previous_headers.get(&header.chunk_stream_id) {
                None => ChunkHeaderFormat::Full,
                Some(ref previous_header) => {
                    // TODO: Update to support rtmp time wrap-around
                    let time_delta = header.timestamp - previous_header.timestamp;
                    header.timestamp_delta = time_delta.value;

                    get_header_format(&mut header, previous_header)
                },
            }
        };

        add_basic_header(bytes, &header_format, header.chunk_stream_id)?;
        add_initial_timestamp(bytes, &header_format, &header)?;
        add_message_length_and_type_id(bytes, &header_format, header.message_length, header.message_type_id)?;
        add_message_stream_id(bytes, &header_format, header.message_stream_id)?;
        add_extended_timestamp(bytes, &header_format, &header)?;
        add_message_payload(bytes, data_to_write)?;

        self.previous_headers.insert(header.chunk_stream_id, header);
        Ok(())
    }
}

fn add_basic_header(bytes: &mut Write, format: &ChunkHeaderFormat, csid: u32) -> Result<(), ChunkSerializationError> {
    if csid <= 1 || csid >= 65600 {
        panic!("Attempted to serialize an RTMP chunk with a csid of {}, but only csids between 2 and 65600 are allowed", csid);
    }

    let format_mask = match *format {
        ChunkHeaderFormat::Full                            => 0b00000000,
        ChunkHeaderFormat::TimeDeltaWithoutMessageStreamId => 0b01000000,
        ChunkHeaderFormat::TimeDeltaOnly                   => 0b10000000,
        ChunkHeaderFormat::Empty                           => 0b11000000
    };

    let mut first_byte = match csid {
        x if x <= 63             => x as u8,
        x if x >= 64 && x <= 319 => 0,
        _                        => 1
    };

    first_byte = first_byte | format_mask;
    bytes.write_u8(first_byte)?;

    // Since get_csid_for_message_type only does csids up to 6, ignore 2 and 3 byte csid formats
    Ok(())
}

fn add_initial_timestamp(bytes: &mut Cursor<Vec<u8>>, format: &ChunkHeaderFormat, header: &ChunkHeader) -> Result<(), ChunkSerializationError> {
    if *format == ChunkHeaderFormat::Empty {
        return Ok(());
    }

    let value_to_write = match *format {
        ChunkHeaderFormat::Full => header.timestamp.value,
        _ => header.timestamp_delta
    };

    let capped_value = min(value_to_write, MAX_INITIAL_TIMESTAMP);
    bytes.write_u24::<BigEndian>(capped_value)?;

    Ok(())
}

fn add_message_length_and_type_id(bytes: &mut Cursor<Vec<u8>>, format: &ChunkHeaderFormat, length: u32, type_id: u8) -> Result<(), ChunkSerializationError> {
    if *format == ChunkHeaderFormat::Empty || *format == ChunkHeaderFormat::TimeDeltaOnly {
        return Ok(());
    }

    bytes.write_u24::<BigEndian>(length)?;
    bytes.write_u8(type_id)?;
    Ok(())
}

fn add_message_stream_id(bytes: &mut Write, format: &ChunkHeaderFormat, stream_id: u32) -> Result<(), ChunkSerializationError> {
    if *format != ChunkHeaderFormat::Full {
        return Ok(());
    }

    bytes.write_u32::<LittleEndian>(stream_id)?;
    Ok(())
}

fn add_extended_timestamp(bytes: &mut Write, format: &ChunkHeaderFormat, header: &ChunkHeader) -> Result<(), ChunkSerializationError> {
    if *format == ChunkHeaderFormat::Empty {
        return Ok(());
    }

    let timestamp = match *format {
        ChunkHeaderFormat::Full => header.timestamp.value,
        _ => header.timestamp_delta
    };

    if timestamp < MAX_INITIAL_TIMESTAMP {
        return Ok(());
    }

    bytes.write_u32::<BigEndian>(timestamp)?;
    Ok(())
}

fn add_message_payload(bytes: &mut Write, data: &[u8]) -> Result<(), ChunkSerializationError> {
    bytes.write(data)?;
    Ok(())
}

fn get_csid_for_message_type(message_type_id: u8) -> u32 {
    // Naive resolution, purpose (afaik) is to allow repeated messages
    // to utilize header compression by spreading them across chunk streams
    match message_type_id {
        1 | 2 | 3 | 4 | 5 | 6 => 2,
        18 | 19               => 3,
        9                     => 4,
        8                     => 5,
        _                     => 6
    }
}

fn get_header_format(current_header: &mut ChunkHeader, previous_header: &ChunkHeader) -> ChunkHeaderFormat {
    if current_header.message_stream_id != previous_header.message_stream_id {
        return ChunkHeaderFormat::Full;
    }

    if current_header.message_type_id != previous_header.message_type_id || current_header.message_length != previous_header.message_length {
        return ChunkHeaderFormat::TimeDeltaWithoutMessageStreamId;
    }

    if current_header.timestamp_delta != previous_header.timestamp_delta {
        return ChunkHeaderFormat::TimeDeltaOnly;
    }

    ChunkHeaderFormat::Empty
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::time::RtmpTimestamp;
    use std::io::{Cursor, Read};
    use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

    #[test]
    fn type_0_chunk_for_first_message_with_small_timestamp() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let bytes = serializer.serialize(&message1, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b00000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 72, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 4, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 50, "Unexpected type id");
        assert_eq!(cursor.read_u32::<LittleEndian>().unwrap(), 12, "Unexpected message stream id");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &message1.data[..], "Unexpected payload contents");
    }

    #[test]
    fn type_0_chunk_for_first_message_with_extended_timestamp() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(16777216),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let bytes = serializer.serialize(&message1, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b00000000 , "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 16777215, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 4, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 50, "Unexpected type id");
        assert_eq!(cursor.read_u32::<LittleEndian>().unwrap(), 12, "Unexpected message stream id");
        assert_eq!(cursor.read_u32::<BigEndian>().unwrap(), 16777216, "Unexpected extended timestamp");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], [1_u8, 2_u8, 3_u8, 4_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_1_chunk_for_second_message_with_same_stream_id_and_different_message_length_and_different_type_id_and_small_timestamp() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(82),
            type_id: 51,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let bytes = serializer.serialize(&message2, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b01000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 10, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 3, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 51, "Unexpected type id");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 3, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[1_u8, 2_u8, 3_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_1_chunk_for_second_message_with_same_stream_id_and_different_message_length_and_different_type_id_and_extended_timestamp() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(10),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(16777226),
            type_id: 51,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let bytes = serializer.serialize(&message2, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b01000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 16777215, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 3, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 51, "Unexpected type id");
        assert_eq!(cursor.read_u32::<BigEndian>().unwrap(), 16777216, "Unexpected extended timestamp");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 3, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[1_u8, 2_u8, 3_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_2_chunk_for_second_message_with_same_stream_id_and_same_message_length_and_same_type_id_and_small_timestamp() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(82),
            type_id: 50,
            message_stream_id: 12,
            data: vec![5_u8, 6_u8, 7_u8, 8_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let bytes = serializer.serialize(&message2, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b10000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 10, "Unexpected timestamp value");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[5_u8, 6_u8, 7_u8, 8_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_2_chunk_for_second_message_with_same_stream_id_and_same_message_length_and_same_type_id_and_extended_timestamp() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(10),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(16777226),
            type_id: 50,
            message_stream_id: 12,
            data: vec![5_u8, 6_u8, 7_u8, 8_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let bytes = serializer.serialize(&message2, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b10000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 16777215, "Unexpected timestamp value");
        assert_eq!(cursor.read_u32::<BigEndian>().unwrap(), 16777216, "Unexpected extended timestamp");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[5_u8, 6_u8, 7_u8, 8_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_3_chunk_for_third_message_with_all_matching_details() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(82),
            type_id: 50,
            message_stream_id: 12,
            data: vec![5_u8, 6_u8, 7_u8, 8_u8]
        };

        let message3 = MessagePayload {
            timestamp: RtmpTimestamp::new(92),
            type_id: 50,
            message_stream_id: 12,
            data: vec![9_u8, 10_u8, 11_u8, 12_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let _ = serializer.serialize(&message2, false).unwrap();
        let bytes = serializer.serialize(&message3, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b11000000, "Unexpected csid value");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[9_u8, 10_u8, 11_u8, 12_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_0_chunks_used_when_new_message_on_different_csid_serialized() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(82),
            type_id: 1,
            message_stream_id: 12,
            data: vec![6_u8, 7_u8, 8_u8, 9_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let bytes = serializer.serialize(&message2, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 2 | 0b00000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 82, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 4, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 1, "Unexpected type id");
        assert_eq!(cursor.read_u32::<LittleEndian>().unwrap(), 12, "Unexpected message stream id");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[6_u8, 7_u8, 8_u8, 9_u8], "Unexpected payload contents");
    }

    #[test]
    fn type_0_chunk_for_second_message_when_forcing_uncompressed() {
        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: vec![1_u8, 2_u8, 3_u8, 4_u8]
        };

        let message2 = MessagePayload {
            timestamp: RtmpTimestamp::new(82),
            type_id: 50,
            message_stream_id: 12,
            data: vec![5_u8, 6_u8, 7_u8, 8_u8]
        };

        let mut serializer = ChunkSerializer::new();
        let _ = serializer.serialize(&message1, false).unwrap();
        let bytes = serializer.serialize(&message2, true).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b00000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 82, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 4, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 50, "Unexpected type id");
        assert_eq!(cursor.read_u32::<LittleEndian>().unwrap(), 12, "Unexpected message stream id");

        let mut payload_bytes = [0_u8; 50];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 4, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &[5_u8, 6_u8, 7_u8, 8_u8], "Unexpected payload contents");
    }

    #[test]
    fn message_split_when_payload_exceeds_max_chunk_size() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&[11_u8; 75]);
        payload.extend_from_slice(&[22_u8; 25]);

        let message1 = MessagePayload {
            timestamp: RtmpTimestamp::new(72),
            type_id: 50,
            message_stream_id: 12,
            data: payload,
        };

        let mut serializer = ChunkSerializer::new();
        serializer.set_max_chunk_size(75, RtmpTimestamp::new(0)).unwrap();

        let bytes = serializer.serialize(&message1, false).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b00000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 72, "Unexpected timestamp value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 100, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 50, "Unexpected type id");
        assert_eq!(cursor.read_u32::<LittleEndian>().unwrap(), 12, "Unexpected message stream id");

        let mut payload_bytes = [0_u8; 75];
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 75, "Unexpected payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &([11_u8; 75])[..], "Unexpected payload contents");

        assert_eq!(cursor.read_u8().unwrap(), 6 | 0b11000000, "Unexpected 2nd csid value");
        let bytes_read = cursor.read(&mut payload_bytes[..]).unwrap();
        assert_eq!(bytes_read, 25, "Unexpected 2nd payload bytes read");
        assert_eq!(&payload_bytes[..bytes_read], &([22_u8; 25])[..], "Unexpected 2nd payload contents");
    }

    #[test]
    fn changing_size_returns_set_chunk_size_outbound_message() {
        let mut serializer = ChunkSerializer::new();
        let bytes = serializer.set_max_chunk_size(75, RtmpTimestamp::new(152)).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.read_u8().unwrap(), 2 | 0b00000000, "Unexpected csid value");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 152, "Unexpected timestamp");
        assert_eq!(cursor.read_u24::<BigEndian>().unwrap(), 4, "Unexpected message length value");
        assert_eq!(cursor.read_u8().unwrap(), 1, "Unexpected type id");
        assert_eq!(cursor.read_u32::<LittleEndian>().unwrap(), 0, "Unexpected message stream id");
        assert_eq!(cursor.read_u32::<BigEndian>().unwrap(), 75, "Unexpected chunk size");
    }
}
