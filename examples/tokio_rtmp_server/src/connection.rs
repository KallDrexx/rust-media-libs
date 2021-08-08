use tokio::net::TcpStream;
use tokio::io::{AsyncWriteExt, AsyncReadExt, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use bytes::{Bytes, BytesMut};
use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};
use rml_rtmp::sessions::{
    ServerSession,
    ServerSessionConfig,
    ServerSessionResult,
    ServerSessionEvent,
};
use tokio::sync::mpsc::UnboundedSender;
use crate::spawn;

pub async fn start_handshake(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let mut handshake = Handshake::new(PeerType::Server);

    let server_p0_and_1 = handshake.generate_outbound_p0_and_p1()
        .map_err(|x| format!("Failed to generate p0 and p1: {:?}", x))?;

    stream.write_all(&server_p0_and_1).await?;

    let mut buffer = [0; 4096];
    loop {
        let bytes_read = stream.read(&mut buffer).await?;
        if bytes_read == 0 {
            return Ok(());
        }

        match handshake.process_bytes(&buffer[0..bytes_read]).map_err(|x| format!("Failed to process bytes: {:?}", x))? {
            HandshakeProcessResult::InProgress {response_bytes} => {
                stream.write_all(&response_bytes).await?;
            },

            HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
                stream.write_all(&response_bytes).await?;
                spawn(start_connection_manager(stream, remaining_bytes));
                return Ok(());
            }
        }
    }
}

async fn start_connection_manager(stream: TcpStream, received_bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send  + Sync>> {
    let (stream_reader, stream_writer) = tokio::io::split(stream);
    let (read_bytes_sender, mut read_bytes_receiver) = mpsc::unbounded_channel();
    let (mut write_bytes_sender, write_bytes_receiver) = mpsc::unbounded_channel();

    spawn(connection_reader(stream_reader, read_bytes_sender));
    spawn(connection_writer(stream_writer, write_bytes_receiver));

    let config = ServerSessionConfig::new();
    let (mut session, mut results) = ServerSession::new(config)
        .map_err(|x| format!("Server session error occurred: {:?}", x))?;

    let remaining_bytes_results = session.handle_input(&received_bytes)
        .map_err(|x| format!("Failed to handle input: {:?}", x))?;

    results.extend(remaining_bytes_results);
    handle_session_results(&mut session, &mut results, &mut write_bytes_sender)?;

    while let Some(received_bytes) = read_bytes_receiver.recv().await {
        results = session.handle_input(&received_bytes)
            .map_err(|x| format!("Error handling input: {:?}", x))?;

        handle_session_results(&mut session, &mut results, &mut write_bytes_sender)?;
    }

    println!("Client disconnected");

    Ok(())
}

async fn connection_reader(mut stream: ReadHalf<TcpStream>, manager: mpsc::UnboundedSender<Bytes>)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let mut buffer = BytesMut::with_capacity(4096);

    loop {
        let bytes_read = stream.read_buf( &mut buffer).await?;
        if bytes_read == 0 {
            break;
        }

        let bytes = buffer.split_off(bytes_read);
        manager.send(buffer.freeze())?;
        buffer = bytes;
    }

    println!("Reader disconnected");
    Ok(())
}

async fn connection_writer(mut stream: WriteHalf<TcpStream>, mut bytes_to_write: mpsc::UnboundedReceiver<Bytes>)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    while let Some(bytes) = bytes_to_write.recv().await {
        // TODO: add support for skipping writes when backlogged
        stream.write_all(bytes.as_ref()).await?;
    }

    println!("Writer disconnected");
    Ok(())
}

fn handle_session_results(session: &mut ServerSession,
                          results: &mut Vec<ServerSessionResult>,
                          byte_writer: &mut UnboundedSender<Bytes>) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    if results.len() == 0 {
        return Ok(());
    }

    let mut new_results = Vec::new();
    for result in results.drain(..) {
        match result {
            ServerSessionResult::OutboundResponse(packet) => {
                let bytes = Bytes::from(packet.bytes);
                byte_writer.send(bytes)?;
            },

            ServerSessionResult::RaisedEvent(event) => {
                handle_raised_event(session, event, &mut new_results)?;
            },

            ServerSessionResult::UnhandleableMessageReceived(payload) => {
                println!("Unhandleable message received: {:?}", payload);
            }
        }
    }

    handle_session_results(session, &mut new_results, byte_writer);

    Ok(())
}

fn handle_raised_event(session: &mut ServerSession, event: ServerSessionEvent, new_results: &mut Vec<ServerSessionResult>) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    match event {
        ServerSessionEvent::ConnectionRequested { request_id, app_name } => {
            println!("Client requested connection to app {:?}", app_name);
            new_results.extend(
                session.accept_request(request_id)
                    .map_err(|x| format!("Error occurred accepting request: {:?}", x))?
            );
        },

        ServerSessionEvent::PublishStreamRequested { request_id, app_name, mode, stream_key } => {
            println!("Client requesting publishing on {}/{} in mode {:?}", app_name, stream_key, mode);
            new_results.extend(
                session.accept_request(request_id)
                    .map_err(|x| format!("Error accepting publish request: {:?}", x))?
            );
        },

        ServerSessionEvent::StreamMetadataChanged {stream_key, app_name, metadata} => {
            println!("New metadata published for stream key '{}': {:?}", stream_key, metadata);
        },

        ServerSessionEvent::VideoDataReceived {app_name: _app, stream_key: _key, timestamp: _time, data: _bytes} => {

        },

        x => println!("Unknown event raised: {:?}", x),
    }

    Ok(())
}
