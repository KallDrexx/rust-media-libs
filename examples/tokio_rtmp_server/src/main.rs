use tokio::net::{TcpListener};
use std::future::Future;
use std::fmt::Display;

mod connection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Listening for connections on port 1935");
    let listener = TcpListener::bind("0.0.0.0:1935").await?;

    loop {
        let (stream, connection_info) = listener.accept().await?;

        println!("Connection received from {}", connection_info.ip());
        let _ = tokio::spawn(connection::start_handshake(stream));
    }
}

async fn spawn<F, E>(future: F)
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