use super::*;
use std::collections::HashMap;
use chunk_io::{ChunkDeserializer, ChunkSerializer, Packet};
use messages::MessagePayload;
use messages::RtmpMessage;
use rml_amf0::Amf0Value;

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