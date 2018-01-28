use super::*;
use std::collections::HashMap;
use rml_amf0::Amf0Value;
use ::messages::{RtmpMessage, PeerBandwidthLimitType, UserControlEventType, MessagePayload};
use ::chunk_io::{ChunkDeserializer};

const DEFAULT_CHUNK_SIZE: u32 = 1111;
const DEFAULT_PEER_BANDWIDTH: u32 = 2222;
const DEFAULT_WINDOW_ACK_SIZE: u32 = 3333;

#[test]
fn new_config_creates_initial_responses() {
    let config = get_basic_config();
    let mut deserializer = ChunkDeserializer::new();
    let (_, results) = ServerSession::new(config).unwrap();

    let (responses, _) = split_results(&mut deserializer, results);

    assert_vec_contains!(responses, &RtmpMessage::WindowAcknowledgement {size: DEFAULT_WINDOW_ACK_SIZE});
    assert_vec_contains!(responses, &RtmpMessage::SetPeerBandwidth {size: DEFAULT_PEER_BANDWIDTH, limit_type: PeerBandwidthLimitType::Dynamic});
    assert_vec_contains!(responses, &RtmpMessage::UserControl {
            event_type: UserControlEventType::StreamBegin,
            stream_id: Some(0),
            buffer_length: None,
            timestamp: None
        });

    // Based on packet capture, not 100% sure if needed
    let mut additional_values: &Vec<Amf0Value> = &Vec::new();
    assert_vec_contains!(responses, &RtmpMessage::Amf0Command {
            command_name: ref command_name_value,
            transaction_id: transaction_id_value,
            command_object: Amf0Value::Null,
            additional_arguments: ref x,
        } if command_name_value == "onBWDone" && transaction_id_value == 0_f64 => additional_values = x);
    assert_eq!(&additional_values[..], &[Amf0Value::Number(8192_f64)], "onBWDone additional values were unexpected");
}

#[test]
fn can_accept_connection_request() {
    let config = get_basic_config();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, initial_results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, initial_results);

    let connect_payload = create_connect_message("some_app".to_string(), 15, 0, 0.0);
    let connect_packet = serializer.serialize(&connect_payload, true, false).unwrap();
    let connect_results = session.handle_input(&connect_packet.bytes[..]).unwrap();
    assert_eq!(connect_results.len(), 1, "Unexpected number of responses when handling connect request message");

    let (_, events) = split_results(&mut deserializer, connect_results);
    assert_eq!(events.len(), 1, "Unexpected number of events returned");
    let request_id = match events[0] {
        ServerSessionEvent::ConnectionRequested {ref app_name, request_id} if app_name == "some_app" => request_id,
        _ => panic!("First event was not as expected: {:?}", events[0]),
    };

    let accept_results = session.accept_request(request_id).unwrap();
    assert_eq!(accept_results.len(), 1, "Unexpected number of results returned");

    let (responses, _) = split_results(&mut deserializer, accept_results);
    match responses[0] {
        RtmpMessage::Amf0Command {
            ref command_name,
            transaction_id: _,
            command_object: Amf0Value::Object(ref properties),
            ref additional_arguments
        } if command_name == "_result" => {
            assert_eq!(properties.get("fmsVer"), Some(&Amf0Value::Utf8String(config.fms_version)), "Unexpected fms version");
            assert_eq!(properties.get("capabilities"), Some(&Amf0Value::Number(31.0)), "Unexpected capabilities value");
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments");
            match additional_arguments[0] {
                Amf0Value::Object(ref properties) => {
                    assert_eq!(properties.get("level"), Some(&Amf0Value::Utf8String("status".to_string())), "Unexpected level value");
                    assert_eq!(properties.get("code"), Some(&Amf0Value::Utf8String("NetConnection.Connect.Success".to_string())), "Unexpected code value");
                    assert_eq!(properties.get("objectEncoding"), Some(&Amf0Value::Number(0.0)), "Unexpected object encoding value");
                    assert!(properties.contains_key("description"), "No description provided");
                },

                _ => panic!("Additional arguments was not an Amf0 object: {:?}", additional_arguments[0]),
            }
        },

        _ => panic!("Unexpected first response message: {:?}", responses[0]),
    }
}

#[test]
fn connect_request_strips_trailing_slash() {
    let config = get_basic_config();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, initial_results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, initial_results);

    let connect_payload = create_connect_message("some_app/".to_string(), 15, 0, 0.0);
    let connect_packet = serializer.serialize(&connect_payload, true, false).unwrap();
    let connect_results = session.handle_input(&connect_packet.bytes[..]).unwrap();
    assert_eq!(connect_results.len(), 1, "Unexpected number of responses when handling connect request message");

    let (_, events) = split_results(&mut deserializer, connect_results);
    assert_eq!(events.len(), 1, "Unexpected number of events returned");
    match events[0] {
        ServerSessionEvent::ConnectionRequested {ref app_name, request_id: _} => assert_eq!(app_name, "some_app", "Unexpected app name"),
        _ => panic!("First event was not as expected: {:?}", events[0]),
    };

}

#[test]
fn accepted_connection_responds_with_same_object_encoding_value_as_connection_request() {
    let config = get_basic_config();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);

    let connect_payload = create_connect_message("some_app".to_string(), 15, 0, 3.0);
    let connect_packet = serializer.serialize(&connect_payload, true, false).unwrap();
    let connect_results = session.handle_input(&connect_packet.bytes[..]).unwrap();
    assert_eq!(connect_results.len(), 1, "Unexpected number of responses when handling connect request message");

    let (_, events) = split_results(&mut deserializer, connect_results);
    assert_eq!(events.len(), 1, "Unexpected number of events returned");
    let request_id = match events[0] {
        ServerSessionEvent::ConnectionRequested {ref app_name, request_id} if app_name == "some_app" => request_id,
        _ => panic!("First event was not as expected: {:?}", events[0]),
    };

    let accept_results = session.accept_request(request_id).unwrap();
    assert_eq!(accept_results.len(), 1, "Unexpected number of results returned");

    let (responses, _) = split_results(&mut deserializer, accept_results);
    match responses[0] {
        RtmpMessage::Amf0Command {
            ref command_name,
            transaction_id: _,
            command_object: Amf0Value::Object(ref properties),
            ref additional_arguments
        } if command_name == "_result" => {
            assert_eq!(properties.get("fmsVer"), Some(&Amf0Value::Utf8String(config.fms_version)), "Unexpected fms version");
            assert_eq!(properties.get("capabilities"), Some(&Amf0Value::Number(31.0)), "Unexpected capabilities value");
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments");
            match additional_arguments[0] {
                Amf0Value::Object(ref properties) => {
                    assert_eq!(properties.get("level"), Some(&Amf0Value::Utf8String("status".to_string())), "Unexpected level value");
                    assert_eq!(properties.get("code"), Some(&Amf0Value::Utf8String("NetConnection.Connect.Success".to_string())), "Unexpected code value");
                    assert_eq!(properties.get("objectEncoding"), Some(&Amf0Value::Number(3.0)), "Unexpected object encoding value");
                    assert!(properties.contains_key("description"), "No description provided");
                },

                _ => panic!("Additional arguments was not an Amf0 object: {:?}", additional_arguments[0]),
            }
        },

        _ => panic!("Unexpected first response message: {:?}", responses[0]),
    }
}

#[test]
fn can_create_stream_on_connected_session() {
    let config = get_basic_config();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection("some_app", &mut session, &mut serializer, &mut deserializer);

    let message = RtmpMessage::Amf0Command {
        command_name: "createStream".to_string(),
        transaction_id: 4.0,
        command_object: Amf0Value::Null,
        additional_arguments: Vec::new()
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(0), 0).unwrap();
    let packet = serializer.serialize(&payload, true, false).unwrap();
    let results = session.handle_input(&packet.bytes[..]).unwrap();
    let (responses, _) = split_results(&mut deserializer, results);

    assert_eq!(responses.len(), 1, "Unexpected number of responses returned");
    match responses[0] {
        RtmpMessage::Amf0Command {
            ref command_name,
            transaction_id,
            command_object: Amf0Value::Null,
            ref additional_arguments
        } if command_name == "_result" && transaction_id == 4.0 => {
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments in response");
            assert_vec_match!(additional_arguments, Amf0Value::Number(x) if x > 0.0);
        },

        _ => panic!("First response was not the expected value: {:?}", responses[0]),
    }
}

#[test]
fn can_accept_live_publishing_to_requested_stream_key() {
    let config = get_basic_config();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection("some_app", &mut session, &mut serializer, &mut deserializer);

    let stream_id = create_active_stream(&mut session, &mut serializer, &mut deserializer);
    let message = RtmpMessage::Amf0Command {
        command_name: "publish".to_string(),
        transaction_id: 5.0,
        command_object: Amf0Value::Null,
        additional_arguments: vec![
            Amf0Value::Utf8String("stream_key".to_string()),
            Amf0Value::Utf8String("live".to_string()),
        ]
    };

    let publish_payload = message.into_message_payload(RtmpTimestamp::new(0), stream_id).unwrap();
    let publish_packet = serializer.serialize(&publish_payload, false, false).unwrap();
    let publish_results = session.handle_input(&publish_packet.bytes[..]).unwrap();
    let (_, events) = split_results(&mut deserializer, publish_results);

    assert_eq!(events.len(), 1, "Unexpected number of events returned");
    let request_id = match events[0] {
        ServerSessionEvent::PublishStreamRequested {
            ref app_name,
            ref stream_key,
            request_id: returned_request_id,
            mode: PublishMode::Live,
        } if app_name == "some_app" && stream_key == "stream_key" => {
            returned_request_id
        },

        _ => panic!("Unexpected first event found: {:?}", events[0]),
    };

    let accept_results = session.accept_request(request_id).unwrap();
    let (responses, _) = split_results(&mut deserializer, accept_results);
    assert_eq!(responses.len(), 1, "Unexpected number of responses received");

    match responses[0] {
        RtmpMessage::Amf0Command {
            ref command_name,
            transaction_id,
            command_object: Amf0Value::Null,
            ref additional_arguments
        } if command_name == "onStatus" && transaction_id == 0.0 => {
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments");

            match additional_arguments[0] {
                Amf0Value::Object(ref properties) => {
                    assert_eq!(properties.get("level"), Some(&Amf0Value::Utf8String("status".to_string())), "Unexpected level value");
                    assert_eq!(properties.get("code"), Some(&Amf0Value::Utf8String("NetStream.Publish.Start".to_string())), "Unexpected code value");
                    assert!(properties.contains_key("description"), "No description was included");
                },

                _ => panic!("Unexpected first additional argument received: {:?}", additional_arguments[0]),
            }
        },

        _ => panic!("Unexpected first response: {:?}", responses[0]),
    }
}

#[test]
fn can_receive_and_raise_event_for_metadata_from_obs() {
    let config = get_basic_config();
    let test_app_name = "some_app".to_string();
    let test_stream_key = "stream_key".to_string();

    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection(test_app_name.as_ref(), &mut session, &mut serializer, &mut deserializer);
    let stream_id = create_active_stream(&mut session, &mut serializer, &mut deserializer);
    start_publishing(test_stream_key.as_ref(), stream_id, &mut session, &mut serializer, &mut deserializer);

    let mut properties = HashMap::new();
    properties.insert("width".to_string(), Amf0Value::Number(1920_f64));
    properties.insert("height".to_string(), Amf0Value::Number(1080_f64));
    properties.insert("videocodecid".to_string(), Amf0Value::Utf8String("avc1".to_string()));
    properties.insert("videodatarate".to_string(), Amf0Value::Number(1200_f64));
    properties.insert("framerate".to_string(), Amf0Value::Number(30_f64));
    properties.insert("audiocodecid".to_string(), Amf0Value::Utf8String("mp4a".to_string()));
    properties.insert("audiodatarate".to_string(), Amf0Value::Number(96_f64));
    properties.insert("audiosamplerate".to_string(), Amf0Value::Number(48000_f64));
    properties.insert("audiosamplesize".to_string(), Amf0Value::Number(16_f64));
    properties.insert("audiochannels".to_string(), Amf0Value::Number(2_f64));
    properties.insert("stereo".to_string(), Amf0Value::Boolean(true));
    properties.insert("encoder".to_string(), Amf0Value::Utf8String("Test Encoder".to_string()));

    let message = RtmpMessage::Amf0Data{
        values: vec![
            Amf0Value::Utf8String("@setDataFrame".to_string()),
            Amf0Value::Utf8String("onMetaData".to_string()),
            Amf0Value::Object(properties),
        ]
    };

    let metadata_payload = message.into_message_payload(RtmpTimestamp::new(0), stream_id).unwrap();
    let metadata_packet = serializer.serialize(&metadata_payload, false, false).unwrap();
    let metadata_results = session.handle_input(&metadata_packet.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, metadata_results);

    assert_eq!(events.len(), 1, "Unexpected number of metadata events");

    match events.remove(0) {
        ServerSessionEvent::StreamMetadataChanged {app_name, stream_key, metadata} => {
            assert_eq!(app_name, test_app_name, "Unexpected metadata app name");
            assert_eq!(stream_key, test_stream_key, "Unexpected metadata stream key");
            assert_eq!(metadata.video_width, Some(1920), "Unexpected video width");
            assert_eq!(metadata.video_height, Some(1080), "Unexepcted video height");
            assert_eq!(metadata.video_codec, Some("avc1".to_string()), "Unexepcted video codec");
            assert_eq!(metadata.video_frame_rate, Some(30_f32), "Unexpected framerate");
            assert_eq!(metadata.video_bitrate_kbps, Some(1200), "Unexpected video bitrate");
            assert_eq!(metadata.audio_codec, Some("mp4a".to_string()), "Unexpected audio codec");
            assert_eq!(metadata.audio_bitrate_kbps, Some(96), "Unexpected audio bitrate");
            assert_eq!(metadata.audio_sample_rate, Some(48000), "Unexpected audio sample rate");
            assert_eq!(metadata.audio_channels, Some(2), "Unexpected audio channels");
            assert_eq!(metadata.audio_is_stereo, Some(true), "Unexpected audio is stereo value");
            assert_eq!(metadata.encoder, Some("Test Encoder".to_string()), "Unexpected encoder value");
        },

        _ => panic!("Unexpected event received: {:?}", events[0]),
    }
}

#[test]
fn can_receive_audio_data_on_published_stream() {
    let config = get_basic_config();
    let test_app_name = "some_app".to_string();
    let test_stream_key = "stream_key".to_string();

    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection(test_app_name.as_ref(), &mut session, &mut serializer, &mut deserializer);
    let stream_id = create_active_stream(&mut session, &mut serializer, &mut deserializer);
    start_publishing(test_stream_key.as_ref(), stream_id, &mut session, &mut serializer, &mut deserializer);

    let message = RtmpMessage::AudioData {data: vec![1_u8, 2_u8, 3_u8]};
    let payload = message.into_message_payload(RtmpTimestamp::new(1234), stream_id).unwrap();
    let packet = serializer.serialize(&payload, false, false).unwrap();
    let results = session.handle_input(&packet.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Unexpected number of events returned");

    match events.remove(0) {
        ServerSessionEvent::AudioDataReceived {app_name, stream_key, data, timestamp} => {
            assert_eq!(app_name, test_app_name, "Unexpected app name");
            assert_eq!(stream_key, test_stream_key, "Unexpected stream key");
            assert_eq!(timestamp, RtmpTimestamp::new(1234), "Unexepcted timestamp");
            assert_eq!(&data[..], &[1_u8, 2_u8, 3_u8], "Unexpected data");
        },

        event => panic!("Expected AudioDataReceived event, instead got: {:?}", event),
    }
}

#[test]
fn can_receive_video_data_on_published_stream() {
    let config = get_basic_config();
    let test_app_name = "some_app".to_string();
    let test_stream_key = "stream_key".to_string();

    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection(test_app_name.as_ref(), &mut session, &mut serializer, &mut deserializer);
    let stream_id = create_active_stream(&mut session, &mut serializer, &mut deserializer);
    start_publishing(test_stream_key.as_ref(), stream_id, &mut session, &mut serializer, &mut deserializer);

    let message = RtmpMessage::VideoData {data: vec![1_u8, 2_u8, 3_u8]};
    let payload = message.into_message_payload(RtmpTimestamp::new(1234), stream_id).unwrap();
    let packet = serializer.serialize(&payload, false, false).unwrap();
    let results = session.handle_input(&packet.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Unexpected number of events returned");

    match events.remove(0) {
        ServerSessionEvent::VideoDataReceived {app_name, stream_key, data, timestamp} => {
            assert_eq!(app_name, test_app_name, "Unexpected app name");
            assert_eq!(stream_key, test_stream_key, "Unexpected stream key");
            assert_eq!(timestamp, RtmpTimestamp::new(1234), "Unexepcted timestamp");
            assert_eq!(&data[..], &[1_u8, 2_u8, 3_u8], "Unexpected data");
        },

        event => panic!("Expected AudioDataReceived event, instead got: {:?}", event),
    }
}

#[test]
fn publish_finished_event_raised_when_delete_stream_invoked_on_publishing_stream() {
    let config = get_basic_config();
    let test_app_name = "some_app".to_string();
    let test_stream_key = "stream_key".to_string();

    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection(test_app_name.as_ref(), &mut session, &mut serializer, &mut deserializer);
    let stream_id = create_active_stream(&mut session, &mut serializer, &mut deserializer);
    start_publishing(test_stream_key.as_ref(), stream_id, &mut session, &mut serializer, &mut deserializer);

    let message = RtmpMessage::Amf0Command {
        command_name: "deleteStream".to_string(),
        transaction_id: 4_f64,
        command_object: Amf0Value::Null,
        additional_arguments: vec![Amf0Value::Number(stream_id as f64)],
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(1234), stream_id).unwrap();
    let packet = serializer.serialize(&payload, false, false).unwrap();
    let results = session.handle_input(&packet.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Unexpected number of events returned");

    match events.remove(0) {
        ServerSessionEvent::PublishStreamFinished {app_name, stream_key} => {
            assert_eq!(app_name, test_app_name, "Unexpected app name");
            assert_eq!(stream_key, test_stream_key, "Unexpected stream key");
        }

        event => panic!("Expected PublishStreamFinished event, instead got: {:?}", event),
    }
}

#[test]
fn publish_finished_event_raised_when_close_stream_invoked_on_publishing_stream() {
    let config = get_basic_config();
    let test_app_name = "some_app".to_string();
    let test_stream_key = "stream_key".to_string();

    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let (mut session, results) = ServerSession::new(config.clone()).unwrap();
    consume_results(&mut deserializer, results);
    perform_connection(test_app_name.as_ref(), &mut session, &mut serializer, &mut deserializer);
    let stream_id = create_active_stream(&mut session, &mut serializer, &mut deserializer);
    start_publishing(test_stream_key.as_ref(), stream_id, &mut session, &mut serializer, &mut deserializer);

    let message = RtmpMessage::Amf0Command {
        command_name: "closeStream".to_string(),
        transaction_id: 4_f64,
        command_object: Amf0Value::Null,
        additional_arguments: vec![Amf0Value::Number(stream_id as f64)],
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(1234), stream_id).unwrap();
    let packet = serializer.serialize(&payload, false, false).unwrap();
    let results = session.handle_input(&packet.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Unexpected number of events returned");

    match events.remove(0) {
        ServerSessionEvent::PublishStreamFinished {app_name, stream_key} => {
            assert_eq!(app_name, test_app_name, "Unexpected app name");
            assert_eq!(stream_key, test_stream_key, "Unexpected stream key");
        }

        event => panic!("Expected PublishStreamFinished event, instead got: {:?}", event),
    }
}

fn get_basic_config() -> ServerSessionConfig {
    ServerSessionConfig {
        chunk_size: DEFAULT_CHUNK_SIZE,
        fms_version: "fms_version".to_string(),
        peer_bandwidth: DEFAULT_PEER_BANDWIDTH,
        window_ack_size: DEFAULT_WINDOW_ACK_SIZE,
    }
}

fn split_results(deserializer: &mut ChunkDeserializer, mut results: Vec<ServerSessionResult>) -> (Vec<RtmpMessage>, Vec<ServerSessionEvent>) {
    let mut responses = Vec::new();
    let mut events = Vec::new();

    for result in results.drain(..) {
        match result {
            ServerSessionResult::OutboundResponse(packet) => {
                let payload = deserializer.get_next_message(&packet.bytes[..]).unwrap().unwrap();
                let message = payload.to_rtmp_message().unwrap();
                match message {
                    RtmpMessage::SetChunkSize{size} => deserializer.set_max_chunk_size(size as usize).unwrap(),
                    _ => (),
                }

                println!("response received from server: {:?}", message);
                responses.push(message);
            },

            ServerSessionResult::RaisedEvent(event) => {
                println!("event received from server: {:?}", event);
                events.push(event);
            },

            _ => (),
        }
    }

    (responses, events)
}

fn consume_results(deserializer: &mut ChunkDeserializer, results: Vec<ServerSessionResult>) {
    // Needed to keep the deserializer up to date
    split_results(deserializer, results);
}

fn create_connect_message(app_name: String, timestamp: u32, stream_id: u32, object_encoding: f64) -> MessagePayload {
    let mut properties = HashMap::new();
    properties.insert("app".to_string(), Amf0Value::Utf8String(app_name));
    properties.insert("objectEncoding".to_string(), Amf0Value::Number(object_encoding));

    let message = RtmpMessage::Amf0Command {
        command_name: "connect".to_string(),
        transaction_id: 1.0,
        command_object: Amf0Value::Object(properties),
        additional_arguments: vec![]
    };

    let timestamp = RtmpTimestamp::new(timestamp);
    let payload = message.into_message_payload(timestamp, stream_id).unwrap();
    payload
}

fn perform_connection(app_name: &str, session: &mut ServerSession, serializer: &mut ChunkSerializer, deserializer: &mut ChunkDeserializer) {
    let connect_payload = create_connect_message(app_name.to_string(), 15, 0, 0.0);
    let connect_packet = serializer.serialize(&connect_payload, true, false).unwrap();
    let connect_results = session.handle_input(&connect_packet.bytes[..]).unwrap();
    assert_eq!(connect_results.len(), 1, "Unexpected number of responses when handling connect request message");

    let (_, events) = split_results(deserializer, connect_results);
    assert_eq!(events.len(), 1, "Unexpected number of events returned");
    let request_id = match events[0] {
        ServerSessionEvent::ConnectionRequested {ref app_name, request_id} if app_name == "some_app" => request_id,
        _ => panic!("First event was not as expected: {:?}", events[0]),
    };

    let results = session.accept_request(request_id).unwrap();
    consume_results(deserializer, results);

    // Assume it was successful
}

fn create_active_stream(session: &mut ServerSession, serializer: &mut ChunkSerializer, deserializer: &mut ChunkDeserializer) -> u32 {
    let message = RtmpMessage::Amf0Command {
        command_name: "createStream".to_string(),
        transaction_id: 4.0,
        command_object: Amf0Value::Null,
        additional_arguments: Vec::new()
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(0), 0).unwrap();
    let packet = serializer.serialize(&payload, true, false).unwrap();
    let results = session.handle_input(&packet.bytes[..]).unwrap();
    let (responses, _) = split_results(deserializer, results);

    assert_eq!(responses.len(), 1, "Unexpected number of responses returned");
    match responses[0] {
        RtmpMessage::Amf0Command {
            ref command_name,
            transaction_id,
            command_object: Amf0Value::Null,
            ref additional_arguments
        } if command_name == "_result" && transaction_id == 4.0 => {
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments in response");
            match additional_arguments[0] {
                Amf0Value::Number(x) => return x as u32,
                _ => panic!("First additional argument was not an Amf0Value::Number"),
            }
        },

        _ => panic!("First response was not the expected value: {:?}", responses[0]),
    }
}

fn start_publishing(stream_key: &str,
                    stream_id: u32,
                    session: &mut ServerSession,
                    serializer: &mut ChunkSerializer,
                    deserializer: &mut ChunkDeserializer) {
    let message = RtmpMessage::Amf0Command {
        command_name: "publish".to_string(),
        transaction_id: 5.0,
        command_object: Amf0Value::Null,
        additional_arguments: vec![
            Amf0Value::Utf8String(stream_key.to_string()),
            Amf0Value::Utf8String("live".to_string()),
        ]
    };

    let publish_payload = message.into_message_payload(RtmpTimestamp::new(0), stream_id).unwrap();
    let publish_packet = serializer.serialize(&publish_payload, false, false).unwrap();
    let publish_results = session.handle_input(&publish_packet.bytes[..]).unwrap();
    let (_, events) = split_results(deserializer, publish_results);

    assert_eq!(events.len(), 1, "Unexpected number of events returned");
    let request_id = match events[0] {
        ServerSessionEvent::PublishStreamRequested {
            ref app_name,
            ref stream_key,
            request_id: returned_request_id,
            mode: PublishMode::Live,
        } if app_name == "some_app" && stream_key == "stream_key" => {
            returned_request_id
        },

        _ => panic!("Unexpected first event found: {:?}", events[0]),
    };

    let accept_results = session.accept_request(request_id).unwrap();
    let (responses, _) = split_results(deserializer, accept_results);
    assert_eq!(responses.len(), 1, "Unexpected number of responses received");

    match responses[0] {
        RtmpMessage::Amf0Command {
            ref command_name,
            transaction_id,
            command_object: Amf0Value::Null,
            ref additional_arguments
        } if command_name == "onStatus" && transaction_id == 0.0 => {
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments");

            match additional_arguments[0] {
                Amf0Value::Object(ref properties) => {
                    assert_eq!(properties.get("level"), Some(&Amf0Value::Utf8String("status".to_string())), "Unexpected level value");
                    assert_eq!(properties.get("code"), Some(&Amf0Value::Utf8String("NetStream.Publish.Start".to_string())), "Unexpected code value");
                    assert!(properties.contains_key("description"), "No description was included");
                },

                _ => panic!("Unexpected first additional argument received: {:?}", additional_arguments[0]),
            }
        },

        _ => panic!("Unexpected first response: {:?}", responses[0]),
    }
}
