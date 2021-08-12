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
        request_id: u32,
    },

    RequestDenied {
        request_id: u32,
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

#[derive(Debug)]
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

struct PublishDetails {
    video_sequence_header: Option<Bytes>,
    audio_sequence_header: Option<Bytes>,
    metadata: Option<StreamMetadata>,
    connection_id: i32,
}

pub fn start() -> mpsc::UnboundedSender<StreamManagerMessage> {
    let (sender, receiver) = mpsc::unbounded_channel();

    spawn(run(receiver));

    sender
}

async fn run(mut receiver: UnboundedReceiver<StreamManagerMessage>) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut players_by_key = HashMap::new();
    let mut publish_details: HashMap<String, PublishDetails> = HashMap::new();
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
                match publish_details.get(&key) {
                    None => (),
                    Some(details) => {
                        println!("Publish request by connection {} for stream '{}' rejected as it's already being published by connection {}",
                            connection_id, key, details.connection_id);

                        sender.send(ConnectionMessage::RequestDenied {request_id})?;
                        continue;
                    }
                }

                key_by_connection_id.insert(connection_id, key.clone());
                publish_details.insert(key, PublishDetails {
                    video_sequence_header: None,
                    audio_sequence_header: None,
                    metadata: None,
                    connection_id,
                });

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
                key_by_connection_id.insert(connection_id, key.clone());

                sender.send(ConnectionMessage::RequestAccepted {request_id})?;

                // If someone is publishing on this stream already, send the latest audio and video
                // sequence headers, so the client can view them.

                let details = match publish_details.get(&key) {
                    Some(x) => x,
                    None => continue,
                };

                if let Some(metadata) = &details.metadata {
                    sender.send(ConnectionMessage::NewMetadata {
                        metadata: metadata.clone(),
                    })?;
                }

                if let Some(data) = &details.video_sequence_header {
                    sender.send(ConnectionMessage::NewVideoData {
                        timestamp: RtmpTimestamp::new(0),
                        data: data.clone(),
                    })?;
                }

                if let Some(data) = &details.audio_sequence_header {
                    sender.send(ConnectionMessage::NewAudioData {
                        timestamp: RtmpTimestamp::new(0),
                        data: data.clone(),
                    })?;
                }
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

                publish_details.remove(key);
                key_by_connection_id.remove(&connection_id);
            },

            StreamManagerMessage::NewAudioData {sending_connection_id, timestamp, data} => {
                let key = match key_by_connection_id.get(&sending_connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let mut details = match publish_details.get_mut(key) {
                    Some(x) => x,
                    None => continue,
                };

                if is_audio_sequence_header(&data) {
                    details.audio_sequence_header = Some(data.clone());
                }

                if let Some(players) = players_by_key.get(key.as_str()) {
                    for player_id in players {
                        let sender = match sender_by_connection_id.get_mut(player_id) {
                            Some(x) => x,
                            None => continue,
                        };

                        sender.send(ConnectionMessage::NewAudioData {timestamp, data: data.clone()})?;
                    }
                }
            },

            StreamManagerMessage::NewVideoData {sending_connection_id, timestamp, data} => {
                let key = match key_by_connection_id.get(&sending_connection_id) {
                    Some(x) => x,
                    None => continue,
                };

                let mut details = match publish_details.get_mut(key) {
                    Some(x) => x,
                    None => continue,
                };

                if is_video_sequence_header(&data) {
                    details.video_sequence_header = Some(data.clone());
                }

                let players = match players_by_key.get(key.as_str()) {
                    Some(x) => x,
                    None => continue,
                };

                if let Some(players) = players_by_key.get(key.as_str()) {
                    for player_id in players {
                        let sender = match sender_by_connection_id.get_mut(player_id) {
                            Some(x) => x,
                            None => continue,
                        };

                        sender.send(ConnectionMessage::NewVideoData {timestamp, data: data.clone()})?;
                    }
                }
            },

            StreamManagerMessage::UpdatedStreamMetadata {sending_connection_id, metadata} => {
                let key = match key_by_connection_id.get(&sending_connection_id) {
                    Some(x) => {
                        println!("Test");
                        x
                    },
                    None => {
                        println!("test2");
                        continue
                    },
                };

                let mut details = match publish_details.get_mut(key) {
                    Some(x) => x,
                    None => continue,
                };

                details.metadata = Some(metadata.clone());

                if let Some(players) = players_by_key.get(key.as_str()) {
                    for player_id in players {
                        let sender = match sender_by_connection_id.get_mut(player_id) {
                            Some(x) => x,
                            None => continue,
                        };

                        sender.send(ConnectionMessage::NewMetadata { metadata: metadata.clone() })?;
                    }
                }
            },
        }
    }

    Ok(())
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
