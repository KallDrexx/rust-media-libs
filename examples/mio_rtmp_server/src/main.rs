extern crate mio;

use mio::*;
use mio::net::{TcpListener};

const SERVER: Token = Token(0);

fn main() {
    let addr = "127.0.0.1:1935".parse().unwrap();
    let server = TcpListener::bind(&addr).unwrap();
    let poll = Poll::new().unwrap();
    poll.register(&server, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    let mut events = Events::with_capacity(1024);

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in events.iter() {
            match event.token() {
                SERVER => {
                    let _ = server.accept();
                    println!("test");
                },

                _ => {
                    return;
                }
            }
        }
    }
}
