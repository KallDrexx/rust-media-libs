extern crate mio;
extern crate slab;
extern crate rml_rtmp;

mod connection;
mod server;

use mio::*;
use mio::net::{TcpListener};
use slab::Slab;
use ::connection::{Connection, ReadResult, ConnectionError};
use ::server::{Server};

const SERVER: Token = Token(std::usize::MAX - 1);

fn main() {
    let addr = "127.0.0.1:1935".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();
    let mut poll = Poll::new().unwrap();

    println!("Listening for connections");
    poll.register(&listener, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    let mut events = Events::with_capacity(1024);
    let mut connections = Slab::new();
    let mut server = Server::new();

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in events.iter() {
            match event.token() {
                SERVER => {
                    let (socket, _) = listener.accept().unwrap();
                    let mut connection = Connection::new(socket);
                    let token = connections.insert(connection);
                    connections[token].token = Some(Token(token));
                    connections[token].register(&mut poll).unwrap();
                },

                Token(token) => {
                    let mut should_close_connection = false;
                    {
                        let mut connection = match connections.get_mut(token) {
                            Some(connection) => connection,
                            None => continue,
                        };

                        if event.readiness().is_readable() {
                            match connection.readable(&mut poll) {
                                Ok(ReadResult::HandshakingInProgress) => (),
                                Ok(ReadResult::NoBytesReceived) => (),
                                Ok(ReadResult::BytesReceived {buffer, byte_count}) => {
                                    match server.bytes_received(token, &buffer[..byte_count]) {
                                        Ok(mut results) => {
                                            for result in results.drain(..) {
                                                println!("Server result: {:?}", result);
                                            }
                                        },

                                        Err(error) => {
                                            should_close_connection = true;
                                            println!("Input caused the following error: {}", error);
                                        },
                                    }
                                },

                                Err(ConnectionError::SocketClosed) => should_close_connection = true,
                                Err(x) => panic!("Error occurred: {:?}", x),
                            }
                        }

                        if event.readiness().is_writable() {
                            connection.writable(&mut poll).unwrap();
                        }
                    }

                    if should_close_connection {
                        println!("Connection closed");
                        connections.remove(token);
                    }
                }
            }
        }
    }
}
