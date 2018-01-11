/// This module contains implemented session abstractions.
///
/// A session is an abstraction that reacts to incoming RTMP messages (encoded as RTMP chunks)
/// with packets to be sent as a response, as well as raising events that applications can
/// perform custom logic on.

mod server;

pub use self::server::ServerSessionEvents;

/// Contains the metadata information a stream may advertise on publishing
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