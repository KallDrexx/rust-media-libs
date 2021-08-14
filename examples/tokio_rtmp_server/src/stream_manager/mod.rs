mod connection_message;
mod stream_manager_message;
mod publish_details;
mod player_details;

use tokio::sync::mpsc;
use rml_rtmp::time::RtmpTimestamp;
use rml_rtmp::sessions::StreamMetadata;
use bytes::Bytes;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use std::collections::hash_map::HashMap;
use crate::send;

pub use connection_message::ConnectionMessage;
pub use stream_manager_message::StreamManagerMessage;
pub use publish_details::PublishDetails;
pub use player_details::PlayerDetails;

pub fn start() -> mpsc::UnboundedSender<StreamManagerMessage> {
    let (sender, receiver) = mpsc::unbounded_channel();

    let manager = StreamManager::new();
    tokio::spawn(manager.run(receiver));

    sender
}

struct StreamManager {
    players_by_key: HashMap<String, HashMap<i32, PlayerDetails>>,
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

    async fn run(mut self, mut receiver: UnboundedReceiver<StreamManagerMessage>) {
        while let Some(message) = receiver.recv().await {
            match message {
                StreamManagerMessage::NewConnection {connection_id, sender} => {
                    self.handle_new_connection(connection_id, sender);
                },

                StreamManagerMessage::PublishRequest {connection_id, request_id, rtmp_app, stream_key} => {
                   self.handle_publish_request(connection_id, request_id, rtmp_app, stream_key);
                },

                StreamManagerMessage::PlaybackRequest {connection_id, request_id, stream_key, rtmp_app} => {
                    self.handle_playback_request(connection_id, request_id, rtmp_app, stream_key);
                },

                StreamManagerMessage::PlaybackFinished {connection_id} => {
                    self.handle_playback_finished(connection_id);
                },

                StreamManagerMessage::PublishFinished {connection_id} => {
                    self.handle_publish_finished(connection_id);
                },

                StreamManagerMessage::NewAudioData {sending_connection_id, timestamp, data} => {
                    self.handle_new_audio_data(sending_connection_id, timestamp, data);
                },

                StreamManagerMessage::NewVideoData {sending_connection_id, timestamp, data} => {
                    self.handle_new_video_data(sending_connection_id, timestamp, data);
                },

                StreamManagerMessage::UpdatedStreamMetadata {sending_connection_id, metadata} => {
                    self.handle_new_metadata(sending_connection_id, metadata);
                },
            }
        }
    }

    fn handle_new_connection(&mut self, connection_id: i32,
                             sender: UnboundedSender<ConnectionMessage>) {
        self.sender_by_connection_id.insert(connection_id, sender);
    }

    fn handle_publish_request(&mut self, connection_id: i32,
                              request_id: u32,
                              rtmp_app: String,
                              stream_key: String) {
        let sender = match self.sender_by_connection_id.get(&connection_id) {
            None => {
                println!("Publish request received by connection {} but that connection hasn't registered", connection_id);
                return;
            },

            Some(x) => x,
        };

        if self.key_by_connection_id.contains_key(&connection_id) {
            println!("Connection {} is requesting to publish, but its already being tracked", connection_id);
            if !send(&sender, ConnectionMessage::RequestDenied {request_id}) {
                self.cleanup_connection(connection_id);
            }

            return;
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

                return;
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
    }

    fn handle_playback_request(&mut self,
                               connection_id: i32,
                               request_id: u32,
                               rtmp_app: String,
                               stream_key: String) {
        let sender = match self.sender_by_connection_id.get(&connection_id) {
            None => {
                println!("Playback request received by connection {} but that connection hasn't registered", connection_id);
                return;
            },

            Some(x) => x,
        };

        if self.key_by_connection_id.contains_key(&connection_id) {
            println!("Playback requested by connection {} but its already being tracked", connection_id);
            if !send(&sender, ConnectionMessage::RequestDenied {request_id}) {
                self.cleanup_connection(connection_id);
            }

            return;
        }

        let key = format!("{}/{}", rtmp_app, stream_key);
        let connection_ids = self.players_by_key.entry(key.clone()).or_insert(HashMap::new());
        connection_ids.insert(connection_id, PlayerDetails::new(connection_id));
        self.key_by_connection_id.insert(connection_id, key.clone());

        if !send(&sender, ConnectionMessage::RequestAccepted {request_id}) {
            self.cleanup_connection(connection_id);

            return;
        }

        // If someone is publishing on this stream already, send the latest audio and video
        // sequence headers, so the client can view them.

        let details = match self.publish_details.get(&key) {
            Some(x) => x,
            None => return,
        };

        if let Some(metadata) = &details.metadata {
            let message = ConnectionMessage::NewMetadata {
                metadata: metadata.clone(),
            };

            if !send(&sender, message) {
                self.cleanup_connection(connection_id);
                return;
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
                return;
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
                return;
            }
        }
    }

    fn handle_playback_finished(&mut self, connection_id: i32) {
        self.cleanup_connection(connection_id);
    }

    fn handle_publish_finished(&mut self, connection_id: i32) {
        self.cleanup_connection(connection_id);
    }

    fn handle_new_audio_data(&mut self,
                             sending_connection_id: i32,
                             timestamp: RtmpTimestamp,
                             data: Bytes) {
        let key = match self.key_by_connection_id.get(&sending_connection_id) {
            Some(x) => x,
            None => return,
        };

        let mut details = match self.publish_details.get_mut(key) {
            Some(x) => x,
            None => return,
        };

        if is_audio_sequence_header(&data) {
            details.audio_sequence_header = Some(data.clone());
        }

        if let Some(players) = self.players_by_key.get(key.as_str()) {
            for (player_id, _) in players {
                let sender = match self.sender_by_connection_id.get_mut(player_id) {
                    Some(x) => x,
                    None => return,
                };

                let message = ConnectionMessage::NewAudioData {
                    timestamp,
                    data: data.clone(),
                    can_be_dropped: true
                };

                send(&sender, message);
            }
        }
    }

    fn handle_new_video_data(&mut self,
                             sending_connection_id: i32,
                             timestamp: RtmpTimestamp,
                             data: Bytes) {
        let key = match self.key_by_connection_id.get(&sending_connection_id) {
            Some(x) => x,
            None => return,
        };

        let mut details = match self.publish_details.get_mut(key) {
            Some(x) => x,
            None => return,
        };

        let mut can_be_dropped = true;
        let mut is_key_frame = false;
        if is_video_sequence_header(&data) {
            details.video_sequence_header = Some(data.clone());
            can_be_dropped = false;
        }
        else if is_video_keyframe(&data) {
            can_be_dropped = false;
            is_key_frame = true;
        }

        if let Some(players) = self.players_by_key.get_mut(key.as_str()) {
            for (player_id, mut details) in players {
                let sender = match self.sender_by_connection_id.get_mut(player_id) {
                    Some(x) => x,
                    None => return,
                };

                // Only send this video frame if it's a required video frame, or the player has
                // already received at least one key frame.  If a player joins mid-stream, then
                // there's no point in sending them video frames without an initial keyframe.
                if can_be_dropped && !details.has_received_video_keyframe {
                    continue;
                }

                if is_key_frame {
                    details.has_received_video_keyframe = true;
                }

                let message = ConnectionMessage::NewVideoData {
                    timestamp,
                    data: data.clone(),
                    can_be_dropped,
                };

                send(&sender, message);
            }
        }
    }

    fn handle_new_metadata(&mut self, sending_connection_id: i32, metadata: StreamMetadata) {
        let key = match self.key_by_connection_id.get(&sending_connection_id) {
            Some(x) => x,
            None => return,
        };

        let mut details = match self.publish_details.get_mut(key) {
            Some(x) => x,
            None => return,
        };

        details.metadata = Some(metadata.clone());

        if let Some(players) = self.players_by_key.get(key.as_str()) {
            for (player_id, _) in players {
                let sender = match self.sender_by_connection_id.get_mut(player_id) {
                    Some(x) => x,
                    None => return,
                };

                let message = ConnectionMessage::NewMetadata { metadata: metadata.clone() };
                send(&sender, message);
            }
        }
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
