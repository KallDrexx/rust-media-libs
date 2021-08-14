use super::ConnectionMessage;
use bytes::Bytes;
use rml_rtmp::sessions::StreamMetadata;
use rml_rtmp::time::RtmpTimestamp;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum StreamManagerMessage {
    NewConnection {
        connection_id: i32,
        sender: mpsc::UnboundedSender<ConnectionMessage>,
        disconnection: mpsc::UnboundedReceiver<()>,
    },

    PublishRequest {
        connection_id: i32,
        rtmp_app: String,
        stream_key: String,
        request_id: u32,
    },

    PlaybackRequest {
        connection_id: i32,
        rtmp_app: String,
        stream_key: String,
        request_id: u32,
    },

    UpdatedStreamMetadata {
        sending_connection_id: i32,
        metadata: StreamMetadata,
    },

    NewVideoData {
        sending_connection_id: i32,
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    NewAudioData {
        sending_connection_id: i32,
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    PublishFinished {
        connection_id: i32,
    },

    PlaybackFinished {
        connection_id: i32,
    },
}
