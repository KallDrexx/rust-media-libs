use tokio::net::{TcpListener};
use std::future::Future;
use std::fmt::Display;
use crate::connection::Connection;

mod connection;
mod stream_manager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Listening for connections on port 1935");
    let listener = TcpListener::bind("0.0.0.0:1935").await?;
    let mut current_id = 0;

    loop {
        let (stream, connection_info) = listener.accept().await?;

        let connection = Connection::new(current_id);
        println!("Connection {}: Connection received from {}", current_id, connection_info.ip());

        spawn(connection.start_handshake(stream));
        current_id = current_id + 1;
    }
}

fn spawn<F, E>(future: F)
where
    F: Future<Output = Result<(), E>> + Send  + 'static,
    E: Display,
{
    tokio::task::spawn(async {
        if let Err(error) = future.await {
            eprintln!("{}", error);
        }
    });
}