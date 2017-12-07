use std::io;

quick_error! {
    #[derive(Debug)]
    pub enum HandshakeError {
        BadVersionId {
            description("First byte of the handshake did not start with a 3")
        }

        NonZeroedTimeInPacket1 {
            description("Packet 1's 2nd time field was expected to be empty, but wasn't")
        }

        IncorrectPeerTime {
            description("Peer did not send the correct time back")
        }

        IncorrectRandomData {
            description("Peer did not send the correct random data back")
        }

        Io(err: io::Error) {
            cause(err)
            description(err.description())
            from()
        }
    }
}