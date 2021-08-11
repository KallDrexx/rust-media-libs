use tokio::sync::mpsc;
use rml_rtmp::sessions::StreamMetadata;
use rml_rtmp::time::RtmpTimestamp;
use bytes::Bytes;
use tokio::sync::mpsc::UnboundedReceiver;
use std::error::Error;
use std::collections::hash_map::HashMap;
use std::collections::hash_set::HashSet;
use crate::spawn;

#[derive(Debug)]
pub enum ConnectionMessage {
    RequestAccepted {
        request_id: i32,
    },

    RequestDenied {
        request_id: i32,
    },

    NewVideoData {
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    NewAudioData {
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    NewMetadata {
        metadata: StreamMetadata,
    },
}

pub enum StreamManagerMessage {
    NewConnection {
        connection_id: i32,
        sender: mpsc::UnboundedSender<ConnectionMessage>,
    },

    PublishRequest {
        connection_id: i32,
        rtmp_app: String,
        stream_key: String,
        request_id: u32,
    },

    PlaybackRequest {
        connection_id: i32,
        rtmp_app: String,
        stream_key: String,
        request_id: u32,
    },

    UpdatedStreamMetadata {
        sending_connection_id: i32,
        metadata: StreamMetadata,
    },

    NewVideoData {
        sending_connection_id: i32,
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    NewAudioData {
        sending_connection_id: i32,
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    PublishFinished {
        connection_id: i32,
    },

    PlaybackFinished {
        connection_id: i32,
    }
}

pub fn start() -> mpsc::UnboundedSender<StreamManagerMessage> {
    let (sender, receiver) = mpsc::unbounded_channel();

    spawn(run(receiver));

    sender
}

async fn run(mut receiver: UnboundedReceiver<StreamManagerMessage>) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut players_by_key = HashMap::new();
    let mut publisher_by_key = HashMap::new();
    let mut sender_by_connection_id = HashMap::new();
    let mut key_by_connection_id = HashMap::new();

    while let Some(message) = receiver.recv().await {
        match message {
            StreamManagerMessage::NewConnection {connection_id, sender} => {
                sender_by_connection_id.insert(connection_id, sender);
            },

            StreamManagerMessage::PublishRequest {connection_id, request_id, rtmp_app, stream_key} => {
                let sender = match sender_by_connection_id.get(&connection_id) {
                    None => {
                        println!("Publish request received by connection {} but that connection hasn't registered", connection_id);
                        continue;
                    },

                    Some(x) => x,
                };

                if key_by_connection_id.contains_key(&connection_id) {
                    println!("Connection {} is requesting to publish, but its already being tracked", connection_id);
                    sender.send(ConnectionMessage::RequestDenied {request_id})?;
                    continue;
                }

                let key = format!("{}/{}", rtmp_app, stream_key);
                match publisher_by_key.get(&key) {
                    None => (),
                    Some(publishing_connection_id) => {
                        println!("Publish request by connection {} for stream '{}' rejected as it's already being published by connection {}",
                            connection_id, key, publishing_connection_id);

                        sender.send(ConnectionMessage::RequestDenied {request_id})?;
                        continue;
                    }
                }

                publisher_by_key.insert(key, connection_id);
                sender.send(ConnectionMessage::RequestAccepted {request_id})?;
            },

            StreamManagerMessage::PlaybackRequest {connection_id, request_id, stream_key, rtmp_app} => {
                let sender = match sender_by_connection_id.get(&connection_id) {
                    None => {
                        println!("Playback request received by connection {} but that connection hasn't registered", connection_id);
                        continue;
                    },

                    Some(x) => x,
                };

                if key_by_connection_id.contains_key(&connection_id) {
                    println!("Playback requested by connection {} but its already being tracked", connection_id);
                    sender.send(ConnectionMessage::RequestDenied {request_id})?;
                    continue;
                }

                let key = format!("{}/{}", rtmp_app, stream_key);
                let connection_ids = players_by_key.entry(key.clone()).or_insert(HashSet::new());
                connection_ids.insert(connection_id);
                key_by_connection_id.insert(connection_id, key);

                sender.send(ConnectionMessage::RequestAccepted {request_id})?;
            },

            StreamManagerMessage::PlaybackFinished {connection_id} => {
                let key = match key_by_connection_id.get(&connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let connections = match players_by_key.get_mut(key.as_str()) {
                    Some(x) => x,
                    None => continue,
                };

                connections.remove(&connection_id);
                key_by_connection_id.remove(&connection_id);
            },

            StreamManagerMessage::PublishFinished {connection_id} => {
                let key = match key_by_connection_id.get(&connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let connections = match publisher_by_key.get_mut(key.as_str()) {
                    Some(x) => x,
                    None => continue,
                };
            },

            StreamManagerMessage::NewAudioData {sending_connection_id, timestamp, data} => {
                let key = match key_by_connection_id.get(&sending_connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let players = match players_by_key.get(key.as_str()) {
                    Some(x) => x,
                    None => continue,
                };

                for player_id in players {
                    let sender = match sender_by_connection_id.get_mut(player_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    sender.send(ConnectionMessage::NewAudioData {timestamp, data: data.clone()})?;
                }
            },

            StreamManagerMessage::NewVideoData {sending_connection_id, timestamp, data} => {
                let key = match key_by_connection_id.get(&sending_connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let players = match players_by_key.get(key.as_str()) {
                    Some(x) => x,
                    None => continue,
                };

                for player_id in players {
                    let sender = match sender_by_connection_id.get_mut(player_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    sender.send(ConnectionMessage::NewVideoData {timestamp, data: data.clone()})?;
                }
            },

            StreamManagerMessage::UpdatedStreamMetadata {sending_connection_id, metadata} => {
                let key = match key_by_connection_id.get(&sending_connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let players = match players_by_key.get(key.as_str()) {
                    Some(x) => x,
                    None => continue,
                };

                for player_id in players {
                    let sender = match sender_by_connection_id.get_mut(player_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    sender.send(ConnectionMessage::NewMetadata{metadata: metadata.clone()})?;
                }
            },
        }
    }

    Ok(())
}