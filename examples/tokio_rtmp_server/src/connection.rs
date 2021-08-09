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

pub struct Connection {
    id: i32,
    session: Option<ServerSession>,
}

impl Connection {
    pub fn new(id: i32) -> Self {
        Connection {
            id,
            session: None
        }
    }

    pub async fn start_handshake(self, mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
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

            match handshake.process_bytes(&buffer[0..bytes_read])
                .map_err(|x| format!("Connection {}: Failed to process bytes: {:?}", self.id, x))? {
                HandshakeProcessResult::InProgress { response_bytes } => {
                    stream.write_all(&response_bytes).await?;
                },

                HandshakeProcessResult::Completed { response_bytes, remaining_bytes } => {
                    stream.write_all(&response_bytes).await?;
                    spawn(self.start_connection_manager(stream, remaining_bytes));
                    return Ok(());
                }
            }
        }
    }

    async fn start_connection_manager(mut self, stream: TcpStream, received_bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (stream_reader, stream_writer) = tokio::io::split(stream);
        let (read_bytes_sender, mut read_bytes_receiver) = mpsc::unbounded_channel();
        let (mut write_bytes_sender, write_bytes_receiver) = mpsc::unbounded_channel();

        spawn(connection_reader(self.id, stream_reader, read_bytes_sender));
        spawn(connection_writer(self.id, stream_writer, write_bytes_receiver));

        let config = ServerSessionConfig::new();
        let (session, mut results) = ServerSession::new(config)
            .map_err(|x| format!("Server session error occurred: {:?}", x))?;

        self.session = Some(session);

        let remaining_bytes_results = self.session.as_mut().unwrap().handle_input(&received_bytes)
            .map_err(|x| format!("Failed to handle input: {:?}", x))?;

        results.extend(remaining_bytes_results);
        self.handle_session_results(&mut results, &mut write_bytes_sender)?;

        while let Some(received_bytes) = read_bytes_receiver.recv().await {
            results = self.session.as_mut().unwrap().handle_input(&received_bytes)
                .map_err(|x| format!("Error handling input: {:?}", x))?;

            self.handle_session_results(&mut results, &mut write_bytes_sender)?;
        }

        println!("Connection {}: Client disconnected", self.id);

        Ok(())
    }

    fn handle_session_results(&mut self,
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
                    self.handle_raised_event(event, &mut new_results)?;
                },

                ServerSessionResult::UnhandleableMessageReceived(payload) => {
                    println!("Connection {}: Unhandleable message received: {:?}", self.id, payload);
                }
            }
        }

        self.handle_session_results(&mut new_results, byte_writer)?;

        Ok(())
    }

    fn handle_raised_event(&mut self, event: ServerSessionEvent, new_results: &mut Vec<ServerSessionResult>)
        -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        match event {
            ServerSessionEvent::ConnectionRequested { request_id, app_name } => {
                println!("Connection {}: Client requested connection to app {:?}", self.id, app_name);
                new_results.extend(
                    self.session.as_mut().unwrap().accept_request(request_id)
                        .map_err(|x| format!("Connection {}: Error occurred accepting request: {:?}", self.id, x))?
                );
            },

            ServerSessionEvent::PublishStreamRequested { request_id, app_name, mode, stream_key } => {
                println!("Connection {}: Client requesting publishing on {}/{} in mode {:?}", self.id, app_name, stream_key, mode);
                new_results.extend(
                    self.session.as_mut().unwrap().accept_request(request_id)
                        .map_err(|x| format!("Connection {}: Error accepting publish request: {:?}", self.id, x))?
                );
            },

            ServerSessionEvent::StreamMetadataChanged { stream_key, app_name: _, metadata } => {
                println!("Connection {}: New metadata published for stream key '{}': {:?}", self.id, stream_key, metadata);
            },

            ServerSessionEvent::VideoDataReceived { app_name: _app, stream_key: _key, timestamp: _time, data: _bytes } => {},

            x => println!("Connection {}: Unknown event raised: {:?}", self.id, x),
        }

        Ok(())
    }


}

async fn connection_reader(connection_id: i32, mut stream: ReadHalf<TcpStream>, manager: mpsc::UnboundedSender<Bytes>)
                           -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let mut buffer = BytesMut::with_capacity(4096);

    loop {
        let bytes_read = stream.read_buf(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }

        let bytes = buffer.split_off(bytes_read);
        manager.send(buffer.freeze())?;
        buffer = bytes;
    }

    println!("Connection {}: Reader disconnected", connection_id);
    Ok(())
}

async fn connection_writer(connection_id: i32, mut stream: WriteHalf<TcpStream>, mut bytes_to_write: mpsc::UnboundedReceiver<Bytes>)
                           -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    while let Some(bytes) = bytes_to_write.recv().await {
        // TODO: add support for skipping writes when backlogged
        stream.write_all(bytes.as_ref()).await?;
    }

    println!("Connection {}: Writer disconnected", connection_id);
    Ok(())
}
