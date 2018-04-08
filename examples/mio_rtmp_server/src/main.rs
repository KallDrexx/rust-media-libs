extern crate bytes;
#[macro_use] extern crate clap;
extern crate mio;
extern crate rml_rtmp;
extern crate slab;

mod connection;
mod server;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::SystemTime;
use clap::App;
use mio::*;
use mio::net::{TcpListener, TcpStream};
use slab::Slab;

use ::connection::{Connection, ReadResult, ConnectionError};
use ::server::{Server, ServerResult};

const SERVER: Token = Token(std::usize::MAX - 1);

type ClosedTokens = HashSet<usize>;
enum EventResult { None, ReadResult(ReadResult), DisconnectConnection }

#[derive(Debug)]
struct PullOptions {
    host: String,
    app: String,
    stream: String,
    target: String,
}

#[derive(Debug)]
struct AppOptions {
    log_io: bool,
    pull: Option<PullOptions>,
}

fn main() {
    let app_options = get_app_options();

    let address = "0.0.0.0:1935".parse().unwrap();
    let listener = TcpListener::bind(&address).unwrap();
    let mut poll = Poll::new().unwrap();

    println!("Listening for connections");
    poll.register(&listener, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    let mut server = Server::new();
    let mut connection_count = 1;
    let mut connections = Slab::new();

    if let Some(ref pull) = app_options.pull {
        println!("Starting pull client for rtmp://{}/{}/{}", pull.host, pull.app, pull.stream);

        let mut pull_host = pull.host.clone();
        if !pull_host.contains(":") {
            pull_host = pull_host + ":1935";
        }

        let addr = SocketAddr::from_str(&pull_host).unwrap();
        let stream = TcpStream::connect(&addr).unwrap();
        let mut connection = Connection::new(stream, connection_count, app_options.log_io, false);
        let token = connections.insert(connection);
        connection_count += 1;

        println!("Pull client started with connection id {}", token);
        connections[token].token = Some(Token(token));
        connections[token].register(&mut poll).unwrap();
        server.register_pull_client(token, pull.app.clone(), pull.stream.clone(), pull.target.clone());
    }

    let mut events = Events::with_capacity(1024);
    let mut outer_started_at = SystemTime::now();
    let mut inner_started_at;
    let mut total_ns = 0;
    let mut poll_count = 0_u32;

    loop {
        poll.poll(&mut events, None).unwrap();

        inner_started_at = SystemTime::now();
        poll_count += 1;

        for event in events.iter() {
            let mut connections_to_close = ClosedTokens::new();

            match event.token() {
                SERVER => {
                    let (socket, _) = listener.accept().unwrap();
                    let mut connection = Connection::new(socket, connection_count, app_options.log_io, true);
                    let token = connections.insert(connection);

                    connection_count += 1;

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

                                ReadResult::HandshakeCompleted {buffer, byte_count} => {
                                    // Server will understand that the first call to
                                    // handle_read_bytes signifies that handshaking is completed
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
        total_ns += inner_elapsed.subsec_nanos();

        if outer_elapsed.as_secs() >= 10 {
            let seconds_since_start = outer_started_at.elapsed().unwrap().as_secs();
            let seconds_doing_work = (total_ns as f64) / (1000 as f64) / (1000 as f64) / (1000 as f64);
            let percentage_doing_work = (seconds_doing_work / seconds_since_start as f64) * 100 as f64;
            println!("Spent {} ms ({}% of time) doing work over {} seconds (avg {} microseconds per iteration)",
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

fn get_app_options() -> AppOptions {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let log_io = matches.is_present("log-io");
    let pull_options = match matches.subcommand_matches("pull") {
        None => None,
        Some(pull_matches) => {
            Some(PullOptions {
                host: pull_matches.value_of("host").unwrap().to_string(),
                app: pull_matches.value_of("app").unwrap().to_string(),
                stream: pull_matches.value_of("stream").unwrap().to_string(),
                target: pull_matches.value_of("target").unwrap().to_string(),
            })
        }
    };

    let app_options = AppOptions {
        pull: pull_options,
        log_io,
    };

    println!("Application options: {:?}", app_options);
    app_options
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
