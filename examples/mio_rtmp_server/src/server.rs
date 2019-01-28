use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use bytes::Bytes;
use slab::Slab;
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent};
use rml_rtmp::sessions::{ClientSession, ClientSessionConfig, ClientSessionResult, ClientSessionEvent};
use rml_rtmp::sessions::{StreamMetadata, PublishRequestType};
use rml_rtmp::chunk_io::Packet;
use rml_rtmp::time::RtmpTimestamp;
use super::PushOptions;

enum ReceivedDataType {Audio, Video}

enum InboundClientAction {
    Waiting,
    Publishing(String), // Publishing to a stream key
    Watching {
        stream_key: String,
        stream_id: u32,
    },
}

struct InboundClient {
    session: ServerSession,
    current_action: InboundClientAction,
    connection_id: usize,
    has_received_video_keyframe: bool,
}

impl InboundClient {
    fn get_active_stream_id(&self) -> Option<u32> {
        match self.current_action {
            InboundClientAction::Waiting => None,
            InboundClientAction::Publishing(_) => None,
            InboundClientAction::Watching {stream_key: _, stream_id} => Some(stream_id),
        }
    }
}

enum PullState {
    Handshaking,
    Connecting,
    Connected,
    Pulling,
}

struct PullClient {
    session: Option<ClientSession>,
    connection_id: usize,
    pull_app: String,
    pull_stream: String,
    pull_target_stream: String,
    state: PullState,
}

#[derive(PartialEq, Clone, Debug)]
enum PushState {
    Inactive,
    WaitingForConnection,
    Handshaking,
    Connecting,
    Connected,
    Pushing,
}

struct PushClient {
    session: Option<ClientSession>,
    connection_id: Option<usize>,
    push_app: String,
    push_source_stream: String,
    push_target_stream: String,
    state: PushState,
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
    },
    StartPushing,
}

pub struct Server {
    clients: Slab<InboundClient>,
    connection_to_client_map: HashMap<usize, usize>,
    channels: HashMap<String, MediaChannel>,
    pull_client: Option<PullClient>,
    push_client: Option<PushClient>,
}

impl Server {
    pub fn new(push_options: &Option<PushOptions>) -> Server {
        let push_client = match push_options {
            &None => None,
            &Some(ref options) => {
                Some(PushClient {
                    push_app: options.app.clone(),
                    push_source_stream: options.source_stream.clone(),
                    push_target_stream: options.target_stream.clone(),
                    connection_id: None,
                    session: None,
                    state: PushState::Inactive,
                })
            }
        };

        Server {
            clients: Slab::with_capacity(1024),
            connection_to_client_map: HashMap::with_capacity(1024),
            channels: HashMap::new(),
            pull_client: None,
            push_client,
        }
    }

    pub fn register_pull_client(&mut self,
                                connection_id: usize,
                                app: String,
                                stream: String,
                                target_stream: String) {
        // Pre-create the target channel.
        self.channels
            .entry(target_stream.clone())
            .or_insert(MediaChannel {
                publishing_client_id: Some(connection_id),
                watching_client_ids: HashSet::new(),
                metadata: None,
                video_sequence_header: None,
                audio_sequence_header: None,
            });

        self.pull_client = Some(PullClient {
            session: None,
            pull_app: app,
            pull_stream: stream,
            pull_target_stream: target_stream,
            state: PullState::Handshaking,
            connection_id,
        });
    }

    pub fn register_push_client(&mut self, connection_id: usize) {
        if let Some(ref mut client) = self.push_client {
            client.connection_id = Some(connection_id);
            client.state = PushState::Handshaking;
        }
    }

    pub fn bytes_received(&mut self, connection_id: usize, bytes: &[u8]) -> Result<Vec<ServerResult>, String> {
        let mut server_results = Vec::new();

        let push_client_connection_id = self.push_client
            .as_ref()
            .map_or(None, |c| if let Some(connection_id) = c.connection_id {
                Some(connection_id)
            } else {
                None
            });

        let pull_client_connection_id = self.pull_client
            .as_ref()
            .map_or(None, |c| Some(c.connection_id));

        if pull_client_connection_id.as_ref().map_or(false, |id| *id == connection_id) {
            // These bytes were received by the current pull client

            let mut initial_session_results = Vec::new();
            if let Some(ref mut pull_client) = self.pull_client {
                if pull_client.session.is_none() {
                    let (session, session_results) = ClientSession::new(ClientSessionConfig::new()).unwrap();
                    pull_client.session = Some(session);

                    for result in session_results {
                        initial_session_results.push(result);
                    }
                }
            }

            let session_results = match self.pull_client.as_mut().unwrap().session.as_mut().unwrap().handle_input(bytes) {
                Ok(results) => results,
                Err(error) => return Err(error.to_string()),
            };

            if initial_session_results.len() > 0 {
                self.handle_push_session_results(initial_session_results, &mut server_results);
            }

            self.handle_pull_session_results(session_results, &mut server_results);
        } else if push_client_connection_id.as_ref().map_or(false, |id| *id == connection_id) {
            // These bytes were received by the current push client
            let mut initial_session_results = Vec::new();

            let session_results = if let Some(ref mut push_client) = self.push_client {
                if push_client.session.is_none() {
                    let (session, session_results) = ClientSession::new(ClientSessionConfig::new()).unwrap();
                    push_client.session = Some(session);

                    for result in session_results {
                        initial_session_results.push(result);
                    }
                }

                match push_client.session.as_mut().unwrap().handle_input(bytes) {
                    Ok(results) => results,
                    Err(error) => return Err(error.to_string()),
                }
            } else {
                Vec::new()
            };

            if initial_session_results.len() > 0 {
                self.handle_push_session_results(initial_session_results, &mut server_results);
            }

            self.handle_push_session_results(session_results, &mut server_results);
        } else {
            // Since the pull client did not send these bytes, map it to an inbound client
            if !self.connection_to_client_map.contains_key(&connection_id) {
                let config = ServerSessionConfig::new();
                let (session, initial_session_results) = match ServerSession::new(config) {
                    Ok(results) => results,
                    Err(error) => return Err(error.to_string()),
                };

                self.handle_server_session_results(connection_id, initial_session_results, &mut server_results);
                let client = InboundClient {
                    session,
                    connection_id,
                    current_action: InboundClientAction::Waiting,
                    has_received_video_keyframe: false,
                };

                let client_id = Some(self.clients.insert(client));
                self.connection_to_client_map.insert(connection_id, client_id.unwrap());
            }

            let client_results;
            {
                let client_id = self.connection_to_client_map.get(&connection_id).unwrap();
                let client = self.clients.get_mut(*client_id).unwrap();
                client_results = match client.session.handle_input(bytes) {
                    Ok(results) => results,
                    Err(error) => return Err(error.to_string()),
                };
            }

            self.handle_server_session_results(connection_id, client_results, &mut server_results);
        }

        Ok(server_results)
    }

    pub fn notify_connection_closed(&mut self, connection_id: usize) {
        if self.pull_client.as_ref().map_or(false, |c| c.connection_id == connection_id) {
            self.pull_client = None;
        } else {
            match self.connection_to_client_map.remove(&connection_id) {
                None => (),
                Some(client_id) => {
                    let client = self.clients.remove(client_id);
                    match client.current_action {
                        InboundClientAction::Publishing(stream_key) => self.publishing_ended(stream_key),
                        InboundClientAction::Watching{stream_key, stream_id: _} => self.play_ended(client_id, stream_key),
                        InboundClientAction::Waiting => (),
                    }
                },
            }
        }
    }

    fn handle_server_session_results(&mut self,
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
                self.handle_server_session_results(requested_connection_id, results, server_results);
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
            client.current_action = InboundClientAction::Publishing(stream_key.clone());

            let channel = self.channels
                .entry(stream_key.clone())
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
                    connection_id: requested_connection_id
                })
            },

            Ok(results) => {
                if let Some(ref mut client) = self.push_client {
                    if client.state == PushState::Inactive {
                        if app_name == client.push_app && stream_key == client.push_source_stream {
                            println!("Publishing on the push source stream key!");
                            client.state = PushState::WaitingForConnection;
                            server_results.push(ServerResult::StartPushing)

                        } else {
                            println!("Not publishing on the push source stream key!");
                        }
                    }
                }

                self.handle_server_session_results(requested_connection_id, results, server_results);
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
            client.current_action = InboundClientAction::Watching {
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
                self.handle_server_session_results(requested_connection_id, results, server_results);
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
        {
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
                        client.has_received_video_keyframe ||
                            is_audio_sequence_header(data.clone())
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
                        println!("Error sending a/v data to client on connection id {}: {:?}", client.connection_id, error);
                        server_results.push(ServerResult::DisconnectConnection {
                            connection_id: client.connection_id
                        });
                    },
                }
            }
        }

        let mut push_results = Vec::new();
        {
            if let Some(ref mut client) = self.push_client {
                if client.state == PushState::Pushing {
                    let result = match data_type {
                        ReceivedDataType::Video => {
                            client.session.as_mut().unwrap().publish_video_data(data.clone(), timestamp.clone(), true)
                        },

                        ReceivedDataType::Audio => {
                            client.session.as_mut().unwrap().publish_audio_data(data.clone(), timestamp.clone(), true)
                        },
                    };

                    match result {
                        Ok(client_result) => push_results.push(client_result),
                        Err(error) => {
                            println!("Error sending a/v data to push client: {:?}", error);
                        },
                    }
                }
            }
        }


        if !push_results.is_empty() {
            self.handle_push_session_results(push_results, server_results);
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

    fn handle_pull_session_results(&mut self,
                                   session_results: Vec<ClientSessionResult>,
                                   server_results: &mut Vec<ServerResult>) {
        let mut new_results = Vec::new();
        let mut events = Vec::new();
        if let Some(ref mut client) = self.pull_client {
            for result in session_results {
                match result {
                    ClientSessionResult::OutboundResponse(packet) => {
                        server_results.push(ServerResult::OutboundPacket {
                            target_connection_id: client.connection_id,
                            packet,
                        });
                    },

                    ClientSessionResult::RaisedEvent(event) => {
                        events.push(event);
                    },

                    x => println!("Client result received: {:?}", x),
                }
            }

            match client.state {
                PullState::Handshaking => {
                    // Since this was called we know we are no longer handshaking, so we need to
                    // initiate the connect to the RTMP app
                    client.state = PullState::Connecting;

                    let result = client.session.as_mut().unwrap().request_connection(client.pull_app.clone())
                        .unwrap();
                    new_results.push(result);
                },

                _ => (),
            }
        }

        if !new_results.is_empty() {
            self.handle_pull_session_results(new_results, server_results);
        }

        for event in events {
            match event {
                ClientSessionEvent::ConnectionRequestAccepted => {
                    self.handle_pull_connection_accepted_event(server_results);
                },

                ClientSessionEvent::PlaybackRequestAccepted {..} => {
                    self.handle_pull_playback_accepted_event(server_results);
                },

                ClientSessionEvent::VideoDataReceived {data, timestamp} => {
                    self.handle_pull_audio_video_data_received(data, ReceivedDataType::Video, timestamp, server_results);
                },

                ClientSessionEvent::AudioDataReceived {data, timestamp} => {
                    self.handle_pull_audio_video_data_received(data, ReceivedDataType::Audio, timestamp, server_results);
                },

                ClientSessionEvent::StreamMetadataReceived {metadata} => {
                    self.handle_pull_metadata_received(metadata, server_results);
                },

                x => println!("Unhandled event raised: {:?}", x),
            }
        }
    }

    fn handle_pull_connection_accepted_event(&mut self, server_results: &mut Vec<ServerResult>) {
        let mut new_results = Vec::new();
        if let Some(ref mut client) = self.pull_client {
            println!("Pull accepted for app '{}'", client.pull_app);
            client.state = PullState::Connected;

            let result = client.session.as_mut().unwrap().request_playback(client.pull_stream.clone()).unwrap();
            let mut results = vec![result];
            new_results.append(&mut results);
        }

        if !new_results.is_empty() {
            self.handle_pull_session_results(new_results, server_results);
        }
    }

    fn handle_pull_playback_accepted_event(&mut self, _server_results: &mut Vec<ServerResult>) {
        if let Some(ref mut client) = self.pull_client {
            println!("Playback accepted for stream '{}'", client.pull_stream);
            client.state = PullState::Pulling;
        }
    }

    fn handle_pull_audio_video_data_received(&mut self,
                                       data: Bytes,
                                       data_type: ReceivedDataType,
                                       timestamp: RtmpTimestamp,
                                       server_results: &mut Vec<ServerResult>) {
        let stream_key = match self.pull_client {
            Some(ref client) => client.pull_target_stream.clone(),
            None => return,
        };

        self.handle_audio_video_data_received(stream_key, timestamp, data, data_type, server_results);
    }

    fn handle_pull_metadata_received(&mut self,
                                metadata: StreamMetadata,
                                server_results: &mut Vec<ServerResult>) {
        let (app_name, stream_key) = match self.pull_client {
            Some(ref client) => (client.pull_target_stream.clone(), client.pull_app.clone()),
            None => return,
        };

        self.handle_metadata_received(app_name, stream_key, metadata, server_results);
    }

    fn handle_push_session_results(&mut self,
                                   session_results: Vec<ClientSessionResult>,
                                   server_results: &mut Vec<ServerResult>) {
        let mut new_results = Vec::new();
        let mut events = Vec::new();
        if let Some(ref mut client) = self.push_client {
            for result in session_results {
                match result {
                    ClientSessionResult::OutboundResponse(packet) => {
                        server_results.push(ServerResult::OutboundPacket {
                            target_connection_id: client.connection_id.unwrap(),
                            packet,
                        });
                    },

                    ClientSessionResult::RaisedEvent(event) => {
                        events.push(event);
                    },

                    x => println!("Push client result received: {:?}", x),
                }
            }

            match client.state {
                PushState::Handshaking => {
                    // Since we got here we know handshaking was successful, so we need
                    // to initiate the connection process
                    client.state = PushState::Connecting;

                    let result = match client.session.as_mut().unwrap().request_connection(client.push_app.clone()) {
                        Ok(result) => result,
                        Err(error) => {
                            println!("Failed to request connection for push client: {:?}", error);
                            return;
                        },
                    };

                    new_results.push(result);
                },
                _ => (),
            }
        }

        if !new_results.is_empty() {
            self.handle_push_session_results(new_results, server_results);
        }

        for event in events {
            match event {
                ClientSessionEvent::ConnectionRequestAccepted => {
                    self.handle_push_connection_accepted_event(server_results);
                }

                ClientSessionEvent::PublishRequestAccepted => {
                    self.handle_push_publish_accepted_event(server_results);
                }

                x => println!("Push event raised: {:?}", x),
            }
        }
    }

    fn handle_push_connection_accepted_event(&mut self, server_results: &mut Vec<ServerResult>) {
        let mut new_results = Vec::new();
        if let Some(ref mut client) = self.push_client {
            println!("push accepted for app '{}'", client.push_app);
            client.state = PushState::Connected;

            let result = client.session.as_mut().unwrap()
                .request_publishing(client.push_target_stream.clone(), PublishRequestType::Live)
                .unwrap();

            let mut results = vec![result];
            new_results.append(&mut results);
        }

        if !new_results.is_empty() {
            self.handle_push_session_results(new_results, server_results);
        }
    }

    fn handle_push_publish_accepted_event(&mut self, server_results: &mut Vec<ServerResult>) {
        let mut new_results = Vec::new();
        if let Some(ref mut client) = self.push_client {
            println!("Publish accepted for push stream key {}", client.push_target_stream);
            client.state = PushState::Pushing;

            // Send out any metadata or header information if we have any
            if let Some(ref channel) = self.channels.get(&client.push_source_stream) {
                if let Some(ref metadata) = channel.metadata {
                    let result = client.session.as_mut().unwrap().publish_metadata(&metadata).unwrap();
                    new_results.push(result);
                }

                if let Some(ref bytes) = channel.video_sequence_header {
                    let result = client.session.as_mut().unwrap().publish_video_data(bytes.clone(), RtmpTimestamp::new(0), false)
                        .unwrap();

                    new_results.push(result);
                }

                if let Some(ref bytes) = channel.audio_sequence_header {
                    let result = client.session.as_mut().unwrap().publish_audio_data(bytes.clone(), RtmpTimestamp::new(0), false)
                        .unwrap();

                    new_results.push(result);
                }
            }
        }

        if !new_results.is_empty() {
            self.handle_push_session_results(new_results, server_results);
        }
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
