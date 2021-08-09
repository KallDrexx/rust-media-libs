use tokio::sync::mpsc;
use rml_rtmp::sessions::StreamMetadata;
use rml_rtmp::time::RtmpTimestamp;
use bytes::Bytes;
use tokio::sync::mpsc::UnboundedReceiver;
use std::error::Error;
use std::collections::hash_map::HashMap;
use crate::spawn;

pub enum ConnectionMessage {
    RequestAccepted {
        request_id: i32,
    },

    RequestDenied {
        request_id: i32,
    },

    IncomingVideoData {
        timestamp: RtmpTimestamp,
        data: Bytes,
    },

    IncomingAudioData {
        timestamp: RtmpTimestamp,
        data: Bytes,
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
        request_id: i32,
    },

    PlaybackRequest {
        connection_id: i32,
        rtmp_app: String,
        stream_key: String,
        request_id: i32,
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

    while let Some(message) = receiver.recv().await? {
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

            StreamManagerMessage::PlaybackRequest {connection_id, request_id, stream_key, rtmp_app} {
                let sender = match sender_by_connection_id.get(&connection_id) {
                    None => {
                        println!("Playback request received by connection {} but that connection hasn't registered", connection_id);
                        continue;
                    },

                    Some(x) => x,
                };

                players_by_key.entry()
            }
        }
    }

    Ok(())
}