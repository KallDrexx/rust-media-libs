use std::collections::HashMap;
use slab::Slab;
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent};
use rml_rtmp::chunk_io::Packet;

struct Client {
    session: ServerSession,
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
}

impl Server {
    pub fn new() -> Server {
        Server {
            clients: Slab::with_capacity(1024),
            connection_to_client_map: HashMap::with_capacity(1024),
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
            let client = Client {session};
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

        let accept_result;
        {
            let client_id = self.connection_to_client_map.get(&requested_connection_id).unwrap();
            let client = self.clients.get_mut(*client_id).unwrap();
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
}
