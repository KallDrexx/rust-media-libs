extern crate mio;
extern crate slab;

mod connection;

use mio::*;
use mio::net::{TcpListener};
use slab::Slab;
use ::connection::Connection;

const SERVER: Token = Token(std::usize::MAX - 1);

fn main() {
    let addr = "127.0.0.1:1935".parse().unwrap();
    let server = TcpListener::bind(&addr).unwrap();
    let mut poll = Poll::new().unwrap();

    println!("Listening for connections");
    poll.register(&server, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    let mut events = Events::with_capacity(1024);
    let mut connections = Slab::new();

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in events.iter() {
            println!("ready {:?} {:?}", event.token(), event.readiness());
            println!("token: {:?}", event.token());

            match event.token() {
                SERVER => {
                    let (socket, _) = server.accept().unwrap();
                    let mut connection = Connection::new(socket);
                    let token = connections.insert(connection);
                    connections[token].token = Some(Token(token));
                    connections[token].register(&mut poll).unwrap();
                },

                Token(value) => {
                    let mut connection = match connections.get_mut(value) {
                        Some(connection) => connection,
                        None => continue,
                    };

                    if event.readiness().is_readable() {
                        connection.readable(&mut poll).unwrap();
                    }

                    if event.readiness().is_writable() {
                        connection.writable(&mut poll).unwrap();
                    }
                }
            }
        }
    }
}
