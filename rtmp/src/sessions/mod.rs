/// This module contains implemented session abstractions.
///
/// A session is an abstraction that reacts to incoming RTMP messages (encoded as RTMP chunks)
/// with packets to be sent as a response, as well as raising events that applications can
/// perform custom logic on.

mod server;
mod client;

pub use self::client::ClientSession;
pub use self::client::ClientSessionEvent;
pub use self::client::ClientSessionConfig;
pub use self::client::ClientSessionError;
pub use self::client::ClientSessionErrorKind;
pub use self::client::ClientSessionResult;

pub use self::server::ServerSession;
pub use self::server::ServerSessionEvent;
pub use self::server::ServerSessionConfig;
pub use self::server::ServerSessionError;
pub use self::server::ServerSessionErrorKind;
pub use self::server::ServerSessionResult;

/// Contains the metadata information a stream may advertise on publishing
#[derive(PartialEq, Debug, Clone)]
pub struct StreamMetadata {
    video_width: Option<u32>,
    video_height: Option<u32>,
    video_codec: Option<String>,
    video_frame_rate: Option<f32>,
    video_bitrate_kbps: Option<u32>,
    audio_codec: Option<String>,
    audio_bitrate_kbps: Option<u32>,
    audio_sample_rate: Option<u32>,
    audio_channels: Option<u32>,
    audio_is_stereo: Option<bool>,
    encoder: Option<String>
}

impl StreamMetadata {
    fn new() -> StreamMetadata {
        StreamMetadata {
            video_width: None,
            video_height: None,
            video_codec: None,
            video_frame_rate: None,
            video_bitrate_kbps: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            audio_sample_rate: None,
            audio_channels: None,
            audio_is_stereo: None,
            encoder: None
        }
    }
}