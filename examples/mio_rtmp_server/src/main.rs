extern crate mio;
extern crate slab;
extern crate rml_rtmp;

mod connection;
mod server;

use std::collections::HashSet;
use std::time::SystemTime;
use mio::*;
use mio::net::{TcpListener};
use slab::Slab;
use ::connection::{Connection, ReadResult, ConnectionError};
use ::server::{Server, ServerResult};

const SERVER: Token = Token(std::usize::MAX - 1);

type ClosedTokens = HashSet<usize>;
enum EventResult { None, ReadResult(ReadResult), DisconnectConnection }

fn main() {
    let addr = "0.0.0.0:1935".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();
    let mut poll = Poll::new().unwrap();

    println!("Listening for connections");
    poll.register(&listener, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    let mut events = Events::with_capacity(1024);
    let mut connections = Slab::new();
    let mut server = Server::new();
    let mut count = 1;
    let mut outer_started_at = SystemTime::now();
    let mut inner_started_at;
    let mut total_ns = 0_u64;
    let mut poll_count = 0;

    loop {
        poll.poll(&mut events, None).unwrap();

        inner_started_at = SystemTime::now();
        poll_count += 1;

        for event in events.iter() {
            let mut connections_to_close = ClosedTokens::new();

            match event.token() {
                SERVER => {
                    let (socket, _) = listener.accept().unwrap();
                    let mut connection = Connection::new(socket, count, false);
                    let token = connections.insert(connection);

                    count += 1;

                    println!("New connection (id {})", token);

                    connections[token].token = Some(Token(token));
                    connections[token].register(&mut poll).unwrap();
                },

                Token(token) => {
                    match process_event(&event.readiness(), &mut connections, token, &mut poll) {
                        EventResult::None => (),
                        EventResult::ReadResult(result) => {
                            match result {
                                ReadResult::HandshakingInProgress => (),
                                ReadResult::NoBytesReceived => (),
                                ReadResult::BytesReceived {buffer, byte_count} => {
                                    connections_to_close = handle_read_bytes(&buffer[..byte_count],
                                        token,
                                        &mut server,
                                        &mut connections,
                                        &mut poll);
                                },
                            }
                        },

                        EventResult::DisconnectConnection => {
                            connections_to_close.insert(token);
                        },
                    }
                }
            }

            for token in connections_to_close {
                println!("Closing connection id {}", token);
                connections.remove(token);
                server.notify_connection_closed(token);
            }
        }

        let inner_elapsed = inner_started_at.elapsed().unwrap();
        let outer_elapsed = outer_started_at.elapsed().unwrap();
        total_ns += inner_elapsed.subsec_nanos() as u64;

        if outer_elapsed.as_secs() >= 10 {
            let seconds_since_start = outer_started_at.elapsed().unwrap().as_secs();
            let seconds_doing_work = (total_ns as f64) / (1000 as f64) / (1000 as f64) / (1000 as f64);
            let percentage_doing_work = (seconds_doing_work / seconds_since_start as f64) * 100 as f64;
            println!("Spent {} ms ({}% of time) doing work over {} seconds (avg {} microseconds per iteration) ",
                     total_ns / 1000 / 1000,
                     percentage_doing_work as u32,
                     seconds_since_start,
                     (total_ns / poll_count) / 1000);

            // Reset so each notification is per that interval
            total_ns = 0;
            poll_count = 0;
            outer_started_at = SystemTime::now();
        }
    }
}

fn process_event(event: &Ready, connections: &mut Slab<Connection>, token: usize, poll: &mut Poll) -> EventResult {
    let connection = match connections.get_mut(token) {
        Some(connection) => connection,
        None => return EventResult::None,
    };

    if event.is_writable() {
        match connection.writable(poll) {
            Ok(_) => (),
            Err(error) => {
                println!("Error occurred while writing: {:?}", error);
                return EventResult::DisconnectConnection
            },
        }
    }

    if event.is_readable() {
        match connection.readable(poll) {
            Ok(result) => return EventResult::ReadResult(result),
            Err(ConnectionError::SocketClosed) => return EventResult::DisconnectConnection,
            Err(x) => {
                println!("Error occurred: {:?}", x);
                return EventResult::DisconnectConnection;
            },
        }
    }

    EventResult::None
}

fn handle_read_bytes(bytes: &[u8],
                     from_token: usize,
                     server: &mut Server,
                     connections: &mut Slab<Connection>,
                     poll: &mut Poll) -> ClosedTokens {
    let mut closed_tokens = ClosedTokens::new();

    let mut server_results = match server.bytes_received(from_token, bytes) {
        Ok(results) => results,
        Err(error) => {
            println!("Input caused the following server error: {}", error);
            closed_tokens.insert(from_token);
            return closed_tokens;
        }
    };

    for result in server_results.drain(..) {
        match result {
            ServerResult::OutboundPacket {target_connection_id, packet} => {
                match connections.get_mut(target_connection_id) {
                    Some(connection) => connection.enqueue_packet(poll, packet).unwrap(),
                    None => (),
                }
            },

            ServerResult::DisconnectConnection {connection_id} => {
                closed_tokens.insert(connection_id);
            }
        }
    }

    closed_tokens
}
