/// The configuration options that govern how a RTMP server session should operate
#[derive(Clone)]
pub struct ServerSessionConfig {
    pub fms_version: String,
    pub chunk_size: u32,
    pub peer_bandwidth: u32,
    pub window_ack_size: u32,
}

impl ServerSessionConfig {
    /// Creates a new server session config with overridable defaults
    pub fn new() -> ServerSessionConfig {
        ServerSessionConfig {
            fms_version: "FMS/3,0,1,1233".to_string(),
            peer_bandwidth: 2_500_000,
            window_ack_size: 1_073_741_824,
            chunk_size: 4096,
        }
    }
}
