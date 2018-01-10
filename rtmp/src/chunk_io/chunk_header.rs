use ::time::RtmpTimestamp;

#[derive(PartialEq, Debug)]
pub enum ChunkHeaderFormat {
    Full, // Format 0
    TimeDeltaWithoutMessageStreamId, // Format 1
    TimeDeltaOnly, // Format 2
    Empty // Format 3
}

pub struct ChunkHeader {
    pub chunk_stream_id: u32,
    pub timestamp: RtmpTimestamp,
    pub timestamp_delta: u32,
    pub message_length: u32,
    pub message_type_id: u8,
    pub message_stream_id: u32,
    pub can_be_dropped: bool,
}

impl ChunkHeader {
    pub fn new() -> ChunkHeader {
        ChunkHeader {
            chunk_stream_id: 0,
            timestamp: RtmpTimestamp::new(0),
            timestamp_delta: 0,
            message_length: 0,
            message_type_id: 0,
            message_stream_id: 0,
            can_be_dropped: false,
        }
    }
}