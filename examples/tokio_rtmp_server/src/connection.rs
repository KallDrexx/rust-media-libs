use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::sync::mpsc;
use bytes::{Bytes, BytesMut};
use rml_rtmp::handshake::{Handshake, HandshakeError, PeerType, HandshakeProcessResult};
use rml_rtmp::sessions::{
    ServerSession,
    ServerSessionConfig,
    ServerSessionResult,
    ServerSessionError
};
use std::borrow::BorrowMut;

pub async fn start_handshake(mut stream: TcpStream) -> Result<(), HandshakeError> {
    let mut handshake = Handshake::new(PeerType::Server);

    let server_p0_and_1 = handshake.generate_outbound_p0_and_p1()?;
    stream.write_all(&server_p0_and_1).await?;

    let mut buffer = [0; 4096];
    loop {
        let bytes_read = stream.read(&mut buffer).await?;
        if bytes_read == 0 {
            // Disconnection
            return Ok(());
        }

        match handshake.process_bytes(&buffer[0..bytes_read])? {
            HandshakeProcessResult::InProgress {response_bytes} => {
                stream.write_all(&response_bytes).await?;
            },

            HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
                stream.write_all(&response_bytes).await?;
                tokio::spawn(start_connection_manager(stream, remaining_bytes));
                return Ok(());
            }
        }
    }
}

async fn start_connection_manager(stream: TcpStream, received_bytes: Vec<u8>) -> Result<(), ServerSessionError> {
    let (stream_reader, stream_writer) = tokio::io::split(stream);
    let (read_bytes_sender, mut read_bytes_receiver) = mpsc::unbounded_channel();
    let (write_bytes_sender, write_bytes_receiver) = mpsc::unbounded_channel();

    tokio::spawn(connection_reader(stream_reader, read_bytes_sender));
    tokio::spawn(connection_writer(stream_writer, write_bytes_receiver));

    let config = ServerSessionConfig::new();
    let (mut session, mut results) = ServerSession::new(config)?;
    let remaining_bytes_results = session.handle_input(&received_bytes)?;

    results.extend(remaining_bytes_results);

    loop {
        for result in results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    let bytes = Bytes::from(packet.bytes);
                    write_bytes_sender.send(bytes);
                },

                ServerSessionResult::RaisedEvent(event) => {
                    println!("Event raised by connection: {:?}", event);
                },

                ServerSessionResult::UnhandleableMessageReceived(payload) => {
                    println!("Unhandleable message received: {:?}", payload);
                }
            }
        }

        while let Some(received_bytes) = read_bytes_receiver.recv().await {
            results = session.handle_input(received_bytes.as_ref())?;
        }
    }
}

async fn connection_reader(stream: Arc<TcpStream>, manager: mpsc::UnboundedSender<Bytes>)
    -> Result<(), std::io::Error>
{
    let mut buffer = BytesMut::with_capacity(4096);
    let stream = stream.borrow_mut();

    loop {
        let bytes_read = stream.read(&mut buffer.as_ref()).await?;
        if bytes_read == 0 {
            break;
        }

        let bytes = buffer.split().freeze();
        manager.send(bytes);
    }

    Ok(())
}

async fn connection_writer(stream: Arc<TcpStream>, mut bytes_to_write: mpsc::UnboundedReceiver<Bytes>)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let mut stream = &*stream;

    while let Some(bytes) = bytes_to_write.recv().await? {
        // TODO: add support for skipping writes when backlogged
        stream.write_all(bytes.as_ref()).await?;
    }

    Ok(())
}