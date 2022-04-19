extern crate bytes;
extern crate rml_amf0;
extern crate rml_rtmp;
extern crate hdrhistogram;

use bytes::Bytes;
use std::collections::HashMap;
use std::time::{Instant, SystemTime};
use hdrhistogram::Histogram;

use rml_amf0::Amf0Value;
use rml_rtmp::chunk_io::ChunkSerializer;
use rml_rtmp::messages::{MessagePayload, RtmpMessage};
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
};
use rml_rtmp::time::RtmpTimestamp;

const ITERATION_COUNT: u32 = 50_000;
const PLAYER_COUNT: u32 = 100;
static APP_NAME: &'static str = "live";
static STREAM_KEY: &'static str = "stream_key";

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let iteration_count = if args.len() >= 2 {
        args[1].parse::<u32>().unwrap()
    } else {
        ITERATION_COUNT
    };

    let (mut publisher, mut publisher_serializer) = create_publishing_session();
    let mut players = Vec::new();
    for _ in 0..PLAYER_COUNT {
        players.push(create_player_session())
    }

    println!("Running {} iterations with {} players", iteration_count, PLAYER_COUNT);

    let mut histogram = Histogram::<u64>::new(2).unwrap();

    let mut vector = Vec::new();
    vector.extend_from_slice(&[1_u8; 3000]);

    let bytes = Bytes::from(vector);
    let video_message = RtmpMessage::VideoData { data: bytes };
    let video_payload = video_message
        .into_message_payload(RtmpTimestamp::new(0), 1)
        .unwrap();
    let video_packet = publisher_serializer
        .serialize(&video_payload, true, true)
        .unwrap();

    let start = SystemTime::now();

    for _ in 0..iteration_count {
        let loop_start = Instant::now();
        let results = publisher.handle_input(&video_packet.bytes[..]).unwrap();

        for result in results {
            match result {
                ServerSessionResult::OutboundResponse(_) => (),
                ServerSessionResult::UnhandleableMessageReceived(_) => (),
                ServerSessionResult::RaisedEvent(event) => match event {
                    ServerSessionEvent::VideoDataReceived {
                        app_name: _,
                        stream_key: _,
                        data,
                        timestamp,
                    } => {
                        for player in &mut players {
                            player.
                                send_video_data(1, data.clone(), timestamp.clone(), true)
                                .unwrap();
                        }
                    }

                    _ => (),
                },
            }
        }

        let elapsed = loop_start.elapsed();
        histogram.record(elapsed.as_micros() as u64).unwrap();
    }

    let elapsed = start.elapsed().unwrap();
    let total_ns = elapsed.as_secs() * 1_000_000_000 + elapsed.subsec_nanos() as u64;
    let average_ns = total_ns / iteration_count as u64;

    println!("50th: {}, 75th: {}, 90th: {}, 99th: {} 99.9th: {}",
        histogram.value_at_percentile(50.0),
        histogram.value_at_percentile(75.0),
        histogram.value_at_percentile(90.0),
        histogram.value_at_percentile(99.0),
        histogram.value_at_percentile(99.9),
    );

    println!(
        "Took {}.{:09} seconds (avg {}ns)",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        average_ns
    );
}

fn create_publishing_session() -> (ServerSession, ChunkSerializer) {
    let mut serializer = ChunkSerializer::new();
    let config = ServerSessionConfig::new();
    let (mut session, _) = ServerSession::new(config).unwrap();

    perform_connection(APP_NAME, &mut session, &mut serializer);
    create_active_stream(&mut session, &mut serializer);
    start_publishing(&mut session, &mut serializer);

    (session, serializer)
}

fn create_player_session() -> ServerSession {
    let mut serializer = ChunkSerializer::new();
    let config = ServerSessionConfig::new();
    let (mut session, _) = ServerSession::new(config).unwrap();

    perform_connection(APP_NAME, &mut session, &mut serializer);
    create_active_stream(&mut session, &mut serializer);
    start_playing(&mut session, &mut serializer);

    session
}

fn perform_connection(
    app_name: &str,
    session: &mut ServerSession,
    serializer: &mut ChunkSerializer,
) {
    let connect_payload = create_connect_message(app_name.to_string(), 15, 0, 0.0);
    let connect_packet = serializer.serialize(&connect_payload, true, false).unwrap();
    let connect_results = session.handle_input(&connect_packet.bytes[..]).unwrap();

    for result in connect_results {
        match result {
            ServerSessionResult::OutboundResponse(_) => (),
            ServerSessionResult::UnhandleableMessageReceived(_) => (),
            ServerSessionResult::RaisedEvent(event) => match event {
                ServerSessionEvent::ConnectionRequested {
                    app_name: _,
                    request_id,
                } => {
                    session.accept_request(request_id).unwrap();
                }

                _ => (),
            },
        }
    }
}

fn create_connect_message(
    app_name: String,
    timestamp: u32,
    stream_id: u32,
    object_encoding: f64,
) -> MessagePayload {
    let mut properties = HashMap::new();
    properties.insert("app".to_string(), Amf0Value::Utf8String(app_name));
    properties.insert(
        "objectEncoding".to_string(),
        Amf0Value::Number(object_encoding),
    );

    let message = RtmpMessage::Amf0Command {
        command_name: "connect".to_string(),
        transaction_id: 1.0,
        command_object: Amf0Value::Object(properties),
        additional_arguments: vec![],
    };

    let timestamp = RtmpTimestamp::new(timestamp);
    let payload = message.into_message_payload(timestamp, stream_id).unwrap();
    payload
}

fn create_active_stream(session: &mut ServerSession, serializer: &mut ChunkSerializer) -> u32 {
    let message = RtmpMessage::Amf0Command {
        command_name: "createStream".to_string(),
        transaction_id: 4.0,
        command_object: Amf0Value::Null,
        additional_arguments: Vec::new(),
    };

    let payload = message
        .into_message_payload(RtmpTimestamp::new(0), 0)
        .unwrap();
    let packet = serializer.serialize(&payload, true, false).unwrap();
    let _ = session.handle_input(&packet.bytes[..]).unwrap();

    1
}

fn start_publishing(session: &mut ServerSession, serializer: &mut ChunkSerializer) {
    let message = RtmpMessage::Amf0Command {
        command_name: "publish".to_string(),
        transaction_id: 5.0,
        command_object: Amf0Value::Null,
        additional_arguments: vec![
            Amf0Value::Utf8String(STREAM_KEY.to_string()),
            Amf0Value::Utf8String("live".to_string()),
        ],
    };

    let publish_payload = message
        .into_message_payload(RtmpTimestamp::new(0), 1)
        .unwrap();
    let publish_packet = serializer
        .serialize(&publish_payload, false, false)
        .unwrap();
    let publish_results = session.handle_input(&publish_packet.bytes[..]).unwrap();

    for result in publish_results {
        match result {
            ServerSessionResult::OutboundResponse(_) => (),
            ServerSessionResult::UnhandleableMessageReceived(_) => (),
            ServerSessionResult::RaisedEvent(event) => match event {
                ServerSessionEvent::PublishStreamRequested {
                    app_name: _,
                    stream_key: _,
                    mode: _,
                    request_id,
                } => {
                    session.accept_request(request_id).unwrap();
                }

                _ => (),
            },
        }
    }
}

fn start_playing(session: &mut ServerSession, serializer: &mut ChunkSerializer) {
    let message = RtmpMessage::Amf0Command {
        command_name: "play".to_string(),
        transaction_id: 4.0,
        command_object: Amf0Value::Null,
        additional_arguments: vec![
            Amf0Value::Utf8String(STREAM_KEY.to_string()),
            Amf0Value::Number(5.0),   // Start argument
            Amf0Value::Number(25.0),  // Duration,
            Amf0Value::Boolean(true), // reset
        ],
    };

    let play_payload = message
        .into_message_payload(RtmpTimestamp::new(0), 1)
        .unwrap();
    let play_packet = serializer.serialize(&play_payload, false, false).unwrap();
    let play_results = session.handle_input(&play_packet.bytes[..]).unwrap();

    for result in play_results {
        match result {
            ServerSessionResult::OutboundResponse(_) => (),
            ServerSessionResult::UnhandleableMessageReceived(_) => (),
            ServerSessionResult::RaisedEvent(event) => match event {
                ServerSessionEvent::PlayStreamRequested {
                    app_name: _,
                    stream_key: _,
                    request_id,
                    start_at: _,
                    duration: _,
                    reset: _,
                    stream_id: _,
                } => {
                    session.accept_request(request_id).unwrap();
                }

                _ => (),
            },
        }
    }
}
