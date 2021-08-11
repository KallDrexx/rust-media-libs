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
use crate::stream_manager::{ConnectionMessage, StreamManagerMessage};

#[derive(PartialEq, Debug)]
enum State {
    Waiting,
    Connected { app_name: String },
    PublishRequested { app_name: String, stream_key: String, request_id: u32 },
    Publishing { app_name: String, stream_key: String },
    PlaybackRequested { app_name: String, stream_key: String, request_id: u32 },
    Playing { app_name: String, stream_key: String },
}

#[derive(PartialEq)]
enum ConnectionAction {
    None,
    Disconnect,
}

pub struct Connection {
    id: i32,
    session: Option<ServerSession>,
    stream_manager_sender: mpsc::UnboundedSender<StreamManagerMessage>,
    state: State,
}

impl Connection {
    pub fn new(id: i32, stream_manager: mpsc::UnboundedSender<StreamManagerMessage>) -> Self {
        Connection {
            id,
            session: None,
            stream_manager_sender: stream_manager,
            state: State::Waiting,
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
        let (message_sender, mut message_receiver) = mpsc::unbounded_channel();

        self.stream_manager_sender.send(StreamManagerMessage::NewConnection {
            connection_id: self.id,
            sender: message_sender,
        });

        spawn(connection_reader(self.id, stream_reader, read_bytes_sender));
        spawn(connection_writer(self.id, stream_writer, write_bytes_receiver));

        let config = ServerSessionConfig::new();
        let (session, mut results) = ServerSession::new(config)
            .map_err(|x| format!("Server session error occurred: {:?}", x))?;

        self.session = Some(session);

        let remaining_bytes_results = self.session.as_mut().unwrap().handle_input(&received_bytes)
            .map_err(|x| format!("Failed to handle input: {:?}", x))?;

        results.extend(remaining_bytes_results);

        loop {
            let action = self.handle_session_results(&mut results, &mut write_bytes_sender)?;
            if action == ConnectionAction::Disconnect {
                return Ok(());
            }

            tokio::select! {
                message = read_bytes_receiver.recv() => {
                    match message {
                        None => break,
                        Some(bytes) => {
                           results = self.session.as_mut()
                                .unwrap()
                                .handle_input(&bytes)
                                .map_err(|x| format!("Error handling input: {:?}", x))?;
                        }
                    }
                }

                manager_message = message_receiver.recv() => {
                    match manager_message {
                        None => break,
                        Some(message) => {
                            results = self.handle_connection_message(message)?;
                        }
                    }
                }
            }
        }

        println!("Connection {}: Client disconnected", self.id);

        Ok(())
    }

    fn handle_connection_message(&mut self, message: ConnectionMessage)
        -> Result<(Vec<ServerSessionResult>, ConnectionAction), Box<dyn std::error::Error + Sync + Send>> {
        match message {
            ConnectionMessage::RequestAccepted { request_id } => {
                println!("Connection {}: Request accepted", self.id);

                match &self.state {
                    State::PublishRequested {stream_key, app_name, request_id} => {
                        self.state = State::Publishing {
                            app_name: app_name.clone(),
                            stream_key: stream_key.clone()
                        };

                        let results = self.session.as_mut().unwrap()
                            .accept_request(request_id.clone())?;

                        return Ok((results, ConnectionAction::None));
                    },

                    State::PlaybackRequested {app_name, stream_key, request_id} => {
                        self.state = State::Playing {
                            app_name: app_name.clone(),
                            stream_key: stream_key.clone(),
                        };

                        let results = self.session.as_mut().unwrap()
                            .accept_request(request_id.clone())?;

                        return Ok((results, ConnectionAction::None));
                    },

                    _ => {
                        eprintln!("Connection {}: Invalid state of {:?}", self.id, self.state);
                        return Ok((results, ConnectionAction::Disconnect));
                    }
                }
            },

            ConnectionMessage::RequestDenied { request_id } => {
                println!("Connection {}: Request denied", self.id);

                return Ok((Vec::new(), ConnectionAction::Disconnect));
            },

            ConnectionMessage::NewVideoData { timestamp, data } => {
            },

            ConnectionMessage::NewAudioData { timestamp, data } => {
            },

            ConnectionMessage::NewMetadata { metadata } => {
            },
        }

        Ok((Vec::new(), ConnectionAction::Disconnect))
    }

    fn handle_session_results(&mut self,
                              results: &mut Vec<ServerSessionResult>,
                              byte_writer: &mut UnboundedSender<Bytes>)
        -> Result<ConnectionAction, Box<dyn std::error::Error + Sync + Send>> {
        if results.len() == 0 {
            return Ok(ConnectionAction::None);
        }

        let mut new_results = Vec::new();
        for result in results.drain(..) {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    let bytes = Bytes::from(packet.bytes);
                    byte_writer.send(bytes)?;
                },

                ServerSessionResult::RaisedEvent(event) => {
                    let action = self.handle_raised_event(event, &mut new_results)?;
                    if action == ConnectionAction::Disconnect {
                        return Ok(ConnectionAction::Disconnect);
                    }
                },

                ServerSessionResult::UnhandleableMessageReceived(payload) => {
                    println!("Connection {}: Unhandleable message received: {:?}", self.id, payload);
                }
            }
        }

        self.handle_session_results(&mut new_results, byte_writer)?;

        Ok(ConnectionAction::None)
    }

    fn handle_raised_event(&mut self, event: ServerSessionEvent, new_results: &mut Vec<ServerSessionResult>)
        -> Result<ConnectionAction, Box<dyn std::error::Error + Sync + Send>> {
        match event {
            ServerSessionEvent::ConnectionRequested { request_id, app_name } => {
                println!("Connection {}: Client requested connection to app {:?}", self.id, app_name);

                if self.state != State::Waiting {
                    eprintln!("Connection {}: Client was not in the waiting state, but was in {:?}", self.id, self.state);
                    return Ok(ConnectionAction::Disconnect);
                }

                new_results.extend(
                    self.session.as_mut().unwrap().accept_request(request_id)
                        .map_err(|x| format!("Connection {}: Error occurred accepting request: {:?}", self.id, x))?
                );

                self.state = State::Connected {app_name};
            },

            ServerSessionEvent::PublishStreamRequested { request_id, app_name, mode, stream_key } => {
                println!("Connection {}: Client requesting publishing on {}/{} in mode {:?}", self.id, app_name, stream_key, mode);

                match &self.state {
                    State::Connected {..} => {
                        self.state = State::PublishRequested {
                            request_id: request_id.clone(),
                            app_name: app_name.clone(),
                            stream_key: stream_key.clone(),
                        };

                        self.stream_manager_sender.send(StreamManagerMessage::PublishRequest {
                            rtmp_app: app_name,
                            stream_key,
                            request_id,
                            connection_id: self.id,
                        })?;
                    },

                    _ => {
                        eprintln!("Connection {}: Client expected to be in connected state, instead was in {:?}", self.id, self.state);
                        return Ok(ConnectionAction::Disconnect);
                    }
                }
            },

            ServerSessionEvent::PlayStreamRequested {request_id, app_name, stream_key, stream_id, ..} => {
                println!("Connection {}: Client requesting playback for key {}/{}", self.id, app_name, stream_key);

                match &self.state {
                    State::Connected {..} => {
                        self.state = State::PlaybackRequested {
                            request_id: request_id.clone(),
                            app_name: app_name.clone(),
                            stream_key: stream_key.clone(),
                        };

                        self.stream_manager_sender.send(StreamManagerMessage::PlaybackRequest {
                            request_id,
                            rtmp_app: app_name,
                            stream_key,
                            connection_id: self.id,
                        })?;
                    },

                    _ => {
                        eprintln!("Connection {}: client wasn't in expected Connected state, was in {:?}", self.id, self.state);
                        return Ok(ConnectionAction::Disconnect);
                    }
                }
            },

            ServerSessionEvent::StreamMetadataChanged { stream_key, app_name: _, metadata } => {
                println!("Connection {}: New metadata published for stream key '{}': {:?}", self.id, stream_key, metadata);

                match &self.state {
                    State::Publishing {..} => {
                        self.stream_manager_sender.send(StreamManagerMessage::UpdatedStreamMetadata {
                            sending_connection_id: self.id,
                            metadata,
                        });
                    },

                    _ => {
                        eprintln!("Connection {}: expected client to be in publishing state, was in {:?}", self.id, self.state);
                        return Ok(ConnectionAction::Disconnect);
                    }
                }
            },

            ServerSessionEvent::VideoDataReceived { app_name: _app, stream_key: _key, timestamp, data } => {
                match &self.state {
                    State::Publishing {..} => {
                        self.stream_manager_sender.send(StreamManagerMessage::NewVideoData {
                            sending_connection_id: self.id,
                            timestamp,
                            data,
                        });
                    },

                    _ => {
                        eprintln!("Connection {}: expected client to be in publishing state, was in {:?}", self.id, self.state);
                        return Ok(ConnectionAction::Disconnect);
                    }
                }
            },

            x => println!("Connection {}: Unknown event raised: {:?}", self.id, x),
        }

        Ok(ConnectionAction::None)
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
