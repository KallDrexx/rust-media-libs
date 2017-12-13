pub mod errors;
pub mod original;

use self::errors::HandshakeError;

#[derive(PartialEq, Eq, Debug)]
pub enum HandshakeProcessResult {
    /// The handshake process is still on-going
    InProgress {
        /// Any bytes that should be sent to the peer as a response
        response_bytes: Vec<u8>,
    },

    /// The handshake process has successfully concluded
    Completed {
        /// Any bytes left over after completing the handshake
        remaining_bytes: Vec<u8>,
    }
}

/// The `HandshakeHandler` trait allows for processing the handshake between
/// two RTMP endpoints attempting to connect to each other.
pub trait HandshakeHandler {
    /// Creates the packets 0 and 1 that should get sent to the peer.  This is only strictly
    /// required to be called by the client in the connection process to initiate the handshake
    /// process.  The server can wait until `process_bytes()` is called, and the outbound
    /// packets #0 and #1 will be included as the handshake's response.
    fn generate_outbound_p0_and_p1(&mut self) -> Result<HandshakeProcessResult, HandshakeError>;

    /// Processes the passed in bytes as part of the handshake process.  If not enough bytes
    /// were received to complete the next handshake step then the handshake handler will
    /// hold onto the currently received bytes until it has enough to either error out or
    /// move onto the next stage of the handshake.
    ///
    /// If the handshake is still in progress it will potentially return bytes that should be
    /// sent to the peer.  If the handshake has completed it will return any overflow bytes it
    /// received that were not part of the handshaking process.
    ///
    /// If the `HandshakeHandler` has not generated the outbound packets 0 and 1 yet, then
    /// the first call to `process_bytes` will include packets 0 and 1 in the `response_bytes`
    /// field.
    fn process_bytes(&mut self, data: &[u8]) -> Result<HandshakeProcessResult, HandshakeError>;
}