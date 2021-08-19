extern crate rml_rtmp;

use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn main() {
    let mut args: Vec<String> = env::args().collect();
    args.drain(0..1); // remove the executable

    if args.len() == 0 || ((args[0] != "client" && args.len() < 2) && args[0] != "server") {
        println!("No arguments provided.  One of the following must be provided");
        println!("Act as a server: server");
        println!("Act as a client: client <server host>");
    } else if args[0] == "client" {
        act_as_client(&args[1]);
    } else if args[0] == "server" {
        act_as_server();
    }
}

fn act_as_client(host_address: &str) {
    let mut stream = TcpStream::connect(host_address).unwrap();
    let mut handshake = Handshake::new(PeerType::Client);
    let c0_and_c1 = handshake.generate_outbound_p0_and_p1().unwrap();
    stream.write(&c0_and_c1).unwrap();

    let mut read_buffer = [0_u8; 1024];

    loop {
        let bytes_read = stream.read(&mut read_buffer).unwrap();
        let (is_finished, response_bytes) =
            match handshake.process_bytes(&read_buffer[..bytes_read]) {
                Err(x) => panic!("Error returned: {:?}", x),
                Ok(HandshakeProcessResult::InProgress {
                    response_bytes: bytes,
                }) => (false, bytes),
                Ok(HandshakeProcessResult::Completed {
                    response_bytes: bytes,
                    remaining_bytes: _,
                }) => (true, bytes),
            };

        if response_bytes.len() > 0 {
            stream.write(&response_bytes).unwrap();
        }

        if is_finished {
            println!("Handshaking Completed!");
            break;
        } else {
            println!("Handshake still in progress");
        }
    }
}

fn act_as_server() {
    let listener = TcpListener::bind("127.0.0.1:1935").unwrap();
    println!("Listening on port 1935");

    for stream in listener.incoming() {
        println!("Incoming connection");
        let mut stream = stream.unwrap();
        let mut handshake = Handshake::new(PeerType::Server);
        let mut read_buffer = [0_u8; 1024];

        loop {
            let bytes_read = stream.read(&mut read_buffer).unwrap();
            let (is_finished, response_bytes) =
                match handshake.process_bytes(&read_buffer[..bytes_read]) {
                    Err(x) => panic!("Error returned: {:?}", x),
                    Ok(HandshakeProcessResult::InProgress {
                        response_bytes: bytes,
                    }) => (false, bytes),
                    Ok(HandshakeProcessResult::Completed {
                        response_bytes: bytes,
                        remaining_bytes: _,
                    }) => (true, bytes),
                };

            if response_bytes.len() > 0 {
                stream.write(&response_bytes).unwrap();
            }

            if is_finished {
                println!("Handshaking Completed!");
                break;
            } else {
                println!("Handshake still in progress");
            }
        }
    }
}
