use std::io;
use std::io::{Read, Write};
use std::collections::VecDeque;
use mio::{Token, Ready, Poll, PollOpt};
use mio::net::TcpStream;
use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};

#[derive(PartialEq, Eq)]
pub enum ConnectionState {
    Active,
    Closed
}

pub struct Connection {
    socket: TcpStream,
    pub token: Option<Token>,
    interest: Ready,
    send_queue: VecDeque<Vec<u8>>,
    has_been_registered: bool,
    handshake: Handshake,
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
        }
    }

    pub fn enqueue_response(&mut self, poll: &mut Poll, line: Vec<u8>) -> io::Result<()> {
        self.send_queue.push_back(line);
        self.interest.insert(Ready::writable());
        self.register(poll)?;
        Ok(())
    }

    pub fn readable(&mut self, poll: &mut Poll) -> io::Result<ConnectionState> {
        let mut buffer = [0_u8; 4096];
        loop {
            match self.socket.read(&mut buffer) {
                Ok(0) => {
                    return Ok(ConnectionState::Closed);
                },

                Ok(bytes_read) => {
                    let result = match self.handshake.process_bytes(&buffer[..bytes_read]) {
                        Ok(result) => result,
                        Err(error) => {
                            println!("Handshake error: {:?}", error);
                            return Ok(ConnectionState::Closed);
                        }
                    };

                    match result {
                        HandshakeProcessResult::InProgress {response_bytes} => {
                            println!("Handshake in progress");
                            if response_bytes.len() > 0 {
                                self.enqueue_response(poll, response_bytes)?;
                            }
                        },

                        HandshakeProcessResult::Completed {response_bytes: _, remaining_bytes: _} => {
                            println!("Handshake completed!");
                            return Ok(ConnectionState::Closed);
                        }
                    }
                },

                Err(error) => {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        // There's no data available in the receive buffer, stop trying until the
                        // next readable event.
                        break;
                    } else {
                        println!("Failed to send buffer for {:?} with error {}", self.token, error);
                        return Err(error);
                    }
                }
            }
        }

        self.register(poll)?;
        Ok(ConnectionState::Active)
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
            Ok(bytes_sent) => {
                println!("Sent message length of {} bytes", bytes_sent);
            },

            Err(error) => {
                if error.kind() == io::ErrorKind::WouldBlock {
                    // Client buffer is full, push it back to the queue
                    println!("test");
                    self.send_queue.push_front(message);
                } else {
                    println!("Failed to send buffer for {:?} with error {}", self.token, error);
                    return Err(error);
                }
            }
        };

        if self.send_queue.is_empty() {
            println!("Outbound queue is empty");
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
}