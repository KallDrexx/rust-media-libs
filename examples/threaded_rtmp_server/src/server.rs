use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use bytes::Bytes;
use slab::Slab;
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent};
use rml_rtmp::sessions::StreamMetadata;
use rml_rtmp::chunk_io::Packet;
use rml_rtmp::time::RtmpTimestamp;

enum ClientAction {
    Waiting,
    Publishing(String), // Publishing to a stream key
    Watching {
        stream_key: String,
        stream_id: u32,
    },
}

enum ReceivedDataType {Audio, Video}

struct Client {
    session: ServerSession,
    current_action: ClientAction,
    connection_id: usize,
    has_received_video_keyframe: bool,
}

impl Client {
    fn get_active_stream_id(&self) -> Option<u32> {
        match self.current_action {
            ClientAction::Waiting => None,
            ClientAction::Publishing(_) => None,
            ClientAction::Watching {stream_key: _, stream_id} => Some(stream_id),
        }
    }
}

struct MediaChannel {
    publishing_client_id: Option<usize>,
    watching_client_ids: HashSet<usize>,
    metadata: Option<Rc<StreamMetadata>>,
    video_sequence_header: Option<Bytes>,
    audio_sequence_header: Option<Bytes>,
}

#[derive(Debug)]
pub enum ServerResult {
    DisconnectConnection {connection_id: usize},
    OutboundPacket {
        target_connection_id: usize,
        packet: Packet,
    }
}

pub struct Server {
    clients: Slab<Client>,
    connection_to_client_map: HashMap<usize, usize>,
    channels: HashMap<String, MediaChannel>,
}

impl Server {
    pub fn new() -> Server {
        Server {
            clients: Slab::with_capacity(1024),
            connection_to_client_map: HashMap::with_capacity(1024),
            channels: HashMap::new(),
        }
    }

    pub fn bytes_received(&mut self, connection_id: usize, bytes: &[u8]) -> Result<Vec<ServerResult>, String> {
        let mut server_results = Vec::new();

        if !self.connection_to_client_map.contains_key(&connection_id) {
            let config = ServerSessionConfig::new();
            let (session, initial_session_results) = match ServerSession::new(config) {
                Ok(results) => results,
                Err(error) => return Err(error.to_string()),
            };

            self.handle_session_results(connection_id, initial_session_results, &mut server_results);
            let client = Client {
                session,
                connection_id,
                current_action: ClientAction::Waiting,
                has_received_video_keyframe: false,
            };

            let client_id = Some(self.clients.insert(client));
            self.connection_to_client_map.insert(connection_id, client_id.unwrap());
        }

        let client_results: Vec<ServerSessionResult>;
        {
            let client_id = self.connection_to_client_map.get(&connection_id).unwrap();
            let client = self.clients.get_mut(*client_id).unwrap();
            client_results = match client.session.handle_input(bytes) {
                Ok(results) => results,
                Err(error) => return Err(error.to_string()),
            };
        }

        self.handle_session_results(connection_id, client_results, &mut server_results);
        Ok(server_results)
    }

    pub fn notify_connection_closed(&mut self, connection_id: usize) {
        match self.connection_to_client_map.remove(&connection_id) {
            None => (),
            Some(client_id) => {
                let client = self.clients.remove(client_id);
                match client.current_action {
                    ClientAction::Publishing(stream_key) => self.publishing_ended(stream_key),
                    ClientAction::Watching{stream_key, stream_id: _} => self.play_ended(client_id, stream_key),
                    ClientAction::Waiting => (),
                }
            },
        }
    }

    fn handle_session_results(&mut self,
                              executed_connection_id: usize,
                              session_results: Vec<ServerSessionResult>,
                              server_results: &mut Vec<ServerResult>) {
        for result in session_results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    server_results.push(ServerResult::OutboundPacket {
                        target_connection_id: executed_connection_id,
                        packet,
                    })
                },

                ServerSessionResult::RaisedEvent(event) =>
                    self.handle_raised_event(executed_connection_id, event, server_results),

                x => println!("Server result received: {:?}", x),
            }
        }
    }

    fn handle_raised_event(&mut self,
                           executed_connection_id: usize,
                           event: ServerSessionEvent,
                           server_results: &mut Vec<ServerResult>) {
        match event {
            ServerSessionEvent::ConnectionRequested {request_id, app_name} => {
                self.handle_connection_requested(executed_connection_id, request_id, app_name, server_results);
            },

            ServerSessionEvent::PublishStreamRequested {request_id, app_name, stream_key, mode: _} => {
                self.handle_publish_requested(executed_connection_id, request_id, app_name, stream_key, server_results);
            },

            ServerSessionEvent::PlayStreamRequested {request_id, app_name, stream_key, start_at: _, duration: _, reset: _, stream_id} => {
                self.handle_play_requested(executed_connection_id, request_id, app_name, stream_key, stream_id, server_results);
            },

            ServerSessionEvent::StreamMetadataChanged {app_name, stream_key, metadata} => {
                self.handle_metadata_received(app_name, stream_key, metadata, server_results);
            },

            ServerSessionEvent::VideoDataReceived {app_name: _, stream_key, data, timestamp} => {
                self.handle_audio_video_data_received(stream_key, timestamp, data, ReceivedDataType::Video, server_results);
            },

            ServerSessionEvent::AudioDataReceived {app_name: _, stream_key, data, timestamp} => {
                self.handle_audio_video_data_received(stream_key, timestamp, data, ReceivedDataType::Audio, server_results);
            },

            _ => println!("Event raised by connection {}: {:?}", executed_connection_id, event),
        }
    }

    fn handle_connection_requested(&mut self,
                                   requested_connection_id: usize,
                                   request_id: u32,
                                   app_name: String,
                                   server_results: &mut Vec<ServerResult>) {
        println!("Connection {} requested connection to app '{}'", requested_connection_id, app_name);

        let accept_result;
        {
            let client_id = self.connection_to_client_map.get(&requested_connection_id).unwrap();
            let client = self.clients.get_mut(*client_id).unwrap();
            accept_result = client.session.accept_request(request_id);
        }

        match accept_result {
            Err(error) => {
                println!("Error occurred accepting connection request: {:?}", error);
                server_results.push(ServerResult::DisconnectConnection {
                    connection_id: requested_connection_id}
                )
            },

            Ok(results) => {
                self.handle_session_results(requested_connection_id, results, server_results);
            }
        }
    }

    fn handle_publish_requested(&mut self,
                                   requested_connection_id: usize,
                                   request_id: u32,
                                   app_name: String,
                                   stream_key: String,
                                   server_results: &mut Vec<ServerResult>) {
        println!("Publish requested on app '{}' and stream key '{}'", app_name, stream_key);

        match self.channels.get(&stream_key) {
            None => (),
            Some(channel) => match channel.publishing_client_id {
                None => (),
                Some(_) => {
                    println!("Stream key already being published to");
                    server_results.push(ServerResult::DisconnectConnection {connection_id: requested_connection_id});
                    return;
                }
            }
        }

        let accept_result;
        {
            let client_id = self.connection_to_client_map.get(&requested_connection_id).unwrap();
            let client = self.clients.get_mut(*client_id).unwrap();
            client.current_action = ClientAction::Publishing(stream_key.clone());

            let channel = self.channels
                .entry(stream_key)
                .or_insert(MediaChannel {
                    publishing_client_id: None,
                    watching_client_ids: HashSet::new(),
                    metadata: None,
                    video_sequence_header: None,
                    audio_sequence_header: None,
                });

            channel.publishing_client_id = Some(*client_id);
            accept_result = client.session.accept_request(request_id);
        }

        match accept_result {
            Err(error) => {
                println!("Error occurred accepting publish request: {:?}", error);
                server_results.push(ServerResult::DisconnectConnection {
                    connection_id: requested_connection_id}
                )
            },

            Ok(results) => {
                self.handle_session_results(requested_connection_id, results, server_results);
            }
        }
    }

    fn handle_play_requested(&mut self,
                             requested_connection_id: usize,
                             request_id: u32,
                             app_name: String,
                             stream_key: String,
                             stream_id: u32,
                             server_results: &mut Vec<ServerResult>) {
        println!("Play requested on app '{}' and stream key '{}'", app_name, stream_key);

        let accept_result;
        {
            let client_id = self.connection_to_client_map.get(&requested_connection_id).unwrap();
            let client = self.clients.get_mut(*client_id).unwrap();
            client.current_action = ClientAction::Watching {
                stream_key: stream_key.clone(),
                stream_id,
            };

            let channel = self.channels
                .entry(stream_key.clone())
                .or_insert(MediaChannel {
                    publishing_client_id: None,
                    watching_client_ids: HashSet::new(),
                    metadata: None,
                    video_sequence_header: None,
                    audio_sequence_header: None,
                });

            channel.watching_client_ids.insert(*client_id);
            accept_result = match client.session.accept_request(request_id) {
                Err(error) => Err(error),
                Ok(mut results) => {

                    // If the channel already has existing metadata, send that to the new client
                    // so they have up to date info
                    match channel.metadata {
                        None => (),
                        Some(ref metadata) => {
                            let packet = match client.session.send_metadata(stream_id, metadata.clone()) {
                                Ok(packet) => packet,
                                Err(error) => {
                                    println!("Error occurred sending existing metadata to new client: {:?}", error);
                                    server_results.push(ServerResult::DisconnectConnection {
                                        connection_id: requested_connection_id}
                                    );

                                    return;
                                },
                            };

                            results.push(ServerSessionResult::OutboundResponse(packet));
                        }
                    }

                    // If the channel already has sequence headers, send them
                    match channel.video_sequence_header {
                        None => (),
                        Some(ref data) => {
                            let packet = match client.session.send_video_data(stream_id, data.clone(), RtmpTimestamp::new(0), false) {
                                Ok(packet) => packet,
                                Err(error) => {
                                    println!("Error occurred sending video header to new client: {:?}", error);
                                    server_results.push(ServerResult::DisconnectConnection {
                                        connection_id: requested_connection_id}
                                    );

                                    return;
                                },
                            };

                            results.push(ServerSessionResult::OutboundResponse(packet));
                        }
                    }

                    match channel.audio_sequence_header {
                        None => (),
                        Some(ref data) => {
                            let packet = match client.session.send_audio_data(stream_id, data.clone(), RtmpTimestamp::new(0), false) {
                                Ok(packet) => packet,
                                Err(error) => {
                                    println!("Error occurred sending audio header to new client: {:?}", error);
                                    server_results.push(ServerResult::DisconnectConnection {
                                        connection_id: requested_connection_id}
                                    );

                                    return;
                                },
                            };

                            results.push(ServerSessionResult::OutboundResponse(packet));
                        }
                    }

                    Ok(results)
                }
            }
        }

        match accept_result {
            Err(error) => {
                println!("Error occurred accepting playback request: {:?}", error);
                server_results.push(ServerResult::DisconnectConnection {
                    connection_id: requested_connection_id}
                );

                return;
            },

            Ok(results) => {
                self.handle_session_results(requested_connection_id, results, server_results);
            }
        }
    }

    fn handle_metadata_received(&mut self,
                                app_name: String,
                                stream_key: String,
                                metadata: StreamMetadata,
                                server_results: &mut Vec<ServerResult>) {
        println!("New metadata received for app '{}' and stream key '{}'", app_name, stream_key);
        let channel = match self.channels.get_mut(&stream_key) {
            Some(channel) => channel,
            None => return,
        };

        let metadata = Rc::new(metadata);
        channel.metadata = Some(metadata.clone());

        // Send the metadata to all current watchers
        for client_id in &channel.watching_client_ids {
            let client = match self.clients.get_mut(*client_id) {
                Some(client) => client,
                None => continue,
            };

            let active_stream_id = match client.get_active_stream_id() {
                Some(stream_id) => stream_id,
                None => continue,
            };

            match client.session.send_metadata(active_stream_id, metadata.clone()) {
                Ok(packet) => {
                    server_results.push(ServerResult::OutboundPacket {
                        target_connection_id: client.connection_id,
                        packet,
                    })
                },

                Err(error) => {
                    println!("Error sending metadata to client on connection id {}: {:?}", client.connection_id, error);
                    server_results.push(ServerResult::DisconnectConnection {
                        connection_id: client.connection_id
                    });
                },
            }
        }
    }

    fn handle_audio_video_data_received(&mut self,
                                        stream_key: String,
                                        timestamp: RtmpTimestamp,
                                        data: Bytes,
                                        data_type: ReceivedDataType,
                                        server_results: &mut Vec<ServerResult>) {
        let channel = match self.channels.get_mut(&stream_key) {
            Some(channel) => channel,
            None => return,
        };

        // If this is an audio or video sequence header we need to save it, so it can be
        // distributed to any late coming watchers
        match data_type {
            ReceivedDataType::Video => {
                if is_video_sequence_header(data.clone()) {
                    channel.video_sequence_header = Some(data.clone());
                }
            },

            ReceivedDataType::Audio => {
                if is_audio_sequence_header(data.clone()) {
                    channel.audio_sequence_header = Some(data.clone());
                }
            }
        }

        for client_id in &channel.watching_client_ids {
            let client = match self.clients.get_mut(*client_id) {
                Some(client) => client,
                None => continue,
            };

            let active_stream_id = match client.get_active_stream_id() {
                Some(stream_id) => stream_id,
                None => continue,
            };

            let should_send_to_client = match data_type {
                ReceivedDataType::Video => {
                    client.has_received_video_keyframe ||
                        (is_video_sequence_header(data.clone()) ||
                            is_video_keyframe(data.clone()))
                },

                ReceivedDataType::Audio => {
                    client.has_received_video_keyframe || is_audio_sequence_header(data.clone())
                },
            };

            if !should_send_to_client {
                continue;
            }

            let send_result = match data_type {
                ReceivedDataType::Audio => client.session.send_audio_data(active_stream_id, data.clone(), timestamp.clone(), true),
                ReceivedDataType::Video => {
                    if is_video_keyframe(data.clone()) {
                        client.has_received_video_keyframe = true;
                    }

                    client.session.send_video_data(active_stream_id, data.clone(), timestamp.clone(), true)
                },
            };

            match send_result {
                Ok(packet) => {
                    server_results.push(ServerResult::OutboundPacket {
                        target_connection_id: client.connection_id,
                        packet,
                    })
                },

                Err(error) => {
                    println!("Error sending metadata to client on connection id {}: {:?}", client.connection_id, error);
                    server_results.push(ServerResult::DisconnectConnection {
                        connection_id: client.connection_id
                    });
                },
            }
        }
    }

    fn publishing_ended(&mut self, stream_key: String) {
        let channel = match self.channels.get_mut(&stream_key) {
            Some(channel) => channel,
            None => return,
        };

        channel.publishing_client_id = None;
        channel.metadata = None;
    }

    fn play_ended(&mut self, client_id: usize, stream_key: String) {
        let channel = match self.channels.get_mut(&stream_key) {
            Some(channel) => channel,
            None => return,
        };

        channel.watching_client_ids.remove(&client_id);
    }
}

fn is_video_sequence_header(data: Bytes) -> bool {
    // This is assuming h264.
    return data.len() >= 2 &&
        data[0] == 0x17 &&
        data[1] == 0x00;
}

fn is_audio_sequence_header(data: Bytes) -> bool {
    // This is assuming aac
    return data.len() >= 2 &&
        data[0] == 0xaf &&
        data[1] == 0x00;
}

fn is_video_keyframe(data: Bytes) -> bool {
    // assumings h264
    return data.len() >= 2 &&
        data[0] == 0x17 &&
        data[1] != 0x00; // 0x00 is the sequence header, don't count that for now
}
