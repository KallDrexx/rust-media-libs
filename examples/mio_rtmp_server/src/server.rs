use std::collections::HashMap;
use slab::Slab;
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent};
use rml_rtmp::chunk_io::Packet;

enum ClientAction {
    Waiting,
    Publishing(String), // Publishing to a stream key
    //Watching(String), // Watching a stream key
}

struct Client {
    session: ServerSession,
    current_action: ClientAction,
}

struct MediaChannel {
    publishing_client_id: Option<usize>,
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
                current_action: ClientAction::Waiting,
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
                    //ClientAction::Watching(_) => (),
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

            ServerSessionEvent::VideoDataReceived {app_name: _, stream_key: _, data: _, timestamp: _} => {

            },

            ServerSessionEvent::AudioDataReceived {app_name: _, stream_key: _, data: _, timestamp: _} => {

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
            let channel = MediaChannel {publishing_client_id: Some(*client_id)};

            client.current_action = ClientAction::Publishing(stream_key.clone());
            self.channels.insert(stream_key, channel);

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

    fn publishing_ended(&mut self, stream_key: String) {
        let channel = match self.channels.get_mut(&stream_key) {
            Some(channel) => channel,
            None => return,
        };

        channel.publishing_client_id = None;
    }
}
