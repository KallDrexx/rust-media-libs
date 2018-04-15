//! Errors that can occur during the handshaking process

use std::io;
use std::fmt;
use failure::{Backtrace, Fail};

/// Data pertaining to errors that occurred during the handshaking process.
#[derive(Debug)]
pub struct HandshakeError {
    /// The type of error that was observed
    pub kind: HandshakeErrorKind,
}

/// Enumeration that represents the various errors that can occur during the handshaking process
#[derive(Debug, Fail)]
pub enum HandshakeErrorKind {
    /// The RTMP specification requires the first byte in the handshake process to start with a
    /// 3, so this error is encountered if any other value is in the first byte.
    #[fail(display = "First byte of the handshake did not start with a 3")]
    BadVersionId,

    /// The RTMP specification requires the 2nd set of 4 bytes to all be zeroes, so this error
    /// is encountered if any of those values are not zeros.
    #[fail(display = "Packet 1's 2nd time field was expected to be empty, but wasn't")]
    NonZeroedTimeInPacket1,

    /// This is encountered when the peer did not send the same timestamp in packet #2 that we
    /// sent them in our packet #1.
    #[fail(display = "Peer did not send the correct time back")]
    IncorrectPeerTime,

    /// This is encountered when the peer did not send back the same random data in their packet
    /// number 2 that we sent them in our packet number 1.
    #[fail(display = "Peer did not send the correct random data back")]
    IncorrectRandomData,

    /// This is encountered if we try to keep progressing on a handshake handler that has already
    /// completed a successful handshake.
    #[fail(display = "Attempted to continue handshake process after completing handshake")]
    HandshakeAlreadyCompleted,

    /// The packet 1 we receive may be one of two formats (digest at position 8 or 772).  There
    /// is no known way to know which one to expect, so both are tested for.  This error is returned
    /// when packet 1 does not match either of the messages (and most likely is a bad handshake).
    #[fail(display = "No known message format could be determined from the received packet 1")]
    UnknownPacket1Format,

    /// This occurs when the incoming p2 did not either contain an exact copy of the p1 we sent
    /// (old handshake) or the hmac signature did not match (digest handshake).
    #[fail(display = "Invalid handshake packet 2 received")]
    InvalidP2Packet,

    /// This occurs when an IO error is encountered while reading the input.
    #[fail(display = "_0")]
    Io(#[cause] io::Error)
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl Fail for HandshakeError {
    fn cause(&self) -> Option<&Fail> {
        self.kind.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.kind.backtrace()
    }
}

impl From<HandshakeErrorKind> for HandshakeError {
    fn from(kind: HandshakeErrorKind) -> Self {
        HandshakeError { kind }
    }
}

impl From<io::Error> for HandshakeError {
    fn from(error: io::Error) -> Self {
        HandshakeError { kind: HandshakeErrorKind::Io(error) }
    }
}