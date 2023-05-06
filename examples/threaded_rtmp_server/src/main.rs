extern crate bytes;
extern crate rml_rtmp;
extern crate slab;

mod connection;
mod server;

use connection::{Connection, ConnectionError, ReadResult};
use server::{Server, ServerResult};
use slab::Slab;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread;

fn main() {
    let address = "0.0.0.0:1935";
    let listener = TcpListener::bind(&address).unwrap();

    let (stream_sender, stream_receiver) = channel();
    thread::spawn(|| handle_connections(stream_receiver));

    println!("Listening for connections on {}", address);
    for stream in listener.incoming() {
        println!("New connection!");
        match stream_sender.send(stream.unwrap()) {
            Ok(_) => (),
            Err(error) => panic!("Error sending stream to connection handler: {:?}", error),
        }
    }
}

fn handle_connections(connection_receiver: Receiver<TcpStream>) {
    let mut connections = Slab::new();
    let mut server = Server::new();

    loop {
        match connection_receiver.try_recv() {
            Err(TryRecvError::Disconnected) => panic!("Connection receiver closed"),
            Err(TryRecvError::Empty) => (),
            Ok(stream) => {
                let entry = connections.vacant_entry();
                let connection_id = entry.key();
                entry.insert(Connection::new(connection_id, stream));

                println!("Connection {connection_id} started");
            }
        }

        let mut ids_to_clear = Vec::new();
        let mut packets_to_write = Vec::new();
        for (connection_id, connection) in connections.iter_mut() {
            match connection.read() {
                Err(ConnectionError::SocketClosed) => {
                    println!("Socket closed for id {}", connection_id);
                    ids_to_clear.push(connection_id);
                }

                Err(error) => {
                    println!(
                        "I/O error while reading connection {}: {:?}",
                        connection_id, error
                    );
                    ids_to_clear.push(connection_id);
                }

                Ok(result) => match result {
                    ReadResult::NoBytesReceived => (),
                    ReadResult::HandshakingInProgress => (),
                    ReadResult::BytesReceived { buffer, byte_count } => {
                        let server_results =
                            match server.bytes_received(connection_id, &buffer[..byte_count]) {
                                Ok(results) => results,
                                Err(error) => {
                                    println!("Input caused the following server error: {}", error);
                                    ids_to_clear.push(connection_id);
                                    continue;
                                }
                            };

                        for result in server_results.into_iter() {
                            match result {
                                ServerResult::OutboundPacket {
                                    target_connection_id,
                                    packet,
                                } => {
                                    packets_to_write.push((target_connection_id, packet));
                                }

                                ServerResult::DisconnectConnection {
                                    connection_id: id_to_close,
                                } => {
                                    ids_to_clear.push(id_to_close);
                                }
                            }
                        }
                    }
                },
            }
        }

        for (connection_id, packet) in packets_to_write.into_iter() {
            let connection = connections.get_mut(connection_id).unwrap();
            connection.write(packet.bytes);
        }

        for closed_id in ids_to_clear {
            println!("Connection {} closed", closed_id);
            connections.remove(closed_id);
            server.notify_connection_closed(closed_id);
        }
    }
}
