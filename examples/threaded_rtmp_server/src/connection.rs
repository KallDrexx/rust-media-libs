use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use std::collections::VecDeque;
use std::io;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

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
    pub connection_id: Option<usize>,
    writer: Sender<Vec<u8>>,
    reader: Receiver<ReadResult>,
    handshake: Handshake,
    handshake_completed: bool,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Connection {
        let (byte_sender, byte_receiver) = channel();
        let (result_sender, result_receiver) = channel();

        start_byte_writer(byte_receiver, &socket);
        start_result_reader(result_sender, &socket);

        Connection {
            connection_id: None,
            writer: byte_sender,
            reader: result_receiver,
            handshake: Handshake::new(PeerType::Server),
            handshake_completed: false,
        }
    }

    pub fn write(&self, bytes: Vec<u8>) {
        self.writer.send(bytes).unwrap();
    }

    pub fn read(&mut self) -> Result<ReadResult, ConnectionError> {
        match self.reader.try_recv() {
            Err(TryRecvError::Empty) => Ok(ReadResult::NoBytesReceived),
            Err(TryRecvError::Disconnected) => Err(ConnectionError::SocketClosed),
            Ok(result) => match self.handshake_completed {
                true => Ok(result),
                false => match result {
                    ReadResult::HandshakingInProgress => unreachable!(),
                    ReadResult::NoBytesReceived => Ok(result),
                    ReadResult::BytesReceived { buffer, byte_count } => {
                        self.handle_handshake_bytes(&buffer[..byte_count])
                    }
                },
            },
        }
    }

    fn handle_handshake_bytes(&mut self, bytes: &[u8]) -> Result<ReadResult, ConnectionError> {
        let result = match self.handshake.process_bytes(bytes) {
            Ok(result) => result,
            Err(error) => {
                println!("Handshake error: {:?}", error);
                return Err(ConnectionError::SocketClosed);
            }
        };

        match result {
            HandshakeProcessResult::InProgress { response_bytes } => {
                if response_bytes.len() > 0 {
                    self.write(response_bytes);
                }

                Ok(ReadResult::HandshakingInProgress)
            }

            HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            } => {
                println!("Handshake successful!");
                if response_bytes.len() > 0 {
                    self.write(response_bytes);
                }

                let mut buffer = [0; BUFFER_SIZE];
                let buffer_size = remaining_bytes.len();
                for (index, value) in remaining_bytes.into_iter().enumerate() {
                    buffer[index] = value;
                }

                self.handshake_completed = true;
                Ok(ReadResult::BytesReceived {
                    buffer,
                    byte_count: buffer_size,
                })
            }
        }
    }
}

fn start_byte_writer(byte_receiver: Receiver<Vec<u8>>, socket: &TcpStream) {
    let mut socket = socket.try_clone().expect("failed to clone socket");
    thread::spawn(move || {
        let mut send_queue = VecDeque::new();

        loop {
            loop {
                match byte_receiver.try_recv() {
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                    Ok(bytes) => send_queue.push_back(bytes),
                }
            }

            match send_queue.pop_front() {
                None => thread::sleep(Duration::from_millis(1)),
                Some(bytes) => match socket.write(&bytes) {
                    Ok(_) => (),
                    Err(error) => {
                        println!("Error writing to socket: {:?}", error);
                        return;
                    }
                },
            }
        }
    });
}

fn start_result_reader(sender: Sender<ReadResult>, socket: &TcpStream) {
    let mut socket = socket.try_clone().unwrap();
    thread::spawn(move || {
        let mut buffer = [0; BUFFER_SIZE];
        loop {
            match socket.read(&mut buffer) {
                Ok(0) => return, // socket closed
                Ok(read_count) => {
                    let mut send_buffer = [0; BUFFER_SIZE];
                    for x in 0..read_count {
                        send_buffer[x] = buffer[x];
                    }

                    let result = ReadResult::BytesReceived {
                        buffer: send_buffer,
                        byte_count: read_count,
                    };

                    sender.send(result).unwrap();
                }

                Err(error) => {
                    println!("Error occurred reading from socket: {:?}", error);
                    return;
                }
            }
        }
    });
}
