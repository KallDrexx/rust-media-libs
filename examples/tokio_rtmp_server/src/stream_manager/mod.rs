mod connection_message;
mod stream_manager_message;
mod publish_details;

use tokio::sync::mpsc;
use rml_rtmp::time::RtmpTimestamp;
use bytes::Bytes;
use tokio::sync::mpsc::UnboundedReceiver;
use std::error::Error;
use std::collections::hash_map::HashMap;
use std::collections::hash_set::HashSet;
use crate::{spawn, send};

pub use connection_message::ConnectionMessage;
pub use stream_manager_message::StreamManagerMessage;
pub use publish_details::PublishDetails;

pub fn start() -> mpsc::UnboundedSender<StreamManagerMessage> {
    let (sender, receiver) = mpsc::unbounded_channel();

    let manager = StreamManager::new();
    spawn(manager.run(receiver));

    sender
}

struct StreamManager {
    players_by_key: HashMap<String, HashSet<i32>>,
    publish_details: HashMap<String, PublishDetails>,
    sender_by_connection_id: HashMap<i32, mpsc::UnboundedSender<ConnectionMessage>>,
    key_by_connection_id: HashMap<i32, String>,
}

impl StreamManager {
    fn new() -> Self {
        StreamManager {
            publish_details: HashMap::new(),
            players_by_key: HashMap::new(),
            sender_by_connection_id: HashMap::new(),
            key_by_connection_id: HashMap::new(),
        }
    }

    fn cleanup_connection(&mut self, connection_id: i32) {
        println!("Stream manager is removing connection id {}", connection_id);

        self.sender_by_connection_id.remove(&connection_id);
        if let Some(key) = self.key_by_connection_id.remove(&connection_id) {
            if let Some(players) = self.players_by_key.get_mut(&key) {
                players.remove(&connection_id);
            }

            if let Some(details) = self.publish_details.get_mut(&key) {
                if details.connection_id == connection_id {
                    self.publish_details.remove(&key);
                }
            }
        }
    }

    async fn run(mut self, mut receiver: UnboundedReceiver<StreamManagerMessage>) -> Result<(), Box<dyn Error + Sync + Send>> {
        while let Some(message) = receiver.recv().await {
            match message {
                StreamManagerMessage::NewConnection {connection_id, sender} => {
                    self.sender_by_connection_id.insert(connection_id, sender);
                },

                StreamManagerMessage::PublishRequest {connection_id, request_id, rtmp_app, stream_key} => {
                    let sender = match self.sender_by_connection_id.get(&connection_id) {
                        None => {
                            println!("Publish request received by connection {} but that connection hasn't registered", connection_id);
                            continue;
                        },

                        Some(x) => x,
                    };

                    if self.key_by_connection_id.contains_key(&connection_id) {
                        println!("Connection {} is requesting to publish, but its already being tracked", connection_id);
                        if !send(&sender, ConnectionMessage::RequestDenied {request_id}) {
                            self.cleanup_connection(connection_id);
                        }

                        continue;
                    }

                    let key = format!("{}/{}", rtmp_app, stream_key);
                    match self.publish_details.get(&key) {
                        None => (),
                        Some(details) => {
                            println!("Publish request by connection {} for stream '{}' rejected as it's already being published by connection {}",
                                     connection_id, key, details.connection_id);

                            if !send(&sender, ConnectionMessage::RequestDenied {request_id}) {
                                self.cleanup_connection(connection_id);
                            }

                            continue;
                        }
                    }

                    self.key_by_connection_id.insert(connection_id, key.clone());
                    self.publish_details.insert(key.clone(), PublishDetails {
                        video_sequence_header: None,
                        audio_sequence_header: None,
                        metadata: None,
                        connection_id,
                    });

                    if !send(&sender, ConnectionMessage::RequestAccepted {request_id}) {
                        self.cleanup_connection(connection_id);
                    }
                },

                StreamManagerMessage::PlaybackRequest {connection_id, request_id, stream_key, rtmp_app} => {
                    let sender = match self.sender_by_connection_id.get(&connection_id) {
                        None => {
                            println!("Playback request received by connection {} but that connection hasn't registered", connection_id);
                            continue;
                        },

                        Some(x) => x,
                    };

                    if self.key_by_connection_id.contains_key(&connection_id) {
                        println!("Playback requested by connection {} but its already being tracked", connection_id);
                        if !send(&sender, ConnectionMessage::RequestDenied {request_id}) {
                            self.cleanup_connection(connection_id);
                        }

                        continue;
                    }

                    let key = format!("{}/{}", rtmp_app, stream_key);
                    let connection_ids = self.players_by_key.entry(key.clone()).or_insert(HashSet::new());
                    connection_ids.insert(connection_id);
                    self.key_by_connection_id.insert(connection_id, key.clone());

                    if !send(&sender, ConnectionMessage::RequestAccepted {request_id}) {
                        self.cleanup_connection(connection_id);

                        continue;
                    }

                    // If someone is publishing on this stream already, send the latest audio and video
                    // sequence headers, so the client can view them.

                    let details = match self.publish_details.get(&key) {
                        Some(x) => x,
                        None => continue,
                    };

                    if let Some(metadata) = &details.metadata {
                        let message = ConnectionMessage::NewMetadata {
                            metadata: metadata.clone(),
                        };

                        if !send(&sender, message) {
                            self.cleanup_connection(connection_id);
                            continue;
                        }
                    }

                    if let Some(data) = &details.video_sequence_header {
                        let message = ConnectionMessage::NewVideoData {
                            timestamp: RtmpTimestamp::new(0),
                            data: data.clone(),
                            can_be_dropped: false,
                        };

                        if !send(&sender, message) {
                            self.cleanup_connection(connection_id);
                            continue;
                        }
                    }

                    if let Some(data) = &details.audio_sequence_header {
                        let message = ConnectionMessage::NewAudioData {
                            timestamp: RtmpTimestamp::new(0),
                            data: data.clone(),
                            can_be_dropped: false,
                        };

                        if !send(&sender, message) {
                            self.cleanup_connection(connection_id);
                            continue;
                        }
                    }
                },

                StreamManagerMessage::PlaybackFinished {connection_id} => {
                    let key = match self.key_by_connection_id.get(&connection_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    let connections = match self.players_by_key.get_mut(key.as_str()) {
                        Some(x) => x,
                        None => continue,
                    };

                    connections.remove(&connection_id);
                    self.key_by_connection_id.remove(&connection_id);
                },

                StreamManagerMessage::PublishFinished {connection_id} => {
                    let key = match self.key_by_connection_id.get(&connection_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    self.publish_details.remove(key);
                    self.key_by_connection_id.remove(&connection_id);
                },

                StreamManagerMessage::NewAudioData {sending_connection_id, timestamp, data} => {
                    let key = match self.key_by_connection_id.get(&sending_connection_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    let mut details = match self.publish_details.get_mut(key) {
                        Some(x) => x,
                        None => continue,
                    };

                    if is_audio_sequence_header(&data) {
                        details.audio_sequence_header = Some(data.clone());
                    }

                    if let Some(players) = self.players_by_key.get(key.as_str()) {
                        for player_id in players {
                            let sender = match self.sender_by_connection_id.get_mut(player_id) {
                                Some(x) => x,
                                None => continue,
                            };

                            let message = ConnectionMessage::NewAudioData {
                                timestamp,
                                data: data.clone(),
                                can_be_dropped: true
                            };

                            send(&sender, message);

                        }
                    }
                },

                StreamManagerMessage::NewVideoData {sending_connection_id, timestamp, data} => {
                    let key = match self.key_by_connection_id.get(&sending_connection_id) {
                        Some(x) => x,
                        None => continue,
                    };

                    let mut details = match self.publish_details.get_mut(key) {
                        Some(x) => x,
                        None => continue,
                    };

                    let mut can_be_dropped = true;
                    if is_video_sequence_header(&data) {
                        details.video_sequence_header = Some(data.clone());
                        can_be_dropped = false;
                    }
                    else if is_video_keyframe(&data) {
                        can_be_dropped = false;
                    }

                    if let Some(players) = self.players_by_key.get(key.as_str()) {
                        for player_id in players {
                            let sender = match self.sender_by_connection_id.get_mut(player_id) {
                                Some(x) => x,
                                None => continue,
                            };

                            let message = ConnectionMessage::NewVideoData {
                                timestamp,
                                data: data.clone(),
                                can_be_dropped,
                            };

                            send(&sender, message);
                        }
                    }
                },

                StreamManagerMessage::UpdatedStreamMetadata {sending_connection_id, metadata} => {
                    let key = match self.key_by_connection_id.get(&sending_connection_id) {
                        Some(x) => {
                            println!("Test");
                            x
                        },
                        None => {
                            println!("test2");
                            continue
                        },
                    };

                    let mut details = match self.publish_details.get_mut(key) {
                        Some(x) => x,
                        None => continue,
                    };

                    details.metadata = Some(metadata.clone());

                    if let Some(players) = self.players_by_key.get(key.as_str()) {
                        for player_id in players {
                            let sender = match self.sender_by_connection_id.get_mut(player_id) {
                                Some(x) => x,
                                None => continue,
                            };

                            let message = ConnectionMessage::NewMetadata { metadata: metadata.clone() };
                            send(&sender, message);
                        }
                    }
                },
            }
        }

        Ok(())
    }
}



fn is_video_sequence_header(data: &Bytes) -> bool {
    // This is assuming h264.
    return data.len() >= 2 &&
        data[0] == 0x17 &&
        data[1] == 0x00;
}

fn is_audio_sequence_header(data: &Bytes) -> bool {
    // This is assuming aac
    return data.len() >= 2 &&
        data[0] == 0xaf &&
        data[1] == 0x00;
}

fn is_video_keyframe(data: &Bytes) -> bool {
    // assumings h264
    return data.len() >= 2 &&
        data[0] == 0x17 &&
        data[1] != 0x00; // 0x00 is the sequence header, don't count that for now
}
