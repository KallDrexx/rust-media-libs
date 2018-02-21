use std::io;
use std::io::{Read, Write};
use std::collections::VecDeque;
use mio::{Token, Ready, Poll, PollOpt};
use mio::net::TcpStream;

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
    read_buffer: Vec<u8>,
    has_been_registered: bool,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Connection {
        Connection {
            socket,
            token: None,
            interest: Ready::readable() | Ready::writable(),
            send_queue: VecDeque::new(),
            read_buffer: Vec::new(),
            has_been_registered: false,
        }
    }

    pub fn write_line(&mut self, poll: &mut Poll, line: Vec<u8>) -> io::Result<()> {
        self.send_queue.push_back(line);
        self.interest.insert(Ready::writable());
        self.register(poll)?;
        Ok(())
    }

    pub fn readable(&mut self, poll: &mut Poll) -> io::Result<ConnectionState> {
        println!("Socket readable");

        let mut buffer = [0_u8; 1024];
        loop {
            match self.socket.read(&mut buffer) {
                Ok(0) => {
                    return Ok(ConnectionState::Closed);
                },

                Ok(bytes_read) => {
                    println!("{} bytes read", bytes_read);

                    for x in 0..bytes_read {
                        self.read_buffer.push(buffer[x]);

                        if buffer[x] == '\n' as u8 {
                            // Send the line back to the client
                            let line = self.read_buffer.drain(..).collect();
                            self.write_line(poll, line)?;
                        }
                    }
                },

                Err(error) => {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        println!("wouldblock");
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
        println!("Socket writable");

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
        println!("Registering with interest: {:?}", self.interest);

        match self.has_been_registered {
            true => poll.reregister(&self.socket, self.token.unwrap(), self.interest, PollOpt::edge() | PollOpt::oneshot())?,
            false => poll.register(&self.socket, self.token.unwrap(), self.interest, PollOpt::edge() | PollOpt::oneshot())?
        }

        self.has_been_registered = true;
        Ok(())
    }
}