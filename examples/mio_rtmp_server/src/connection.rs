use std::io;
use std::io::{Read, Write};
use std::collections::VecDeque;
use mio::{Token, Ready, Poll, PollOpt};
use mio::net::TcpStream;
use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};

const BUFFER_SIZE: usize = 4096;

pub enum ReadResult {
    HandshakingInProgress,
    NoBytesReceived,
    BytesReceived {
        buffer: [u8; BUFFER_SIZE],
        byte_count: usize,
    },
}

#[derive(Debug)]
pub enum ConnectionError {
    IoError(io::Error),
    SocketClosed,
}

impl From<io::Error> for ConnectionError {
    fn from(error: io::Error) -> Self {
        ConnectionError::IoError(error)
    }
}

pub struct Connection {
    socket: TcpStream,
    pub token: Option<Token>,
    interest: Ready,
    send_queue: VecDeque<Vec<u8>>,
    has_been_registered: bool,
    handshake: Handshake,
    handshake_completed: bool,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Connection {
        Connection {
            socket,
            token: None,
            interest: Ready::readable() | Ready::writable(),
            send_queue: VecDeque::new(),
            has_been_registered: false,
            handshake: Handshake::new(PeerType::Server),
            handshake_completed: false,
        }
    }

    pub fn enqueue_response(&mut self, poll: &mut Poll, bytes: Vec<u8>) -> io::Result<()> {
        self.send_queue.push_back(bytes);
        self.interest.insert(Ready::writable());
        self.register(poll)?;
        Ok(())
    }

    pub fn readable(&mut self, poll: &mut Poll) -> Result<ReadResult, ConnectionError> {
        let mut buffer = [0_u8; 4096];
        match self.socket.read(&mut buffer) {
            Ok(0) => {
                Err(ConnectionError::SocketClosed)
            },

            Ok(bytes_read_count) => {
                let read_bytes = match self.handshake_completed {
                    false => self.handle_handshake_bytes(poll, &buffer[..bytes_read_count])?,
                    true => ReadResult::BytesReceived {buffer, byte_count: bytes_read_count},
                };

                self.register(poll)?;
                Ok(read_bytes)
            },

            Err(error) => {
                if error.kind() == io::ErrorKind::WouldBlock {
                    // There's no data available in the receive buffer, stop trying until the
                    // next readable event.
                    self.register(poll)?;
                    Ok(ReadResult::NoBytesReceived)
                } else {
                    println!("Failed to send buffer for {:?} with error {}", self.token, error);
                    return Err(ConnectionError::IoError(error));
                }
            }
        }
    }

    pub fn writable(&mut self, poll: &mut Poll) -> io::Result<()> {
        let message = match self.send_queue.pop_front() {
            Some(x) => x,
            None => {
                // Queue was empty, so we are no longer interested in writable events
                self.interest.remove(Ready::writable());
                self.register(poll)?;
                return Ok(());
            }
        };

        match self.socket.write(&message) {
            Ok(_bytes_sent) => {
            },

            Err(error) => {
                if error.kind() == io::ErrorKind::WouldBlock {
                    // Client buffer is full, push it back to the queue
                    println!("Full write buffer!");
                    self.send_queue.push_front(message);
                } else {
                    println!("Failed to send buffer for {:?} with error {}", self.token, error);
                    return Err(error);
                }
            }
        };

        if self.send_queue.is_empty() {
            self.interest.remove(Ready::writable());
        }

        self.register(poll)?;
        Ok(())
    }

    pub fn register(&mut self, poll: &mut Poll) -> io::Result<()> {
        match self.has_been_registered {
            true => poll.reregister(&self.socket, self.token.unwrap(), self.interest, PollOpt::edge() | PollOpt::oneshot())?,
            false => poll.register(&self.socket, self.token.unwrap(), self.interest, PollOpt::edge() | PollOpt::oneshot())?
        }

        self.has_been_registered = true;
        Ok(())
    }

    fn handle_handshake_bytes(&mut self, poll: &mut Poll, bytes: &[u8]) -> Result<ReadResult, ConnectionError> {
        let result = match self.handshake.process_bytes(bytes) {
            Ok(result) => result,
            Err(error) => {
                println!("Handshake error: {:?}", error);
                return Err(ConnectionError::SocketClosed);
            }
        };

        match result {
            HandshakeProcessResult::InProgress {response_bytes} => {
                println!("Handshake in progress");
                if response_bytes.len() > 0 {
                    self.enqueue_response(poll, response_bytes)?;
                }

                Ok(ReadResult::HandshakingInProgress)
            },

            HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
                println!("Handshake completed!");
                if response_bytes.len() > 0 {
                    self.enqueue_response(poll, response_bytes)?;
                }

                let mut buffer = [0; BUFFER_SIZE];
                let buffer_size = remaining_bytes.len();
                for (index, value) in remaining_bytes.into_iter().enumerate() {
                    buffer[index] = value;
                }

                self.handshake_completed = true;
                Ok(ReadResult::BytesReceived {buffer, byte_count: buffer_size})
            }
        }
    }
}