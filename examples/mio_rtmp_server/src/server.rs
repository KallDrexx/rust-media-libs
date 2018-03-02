use std::collections::HashMap;
use slab::Slab;
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult};
use rml_rtmp::chunk_io::Packet;

struct Client {
    connection_id: usize,
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

            handle_session_results(connection_id, initial_session_results, &mut server_results);
            let client = Client {connection_id, session};
            let client_id = Some(self.clients.insert(client));
            self.connection_to_client_map.insert(connection_id, client_id.unwrap());
        }

        let client_id = self.connection_to_client_map.get(&connection_id).unwrap();
        let client = self.clients.get_mut(*client_id).unwrap();

        let client_results = match client.session.handle_input(bytes) {
            Ok(results) => results,
            Err(error) => return Err(error.to_string()),
        };

        handle_session_results(connection_id, client_results, &mut server_results);
        Ok(server_results)
    }
}

fn handle_session_results(executed_connection_id: usize,
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

            x => println!("Server result received: {:?}", x),
        }
    }
}