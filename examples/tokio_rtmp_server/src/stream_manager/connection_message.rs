use bytes::Bytes;
use rml_rtmp::sessions::StreamMetadata;
use rml_rtmp::time::RtmpTimestamp;

#[derive(Debug)]
pub enum ConnectionMessage {
    RequestAccepted {
        request_id: u32,
    },

    RequestDenied {
        request_id: u32,
    },

    NewVideoData {
        timestamp: RtmpTimestamp,
        data: Bytes,
        can_be_dropped: bool,
    },

    NewAudioData {
        timestamp: RtmpTimestamp,
        data: Bytes,
        can_be_dropped: bool,
    },

    NewMetadata {
        metadata: StreamMetadata,
    },
}
