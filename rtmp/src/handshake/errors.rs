use std::io;

#[derive(Debug, Fail)]
pub enum HandshakeError {
    #[fail(display = "First byte of the handshake did not start with a 3")]
    BadVersionId,

    #[fail(display = "Packet 1's 2nd time field was expected to be empty, but wasn't")]
    NonZeroedTimeInPacket1,

    #[fail(display = "Peer did not send the correct time back")]
    IncorrectPeerTime,

    #[fail(display = "Peer did not send the correct random data back")]
    IncorrectRandomData,

    #[fail(display = "_0")]
    Io(#[cause] io::Error)
}

impl From<io::Error> for HandshakeError {
    fn from(error: io::Error) -> Self {
        HandshakeError::Io(error)
    }
}