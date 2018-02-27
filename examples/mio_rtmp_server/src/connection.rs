use std::io;
use std::io::{Read, Write};
use std::collections::VecDeque;
use mio::{Token, Ready, Poll, PollOpt};
use mio::net::TcpStream;
use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};
use rml_rtmp::sessions::{ServerSession, ServerSessionResult, ServerSessionConfig};

#[derive(PartialEq, Eq)]
pub enum ConnectionState {
    Active,
    Closed
}

enum ByteHandler {
    Handshake,
    RtmpSession,
}

pub struct Connection {
    socket: TcpStream,
    pub token: Option<Token>,
    interest: Ready,
    send_queue: VecDeque<Vec<u8>>,
    has_been_registered: bool,
    byte_handler: ByteHandler,
    handshake: Handshake,
    server_session: Option<ServerSession>,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Connection {
        Connection {
            socket,
            token: None,
            interest: Ready::readable() | Ready::writable(),
            send_queue: VecDeque::new(),
            has_been_registered: false,
            byte_handler: ByteHandler::Handshake,
            handshake: Handshake::new(PeerType::Server),
            server_session: None,
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
                    let state = match self.byte_handler {
                        ByteHandler::Handshake => self.handle_handshake_bytes(poll, &buffer[..bytes_read])?,
                        ByteHandler::RtmpSession => self.handle_rtmp_bytes(poll, &buffer[..bytes_read])?,
                    };

                    self.register(poll)?;
                    return Ok(state);
                },

                Err(error) => {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        // There's no data available in the receive buffer, stop trying until the
                        // next readable event.
                        self.register(poll)?;
                        return Ok(ConnectionState::Active);
                    } else {
                        println!("Failed to send buffer for {:?} with error {}", self.token, error);
                        return Err(error);
                    }
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

    fn handle_handshake_bytes(&mut self, poll: &mut Poll, bytes: &[u8]) -> io::Result<ConnectionState> {
        let result = match self.handshake.process_bytes(bytes) {
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

                Ok(ConnectionState::Active)
            },

            HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
                println!("Handshake completed!");
                if response_bytes.len() > 0 {
                    self.enqueue_response(poll, response_bytes)?;
                }

                let (session, mut results) = match ServerSession::new(ServerSessionConfig::new()) {
                    Ok(x) => x,
                    Err(error) => {
                        println!("Error creating new server session: {:?}", error);
                        return Ok(ConnectionState::Closed);
                    }
                };

                self.server_session = Some(session);

                for result in results.drain(..) {
                    match result {
                        ServerSessionResult::OutboundResponse(packet) => self.enqueue_response(poll, packet.bytes)?,
                        x => println!("Session result: {:?}", x),
                    }
                }

                self.byte_handler = ByteHandler::RtmpSession;
                self.handle_rtmp_bytes(poll, &remaining_bytes[..])
            }
        }
    }

    fn handle_rtmp_bytes(&mut self, poll: &mut Poll, bytes: &[u8]) -> io::Result<ConnectionState> {
        let mut results = match self.server_session.as_mut().unwrap().handle_input(bytes) {
            Ok(results) => results,
            Err(error) => {
                println!("Server session error: {:?}", error);
                return Ok(ConnectionState::Closed);
            }
        };

        for result in results.drain(..) {
            match result {
                ServerSessionResult::OutboundResponse(packet) => self.enqueue_response(poll, packet.bytes)?,
                x => println!("Session result: {:?}", x),
            }
        }

        return Ok(ConnectionState::Active);
    }
}