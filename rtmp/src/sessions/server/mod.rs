mod events;
mod config;
mod errors;
mod result;

use std::collections::HashMap;
use std::time::SystemTime;
use rml_amf0::Amf0Value;
use ::chunk_io::{ChunkSerializer, ChunkDeserializer};
use ::messages::{RtmpMessage, UserControlEventType, PeerBandwidthLimitType};
use ::time::RtmpTimestamp;

pub use self::errors::{ServerSessionError, ServerSessionErrorKind};
pub use self::events::ServerSessionEvent;
pub use self::config::ServerSessionConfig;
pub use self::result::ServerSessionResult;

enum OutstandingRequest {
    ConnectionRequest{
        app_name: String,
        transaction_id: f64,
    }
}

enum SessionState {
    Started,
    Connected,
}

/// A session that represents the server side of a single RTMP connection.
///
/// The `ServerSession` encapsulates the process of parsing RTMP chunks coming in from a client
/// into RTMP messages and performs common server side workflows to handle those messages.  It can
/// either provide pre-serialized messages to be sent back to the client or events that
/// parent applications can perform custom logic against (like verifying if a connection request
/// should be accepted or not).
///
/// The `ServerSession` does not care how RTMP chunks (encoded as bytes) come in or get sent out,
/// but leaves that up to the application utilizing the `ServerSession`.
///
/// Due to the header compression properties of the RTMP chunking protocol it is required that
/// all bytes **after** the handshake has been completed are passed into the `ServerSession`, that
/// all responses returned by the `ServerSession` are sent to the client **in order**, and that
/// no additional bytes are sent to the client.  Any violation of these rules have a high
/// high probability of causing RTMP chunk parsing errors by the peer or by the `ServerSession`
/// instance itself.
pub struct ServerSession {
    start_time: SystemTime,
    serializer: ChunkSerializer,
    deserializer: ChunkDeserializer,
    self_window_ack_size: u32,
    connected_app_name: Option<String>,
    outstanding_requests: HashMap<u32, OutstandingRequest>,
    next_request_number: u32,
    current_state: SessionState,
    fms_version: String,
    object_encoding: f64,
}

impl ServerSession {
    /// Creates a new server session.
    ///
    /// As part of the initial creation it automatically creates the initial outbound RTMp messages
    /// that the RTMP message protocol requires to confirm to the client that it can stream on
    /// stream id 0 (as well as other important initial information
    pub fn new(config: ServerSessionConfig) -> Result<(ServerSession, Vec<ServerSessionResult>), ServerSessionError> {
        let mut session = ServerSession {
            start_time: SystemTime::now(),
            serializer: ChunkSerializer::new(),
            deserializer: ChunkDeserializer::new(),
            self_window_ack_size: config.window_ack_size,
            connected_app_name: None,
            outstanding_requests: HashMap::new(),
            next_request_number: 0,
            current_state: SessionState::Started,
            fms_version: config.fms_version,
            object_encoding: 0.0,
        };

        let mut results = Vec::with_capacity(4);

        let chunk_size_packet = session.serializer.set_max_chunk_size(config.chunk_size, RtmpTimestamp::new(0))?;
        results.push(ServerSessionResult::OutboundResponse(chunk_size_packet));

        let window_ack_message = RtmpMessage::WindowAcknowledgement {size: session.self_window_ack_size};
        let window_ack_payload = window_ack_message.into_message_payload(session.get_epoch(), 0)?;
        let window_ack_packet = session.serializer.serialize(&window_ack_payload, true, false)?;
        results.push(ServerSessionResult::OutboundResponse(window_ack_packet));

        let begin_message = RtmpMessage::UserControl {
            event_type: UserControlEventType::StreamBegin,
            stream_id: Some(0),
            timestamp: None,
            buffer_length: None
        };

        let begin_payload = begin_message.into_message_payload(session.get_epoch(), 0)?;
        let begin_packet = session.serializer.serialize(&begin_payload, true, false)?;
        results.push(ServerSessionResult::OutboundResponse(begin_packet));

        let peer_message = RtmpMessage::SetPeerBandwidth {size: config.peer_bandwidth, limit_type: PeerBandwidthLimitType::Dynamic};
        let peer_payload = peer_message.into_message_payload(session.get_epoch(), 0)?;
        let peer_packet = session.serializer.serialize(&peer_payload, true, false)?;
        results.push(ServerSessionResult::OutboundResponse(peer_packet));

        let bw_done_message = RtmpMessage::Amf0Command {
            command_name: "onBWDone".to_string(),
            transaction_id: 0.0,
            command_object: Amf0Value::Null,
            additional_arguments: vec![Amf0Value::Number(8192_f64)]
        };

        let bw_done_payload = bw_done_message.into_message_payload(session.get_epoch(), 0)?;
        let bw_done_packet = session.serializer.serialize(&bw_done_payload, true, false)?;
        results.push(ServerSessionResult::OutboundResponse(bw_done_packet));

        Ok((session, results))
    }

    /// Takes in bytes that are encoding RTMP chunks and returns any responses or events that can
    /// be reacted to.
    pub fn handle_input(&mut self, bytes: &[u8]) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        let mut results = Vec::new();
        let mut bytes_to_process = bytes;

        loop {
            match self.deserializer.get_next_message(bytes_to_process)? {
                None => break,
                Some(payload) => {
                    let message = payload.to_rtmp_message()?;

                    let mut message_results = match message {
                        RtmpMessage::Abort{stream_id} 
                            => self.handle_abort_message(stream_id)?,

                        RtmpMessage::Acknowledgement{sequence_number} 
                            => self.handle_acknowledgement_message(sequence_number)?,

                        RtmpMessage::Amf0Command{command_name, transaction_id, command_object, additional_arguments}
                            => self.handle_amf0_command(command_name, transaction_id, command_object, additional_arguments)?,

                        RtmpMessage::Amf0Data{values}
                            => self.handle_amf0_data(values)?,

                        RtmpMessage::AudioData{data}
                            => self.handle_audio_data(data)?,

                        RtmpMessage::SetChunkSize{size}
                            => self.handle_set_chunk_size(size)?,

                        RtmpMessage::SetPeerBandwidth{size, limit_type}
                            => self.handle_set_peer_bandwidth(size, limit_type)?,

                        RtmpMessage::UserControl{event_type, stream_id, buffer_length, timestamp}
                            => self.handle_user_control(event_type, stream_id, buffer_length, timestamp)?,

                        RtmpMessage::VideoData{data}
                            => self.handle_video_data(data)?,

                        RtmpMessage::WindowAcknowledgement{size}
                            => self.handle_window_acknowledgement(size)?,

                        _ => vec![ServerSessionResult::UnhandleableMessageReceived(payload)],
                    };

                    results.append(&mut message_results);
                    bytes_to_process = &[];
                }
            }
        }

        Ok(results)
    }

    /// Tells the server session that it should accept an outstanding request
    pub fn accept_request(&mut self, request_id: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        let request = match self.outstanding_requests.remove(&request_id) {
            Some(x) => x,
            None => return Err(ServerSessionError{kind: ServerSessionErrorKind::InvalidRequestId}),
        };

        match request {
            OutstandingRequest::ConnectionRequest {app_name, transaction_id}
                => self.accept_connection_request(app_name, transaction_id),
        }
    }

    /// Tells the server session that it should reject an outstanding request
    pub fn reject_request(&mut self, _request_id: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        unimplemented!()
    }

    fn handle_abort_message(&self, _stream_id: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_acknowledgement_message(&self, _sequence_number: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_amf0_command(&mut self, name: String, transaction_id: f64, command_object: Amf0Value, _additional_args: Vec<Amf0Value>)
        -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        
        let results = match name.as_str() {
            "connect" => self.handle_command_connect(transaction_id, command_object)?,
            _ => Vec::new(),
        };

        Ok(results)
    }

    fn handle_command_connect(&mut self, transaction_id: f64, command_object: Amf0Value) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        let mut properties = match command_object {
            Amf0Value::Object(properties) => properties,
            _ => return Err(ServerSessionError{kind: ServerSessionErrorKind::NoAppNameForConnectionRequest}),
        };

        let app_name = match properties.remove("app") {
            Some(value) => match value {
                Amf0Value::Utf8String(app) => app,
                _ => return Err(ServerSessionError{kind: ServerSessionErrorKind::NoAppNameForConnectionRequest}),
            },
            None => return Err(ServerSessionError{kind: ServerSessionErrorKind::NoAppNameForConnectionRequest}),
        };

        self.object_encoding = match properties.remove("objectEncoding") {
            Some(value) => match value {
                Amf0Value::Number(number) => number,
                _ => 0.0,
            },
            None => 0.0,
        };

        let request = OutstandingRequest::ConnectionRequest {
            app_name: app_name.clone(),
            transaction_id,
        };

        let request_number = self.next_request_number;
        self.next_request_number = self.next_request_number + 1;
        self.outstanding_requests.insert(request_number, request);

        let event = ServerSessionEvent::ConnectionRequested {
            app_name: app_name,
            request_id: request_number,
        };

        Ok(vec![ServerSessionResult::RaisedEvent(event)])
    }

    fn handle_amf0_data(&self, _data: Vec<Amf0Value>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_audio_data(&self, _data: Vec<u8>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_set_chunk_size(&mut self, size: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        self.deserializer.set_max_chunk_size(size as usize)?;
        Ok(Vec::new())
    }

    fn handle_set_peer_bandwidth(&self, _size: u32, _limit_type: PeerBandwidthLimitType) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_user_control(&self, _event_type: UserControlEventType, _stream_id: Option<u32>, _buffer_length: Option<u32>, _timestamp: Option<RtmpTimestamp>)
        -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_video_data(&self, _data: Vec<u8>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_window_acknowledgement(&self, _size: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn accept_connection_request(&mut self, app_name: String, transaction_id: f64) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        self.connected_app_name = Some(app_name.clone());
        self.current_state = SessionState::Connected;

        let mut command_object_properties = HashMap::new();
        command_object_properties.insert("fmsVer".to_string(), Amf0Value::Utf8String(self.fms_version.clone()));
        command_object_properties.insert("capabilities".to_string(), Amf0Value::Number(31.0));

        let mut additional_properties = HashMap::new();
        additional_properties.insert("level".to_string(), Amf0Value::Utf8String("status".to_string()));
        additional_properties.insert("code".to_string(), Amf0Value::Utf8String("NetConnection.Connect.Success".to_string()));
        additional_properties.insert("objectEncoding".to_string(), Amf0Value::Number(self.object_encoding));
        additional_properties.insert("description".to_string(), Amf0Value::Utf8String("Successfully connected on app: ".to_string() + &app_name));

        let message = RtmpMessage::Amf0Command {
            command_name: "_result".to_string(),
            transaction_id: transaction_id,
            command_object: Amf0Value::Object(command_object_properties),
            additional_arguments: vec![Amf0Value::Object(additional_properties)]
        };

        let payload = message.into_message_payload(self.get_epoch(), 0)?;
        let packet = self.serializer.serialize(&payload, false, false)?;
        
        
        Ok(vec![ServerSessionResult::OutboundResponse(packet)])
    }
    
    fn get_epoch(&self) -> RtmpTimestamp {
        match self.start_time.elapsed() {
            Ok(duration) => {
                let milliseconds = (duration.as_secs() * 1000) + (duration.subsec_nanos() as u64 / 1_000_000);

                // Casting to u32 should auto-wrap the value as expected.  If not a stream will probably
                // break after 49 days but testing shows it should wrap  
                RtmpTimestamp::new(milliseconds as u32)
            },

            Err(_) => RtmpTimestamp::new(0), // Time went backwards, so just consider time as at epoch
        }
    }
}

#[cfg(test)]
mod tests {
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
        let (_, mut results) = ServerSession::new(config).unwrap();

        let (responses, _) = split_results(&mut deserializer, &mut results);

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
        let (mut session, mut initial_results) = ServerSession::new(config.clone()).unwrap();
        consume_results(&mut deserializer, &mut initial_results);

        let connect_payload = create_connect_message("some_app".to_string(), 15, 0, 0.0);
        let connect_packet = serializer.serialize(&connect_payload, true, false).unwrap();
        let mut connect_results = session.handle_input(&connect_packet.bytes[..]).unwrap();
        assert_eq!(connect_results.len(), 1, "Unexpected number of responses when handling connect request message");

        let (_, events) = split_results(&mut deserializer, &mut connect_results);
        assert_eq!(events.len(), 1, "Unexpected number of events returned");
        let request_id = match events[0] {
            ServerSessionEvent::ConnectionRequested {ref app_name, request_id} if app_name == "some_app" => request_id,
            _ => panic!("First event was not as expected: {:?}", events[0]),
        };

        let mut accept_results = session.accept_request(request_id).unwrap();
        assert_eq!(accept_results.len(), 1, "Unexpected number of results returned");

        let (responses, _) = split_results(&mut deserializer, &mut accept_results);
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

    fn get_basic_config() -> ServerSessionConfig {
        ServerSessionConfig {
            chunk_size: DEFAULT_CHUNK_SIZE,
            fms_version: "fms_version".to_string(),
            peer_bandwidth: DEFAULT_PEER_BANDWIDTH,
            window_ack_size: DEFAULT_WINDOW_ACK_SIZE,
        }
    }

    fn split_results(deserializer: &mut ChunkDeserializer, results: &mut Vec<ServerSessionResult>) -> (Vec<RtmpMessage>, Vec<ServerSessionEvent>) {
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
                    events.push(event);
                },

                _ => (),
            }
        }

        (responses, events)
    }

    fn consume_results(deserializer: &mut ChunkDeserializer, results: &mut Vec<ServerSessionResult>) {
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
}