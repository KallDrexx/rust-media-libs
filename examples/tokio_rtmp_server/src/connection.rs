use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::sync::mpsc;
use bytes::{BytesMut, BufMut};
use rml_rtmp::handshake::{Handshake, HandshakeError, PeerType, HandshakeProcessResult};
use rml_rtmp::sessions::{
    ServerSession,
    ServerSessionConfig,
    ServerSessionResult,
    ServerSessionError
};

pub fn start_handshake(mut stream: TcpStream) -> Result<(), HandshakeError> {
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

fn start_connection_manager(stream: TcpStream, received_bytes: Vec<u8>) -> Result<(), ServerSessionError> {
    let stream = Arc::new(stream);
    let (read_bytes_sender, read_bytes_receiver) = mpsc::unbounded_channel();

}

fn connection_reader(stream: Arc<TcpStream>, receiver: mpsc::UnboundedReceiver<BytesMut>)
    -> Result<(), std::io::Error>
{
    let mut buffer = BytesMut::with_capacity(4096);

}