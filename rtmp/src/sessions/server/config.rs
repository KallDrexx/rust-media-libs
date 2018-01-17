
/// The configuration options that govern how a RTMP server session should operate
#[derive(Clone)]
pub struct ServerSessionConfig {
    pub fms_version: String,
    pub chunk_size: u32,
    pub peer_bandwidth: u32,
    pub window_ack_size: u32,
}

