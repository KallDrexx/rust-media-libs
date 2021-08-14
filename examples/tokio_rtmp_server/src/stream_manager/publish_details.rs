use bytes::Bytes;
use rml_rtmp::sessions::StreamMetadata;

pub struct PublishDetails {
    pub video_sequence_header: Option<Bytes>,
    pub audio_sequence_header: Option<Bytes>,
    pub metadata: Option<StreamMetadata>,
    pub connection_id: i32,
}
