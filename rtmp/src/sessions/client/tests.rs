use super::*;
use std::collections::HashMap;
use rand;
use rml_amf0::Amf0Value;
use chunk_io::{ChunkDeserializer, ChunkSerializer, Packet};
use messages::{MessagePayload, RtmpMessage,UserControlEventType};

#[test]
fn can_send_connect_request() {
    let app_name = "test".to_string();
    let config = ClientSessionConfig::new();
    let mut deserializer = ChunkDeserializer::new();
    let mut session = ClientSession::new(config.clone());

    let results = session.request_connection(app_name.clone()).unwrap();
    let (mut responses, _) = split_results(&mut deserializer, vec![results]);

    assert_eq!(responses.len(), 1, "Expected 1 response");
    match responses.remove(0) {
        (payload, RtmpMessage::Amf0Command {command_name, transaction_id, command_object, additional_arguments}) => {
            assert_eq!(payload.message_stream_id, 0, "Unexpected message stream id");
            assert_eq!(command_name, "connect".to_string(), "Unexpected command name");
            assert_ne!(transaction_id, 0.0, "Transaction id should not be zero");
            assert_eq!(additional_arguments.len(), 0, "Expected no additional arguments");

            match command_object {
                Amf0Value::Object(properties) => {
                    assert_eq!(properties.get("app"), Some(&Amf0Value::Utf8String(app_name.clone())), "Unexpected app name");
                    assert_eq!(properties.get("objectEncoding"), Some(&Amf0Value::Number(0.0)), "Unexpected object encoding");
                    assert_eq!(properties.get("flashVer"), Some(&Amf0Value::Utf8String(config.flash_version.clone())), "Unexpected flash version");
                },

                x => panic!("Expected Amf0Value::Object for command object, instead received: {:?}", x),
            }
        },

        x => panic!("Expected Amf0Command, instead received: {:?}", x),
    }
}

#[test]
fn can_process_connect_success_response() {
    let app_name = "test".to_string();
    let config = ClientSessionConfig::new();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let mut session = ClientSession::new(config.clone());

    let results = session.request_connection(app_name.clone()).unwrap();
    consume_results(&mut deserializer, vec![results]);

    let response = get_connect_success_response(&mut serializer);
    let results = session.handle_input(&response.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Expected one event returned");
    match events.remove(0) {
        ClientSessionEvent::ConnectionRequestAccepted => (),
        x => panic!("Expected connection accepted event, instead received: {:?}", x),
    }
}

#[test]
fn event_raised_when_connect_request_rejected() {
    let app_name = "test".to_string();
    let config = ClientSessionConfig::new();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let mut session = ClientSession::new(config.clone());

    let results = session.request_connection(app_name.clone()).unwrap();
    consume_results(&mut deserializer, vec![results]);

    let response = get_connect_error_response(&mut serializer);
    let results = session.handle_input(&response.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Expected one event returned");
    match events.remove(0) {
        ClientSessionEvent::ConnectionRequestRejected {description} => {
            assert!(description.len() > 0, "Expected a non-empty description");
        },

        x => panic!("Expected connection accepted event, instead received: {:?}", x),
    }
}

#[test]
fn error_thrown_when_connect_request_made_after_successful_connection() {
    let app_name = "test".to_string();
    let config = ClientSessionConfig::new();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let mut session = ClientSession::new(config.clone());

    let results = session.request_connection(app_name.clone()).unwrap();
    consume_results(&mut deserializer, vec![results]);

    let response = get_connect_success_response(&mut serializer);
    let results = session.handle_input(&response.bytes[..]).unwrap();
    consume_results(&mut deserializer, results);

    let error = session.request_connection(app_name.clone()).unwrap_err();
    match error.kind {
        ClientSessionErrorKind::CantConnectWhileAlreadyConnected => (),
        x => panic!("Expected CantConnectWhileAlreadyConnected, instead found {:?}", x),
    }
}

#[test]
fn successful_connect_request_sends_window_ack_size() {
    let app_name = "test".to_string();
    let config = ClientSessionConfig::new();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let mut session = ClientSession::new(config.clone());

    let results = session.request_connection(app_name.clone()).unwrap();
    consume_results(&mut deserializer, vec![results]);

    let response = get_connect_success_response(&mut serializer);
    let results = session.handle_input(&response.bytes[..]).unwrap();
    let (mut responses, _) = split_results(&mut deserializer, results);

    assert_eq!(responses.len(), 1, "Unexpected number of responses received");
    match responses.remove(0) {
        (payload, RtmpMessage::WindowAcknowledgement {size}) => {
            assert_eq!(payload.message_stream_id, 0, "Unexpected message stream id");
            assert_eq!(size, config.window_ack_size, "Unexpected message ack size");
        },

        (_, x) => panic!("Expected window ack message, instead found {:?}", x),
    }
}

#[test]
fn successful_play_request_workflow() {
    let app_name = "test".to_string();
    let stream_key = "test-key".to_string();
    let config = ClientSessionConfig::new();
    let mut deserializer = ChunkDeserializer::new();
    let mut serializer = ChunkSerializer::new();
    let mut session = ClientSession::new(config.clone());
    perform_successful_connect(app_name.clone(), &mut session, &mut serializer, &mut deserializer);

    let result = session.request_playback(stream_key.clone()).unwrap();
    let (mut responses, _) = split_results(&mut deserializer, vec![result]);

    assert_eq!(responses.len(), 1, "Unexpected number of responses");
    let transaction_id = match responses.remove(0) {
        (payload, RtmpMessage::Amf0Command {command_name, transaction_id, command_object, additional_arguments}) => {
            assert_eq!(payload.message_stream_id, 0, "Unexpected stream id");
            assert_eq!(command_name, "createStream", "Unexpected command name");
            assert_eq!(command_object, Amf0Value::Null, "Unexpected command object");
            assert_eq!(additional_arguments.len(), 0, "Uenxpected number of additional arguments");
            transaction_id
        },

        x => panic!("Unexpected response seen: {:?}", x),
    };

    let (created_stream_id, create_stream_response) = get_create_stream_success_response(transaction_id, &mut serializer);
    let results = session.handle_input(&create_stream_response.bytes[..]).unwrap();
    let (mut responses, _) = split_results(&mut deserializer, results);

    assert_eq!(responses.len(), 2, "Expected one response returned");
    match responses.remove(0) {
        (payload, RtmpMessage::UserControl {event_type, stream_id, buffer_length, timestamp}) => {
            assert_eq!(payload.message_stream_id, 0, "Unexpected message stream id");
            assert_eq!(stream_id, Some(created_stream_id), "Unexpected user control stream id");
            assert_eq!(event_type, UserControlEventType::SetBufferLength, "Unexpected user control event type");
            assert_eq!(buffer_length, Some(config.playback_buffer_length_ms), "Unexpected playback buffer lenght");
            assert_eq!(timestamp, None, "Unexpected timestamp");
        },

        x => panic!("Expected set buffer length message, instead received: {:?}", x),
    }

    match responses.remove(0) {
        (payload, RtmpMessage::Amf0Command {command_name, transaction_id: _, command_object, additional_arguments}) => {
            assert_eq!(payload.message_stream_id, created_stream_id, "Unexpected message stream id");
            assert_eq!(command_name, "play".to_string(), "Unexpected command name");
            assert_eq!(command_object, Amf0Value::Null, "Unexpected command object");
            assert_eq!(additional_arguments.len(), 1, "Unexpected number of additional arguments");
            assert_eq!(additional_arguments[0], Amf0Value::Utf8String(stream_key.clone()), "Unexpected stream key");
        },

        x => panic!("Expected play message, instead received: {:?}", x),
    };

    let play_response = get_play_success_response(&mut serializer);
    let results = session.handle_input(&play_response.bytes[..]).unwrap();
    let (_, mut events) = split_results(&mut deserializer, results);

    assert_eq!(events.len(), 1, "Expected one event returned");
    match events.remove(0) {
        ClientSessionEvent::PlaybackRequestAccepted {stream_key: event_stream_key} => {
            assert_eq!(event_stream_key, stream_key, "Unexpected stream key in play request accepted event");
        },

        x => panic!("Expected playback accepted event, instead received: {:?}", x),
    }
}

fn split_results(deserializer: &mut ChunkDeserializer, mut results: Vec<ClientSessionResult>)
    -> (Vec<(MessagePayload, RtmpMessage)>, Vec<ClientSessionEvent>) {
    let mut responses = Vec::new();
    let mut events = Vec::new();

    for result in results.drain(..) {
        match result {
            ClientSessionResult::OutboundResponse(packet) => {
                let payload = deserializer.get_next_message(&packet.bytes[..]).unwrap().unwrap();
                let message = payload.to_rtmp_message().unwrap();
                match message {
                    RtmpMessage::SetChunkSize{size} => deserializer.set_max_chunk_size(size as usize).unwrap(),
                    _ => (),
                }

                println!("response received: {:?}", message);
                responses.push((payload, message));
            },

            ClientSessionResult::RaisedEvent(event) => {
                println!("event received: {:?}", event);
                events.push(event);
            },

            ClientSessionResult::UnhandleableMessageReceived(payload) => {
                println!("unhandleable message: {:?}", payload);
            }
        }
    }

    (responses, events)
}

fn consume_results(deserializer: &mut ChunkDeserializer, results: Vec<ClientSessionResult>) {
    // Needed to keep the deserializer up to date
    split_results(deserializer, results);
}

fn get_connect_success_response(serializer: &mut ChunkSerializer) -> Packet {
    let mut command_properties = HashMap::new();
    command_properties.insert("fmsVer".to_string(), Amf0Value::Utf8String("fms".to_string()));
    command_properties.insert("capabilities".to_string(), Amf0Value::Number(31.0));

    let mut additional_properties = HashMap::new();
    additional_properties.insert("level".to_string(), Amf0Value::Utf8String("status".to_string()));
    additional_properties.insert("code".to_string(), Amf0Value::Utf8String("NetConnection.Connect.Success".to_string()));
    additional_properties.insert("description".to_string(), Amf0Value::Utf8String("hi".to_string()));
    additional_properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));

    let message = RtmpMessage::Amf0Command {
        command_name: "_result".to_string(),
        transaction_id: 1.0,
        command_object: Amf0Value::Object(command_properties),
        additional_arguments: vec![Amf0Value::Object(additional_properties)],
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(0), 0).unwrap();
    serializer.serialize(&payload, false, false).unwrap()
}

fn get_connect_error_response(serializer: &mut ChunkSerializer) -> Packet {
    let mut command_properties = HashMap::new();
    command_properties.insert("fmsVer".to_string(), Amf0Value::Utf8String("fms".to_string()));
    command_properties.insert("capabilities".to_string(), Amf0Value::Number(31.0));

    let mut additional_properties = HashMap::new();
    additional_properties.insert("level".to_string(), Amf0Value::Utf8String("error".to_string()));
    additional_properties.insert("code".to_string(), Amf0Value::Utf8String("NetConnection.Connect.Failed".to_string()));
    additional_properties.insert("description".to_string(), Amf0Value::Utf8String("hi".to_string()));
    additional_properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));

    let message = RtmpMessage::Amf0Command {
        command_name: "_error".to_string(),
        transaction_id: 1.0,
        command_object: Amf0Value::Object(command_properties),
        additional_arguments: vec![Amf0Value::Object(additional_properties)],
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(0), 0).unwrap();
    serializer.serialize(&payload, false, false).unwrap()
}

fn get_create_stream_success_response(transaction_id: f64, serializer: &mut ChunkSerializer) -> (u32, Packet) {
    let stream_id = rand::random::<u32>();
    let message = RtmpMessage::Amf0Command {
        command_name: "_result".to_string(),
        command_object: Amf0Value::Null,
        additional_arguments: vec![Amf0Value::Number(stream_id as f64)],
        transaction_id,
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(0), 0).unwrap();
    let packet = serializer.serialize(&payload, false, false).unwrap();
    (stream_id, packet)
}

fn get_play_success_response(serializer: &mut ChunkSerializer) -> Packet {
    let mut additional_properties = HashMap::new();
    additional_properties.insert("level".to_string(), Amf0Value::Utf8String("status".to_string()));
    additional_properties.insert("code".to_string(), Amf0Value::Utf8String("NetStream.Play.Start".to_string()));
    additional_properties.insert("description".to_string(), Amf0Value::Utf8String("hi".to_string()));

    let message = RtmpMessage::Amf0Command {
        command_name: "onStatus".to_string(),
        transaction_id: 0.0,
        command_object: Amf0Value::Null,
        additional_arguments: vec![Amf0Value::Object(additional_properties)],
    };

    let payload = message.into_message_payload(RtmpTimestamp::new(0), 0).unwrap();
    serializer.serialize(&payload, false, false).unwrap()
}

fn perform_successful_connect(app_name: String,
                              session: &mut ClientSession,
                              serializer: &mut ChunkSerializer,
                              deserializer: &mut ChunkDeserializer) {
    let results = session.request_connection(app_name).unwrap();
    consume_results(deserializer, vec![results]);

    let response = get_connect_success_response(serializer);
    let results = session.handle_input(&response.bytes[..]).unwrap();
    let (_, mut events) = split_results(deserializer, results);

    assert_eq!(events.len(), 1, "Expected one event returned");
    match events.remove(0) {
        ClientSessionEvent::ConnectionRequestAccepted => (),
        x => panic!("Expected connection accepted event, instead received: {:?}", x),
    }
}