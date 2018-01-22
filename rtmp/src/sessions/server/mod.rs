mod active_stream;
mod config;
mod errors;
mod events;
mod outstanding_requests;
mod publish_mode;
mod result;
mod session_state;

use std::collections::HashMap;
use std::time::SystemTime;
use rml_amf0::Amf0Value;
use ::chunk_io::{ChunkSerializer, ChunkDeserializer, Packet};
use ::messages::{RtmpMessage, UserControlEventType, PeerBandwidthLimitType};
use ::sessions::{StreamMetadata};
use ::time::RtmpTimestamp;
use self::active_stream::{ActiveStream, StreamState};
use self::outstanding_requests::OutstandingRequest;
use self::session_state::SessionState;

pub use self::errors::{ServerSessionError, ServerSessionErrorKind};
pub use self::config::ServerSessionConfig;
pub use self::events::ServerSessionEvent;
pub use self::publish_mode::PublishMode;
pub use self::result::ServerSessionResult;

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
    active_streams: HashMap<u32, ActiveStream>,
    next_stream_id: u32,
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
            active_streams: HashMap::new(),
            next_stream_id: 1,
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
                            => self.handle_amf0_command(payload.message_stream_id, command_name, transaction_id, command_object, additional_arguments)?,

                        RtmpMessage::Amf0Data{values}
                            => self.handle_amf0_data(values, payload.message_stream_id)?,

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

            OutstandingRequest::PublishRequested {stream_key, mode, stream_id}
                => self.accept_publish_request(stream_id, stream_key, mode),
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

    fn handle_amf0_command(&mut self,
                           stream_id: u32,
                           name: String,
                           transaction_id: f64,
                           command_object: Amf0Value,
                           additional_args: Vec<Amf0Value>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        let results = match name.as_str() {
            "connect" => self.handle_command_connect(transaction_id, command_object)?,
            "createStream" => self.handle_command_create_stream(transaction_id)?,
            "publish" => self.handle_command_publish(stream_id, transaction_id, additional_args)?,

            _ => vec![ServerSessionResult::RaisedEvent(ServerSessionEvent::UnhandleableAmf0Command {
                command_name: name,
                additional_values: additional_args,
                transaction_id,
                command_object,
            })],
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

    fn handle_command_create_stream(&mut self, transaction_id: f64) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        let new_stream_id = self.next_stream_id;
        self.next_stream_id = self.next_stream_id + 1;

        let new_stream = ActiveStream{
            current_state: StreamState::Created,
        };
        self.active_streams.insert(new_stream_id, new_stream);

        let packet = self.create_success_response(transaction_id,
            Amf0Value::Null,
            vec![Amf0Value::Number(new_stream_id as f64)],
            new_stream_id)?;

        Ok(vec![ServerSessionResult::OutboundResponse(packet)])
    }

    fn handle_command_publish(&mut self, stream_id: u32, transaction_id: f64, mut arguments: Vec<Amf0Value>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        if arguments.len() < 2 {
            let packet = self.create_error_packet("NetStream.Publish.Start", "Invalid publish arguments", transaction_id, stream_id)?;
            return Ok(vec![ServerSessionResult::OutboundResponse(packet)]);
        }

        if self.current_state != SessionState::Connected || self.connected_app_name.is_none() {
            let packet = self.create_error_packet("NetStream.Publish.Start", "Can't publish before connecting", transaction_id, stream_id)?;
            return Ok(vec![ServerSessionResult::OutboundResponse(packet)]);
        }

        let stream_key = match arguments.remove(0) {
            Amf0Value::Utf8String(stream_key) => stream_key,
            _ => {
                let packet = self.create_error_packet("NetStream.Publish.Start", "Invalid publish arguments", transaction_id, stream_id)?;
                return Ok(vec![ServerSessionResult::OutboundResponse(packet)]);
            },
        };

        let mode = match arguments.remove(0) {
            Amf0Value::Utf8String(raw_mode) => {
                match raw_mode.as_ref() {
                    "live" => PublishMode::Live,
                    "append" => PublishMode::Append,
                    "record" => PublishMode::Record,
                    _ => {
                        let error_properties = create_status_object("error", "NetStream.Publish.Start", "Invalid publish mode given");
                        let packet = self.create_error_response(transaction_id,
                                                                Amf0Value::Null,
                                                                vec![Amf0Value::Object(error_properties)],
                                                                stream_id)?;

                        return Ok(vec![ServerSessionResult::OutboundResponse(packet)]);
                    }
                }
            },

            _ => {
                let packet = self.create_error_packet("NetStream.Publish.Start", "Invalid publish arguments", transaction_id, stream_id)?;
                return Ok(vec![ServerSessionResult::OutboundResponse(packet)]);
            },
        };

        let request = OutstandingRequest::PublishRequested {
            stream_key: stream_key.clone(),
            mode: mode.clone(),
            stream_id
        };

        let request_number = self.next_request_number;
        self.next_request_number = self.next_request_number + 1;
        self.outstanding_requests.insert(request_number, request);

        let event = ServerSessionEvent::PublishStreamRequested {
            app_name: match self.connected_app_name {
                Some(ref name) => name.clone(),
                None => unreachable!(), // unreachable due to if check above
            },
            request_id: request_number,
            stream_key,
            mode,
        };

        Ok(vec![ServerSessionResult::RaisedEvent(event)])
    }

    fn handle_amf0_data(&mut self, mut data: Vec<Amf0Value>, stream_id: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        if data.len() == 0 {
            // No data so just do nothing
            return Ok(Vec::new());
        }

        let first_element = data.remove(0);
        match first_element {
            Amf0Value::Utf8String(ref value) if value == "@setDataFrame" => self.handle_amf0_data_set_data_frame(data, stream_id),
            _ => Ok(Vec::new()),
        }
    }

    fn handle_amf0_data_set_data_frame(&mut self, mut data: Vec<Amf0Value>, stream_id: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        if data.len() < 2 {
            // We are expecting a "onMetaData" value and then a property with the actual metadata.  Since
            // this wasn't provided we don't know how to deal with this message.
        }

        match data[0] {
            Amf0Value::Utf8String(ref value) if value == "onMetaData" => (),
            _ => return Ok(Vec::new()),
        }

        if self.connected_app_name.is_none() {
            return Ok(Vec::new()); // setDataFrame has no meaning until they are conneted and publishing
        }

        let publish_stream_key = match self.active_streams.get(&stream_id) {
            Some(ref stream) => {
                match stream.current_state {
                    StreamState::Publishing{ref stream_key, mode: _} => stream_key,
                    _ => return Ok(Vec::new()), // Return nothing since we aren't publishing
                }
            },

            None => return Ok(Vec::new()), // Return nothing since this was not sent on an active stream
        };

        let mut metadata = StreamMetadata::new();
        let object = data.remove(1);
        let properties_option = object.get_object_properties();
        match properties_option {
            Some(properties) => apply_metadata_values(&mut metadata, properties),
            _ => (),
        }

        let event = ServerSessionEvent::StreamMetadataChanged {
            app_name: match self.connected_app_name {
                Some(ref name) => name.clone(),
                None => unreachable!(), // unreachable due to if check above
            },

            stream_key: publish_stream_key.clone(),
            metadata,
        };

        Ok(vec![ServerSessionResult::RaisedEvent(event)])
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

        let description = "Successfully connected on app: ".to_string() + &app_name;
        let mut additional_properties = create_status_object("status", "NetConnection.Connect.Success", description.as_ref());
        additional_properties.insert("objectEncoding".to_string(), Amf0Value::Number(self.object_encoding));

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

    fn accept_publish_request(&mut self, stream_id: u32, stream_key: String, mode: PublishMode) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        match self.active_streams.get_mut(&stream_id) {
            Some(active_stream) => {
                active_stream.current_state = StreamState::Publishing {
                    stream_key: stream_key.clone(),
                    mode,
                };
            },

            None => return Err(ServerSessionError {
                kind: ServerSessionErrorKind::ActionAttemptedOnInactiveStream {
                    action: "publish".to_string(),
                    stream_id,
                }
            }),
        };

        let description = format!("Successfully started publishing on stream key {}", stream_key);
        let status_object = create_status_object("status", "NetStream.Publish.Start", description.as_ref());
        let message = RtmpMessage::Amf0Command {
            command_name: "onStatus".to_string(),
            transaction_id: 0.0,
            command_object: Amf0Value::Null,
            additional_arguments: vec![Amf0Value::Object(status_object)]
        };

        let payload = message.into_message_payload(self.get_epoch(), stream_id)?;
        let packet = self.serializer.serialize(&payload, false, false)?;
        Ok(vec![ServerSessionResult::OutboundResponse(packet)])
    }

    fn create_success_response(&mut self,
        transaction_id: f64,
        command_object: Amf0Value,
        additional_arguments: Vec<Amf0Value>,
        stream_id: u32) -> Result<Packet, ServerSessionError> {

        let message = RtmpMessage::Amf0Command {
            command_name: "_result".to_string(),
            transaction_id,
            command_object,
            additional_arguments
        };

        let payload = message.into_message_payload(self.get_epoch(), stream_id)?;
        let packet = self.serializer.serialize(&payload, false, false)?;
        Ok(packet)
    }

    fn create_error_response(&mut self,
        transaction_id: f64,
        command_object: Amf0Value,
        additional_arguments: Vec<Amf0Value>,
        stream_id: u32) -> Result<Packet, ServerSessionError> {

        let message = RtmpMessage::Amf0Command {
            command_name: "_error".to_string(),
            transaction_id,
            command_object,
            additional_arguments
        };

        let payload = message.into_message_payload(self.get_epoch(), stream_id)?;
        let packet = self.serializer.serialize(&payload, false, false)?;
        Ok(packet)
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

    fn create_error_packet(&mut self, code: &str, description: &str, transaction_id: f64, stream_id: u32) -> Result<Packet, ServerSessionError> {
        let status_object = create_status_object("_error", code, description);
        let packet = self.create_error_response(transaction_id, Amf0Value::Null, vec![Amf0Value::Object(status_object)], stream_id)?;
        Ok(packet)
    }
}

fn create_status_object(level: &str, code: &str, description: &str) -> HashMap<String, Amf0Value> {
    let mut properties = HashMap::new();
    properties.insert("level".to_string(), Amf0Value::Utf8String(level.to_string()));
    properties.insert("code".to_string(), Amf0Value::Utf8String(code.to_string()));
    properties.insert("description".to_string(), Amf0Value::Utf8String(description.to_string()));
    properties
}

fn apply_metadata_values(metadata: &mut StreamMetadata, mut properties: HashMap<String, Amf0Value>) {
    for (key, value) in properties.drain() {
        match key.as_ref() {
            "width" => match value.get_number() { 
                Some(x) => metadata.video_width = Some(x as u32),
                None => (),
            },

            "height" => match value.get_number() {
                Some(x) => metadata.video_height = Some(x as u32),
                None => (),
            },

            "videocodecid" => match value.get_string() {
                Some(x) => metadata.video_codec = Some(x),
                None => (),
            },

            "videodatarate" => match value.get_number() {
                Some(x) => metadata.video_bitrate_kbps = Some(x as u32),
                None => (),
            },

            "framerate" => match value.get_number() {
                Some(x) => metadata.video_frame_rate = Some(x as f32),
                None => (),
            },

            "audiocodecid" => match value.get_string() {
                Some(x) => metadata.audio_codec = Some(x),
                None => (),
            },

            "audiodatarate" => match value.get_number() {
                Some(x) => metadata.audio_bitrate_kbps = Some(x as u32),
                None => (),
            },

            "audiosamplerate" => match value.get_number() {
                Some(x) => metadata.audio_sample_rate = Some(x as u32),
                None => (),
            },

            "audiochannels" => match value.get_number() {
                Some(x) => metadata.audio_channels = Some(x as u32),
                None => (),
            },

            "stereo" => match value.get_boolean() {
                Some(x) => metadata.audio_is_stereo = Some(x),
                None => (),
            },

            "encoder" => match value.get_string() {
                Some(x) => metadata.encoder = Some(x),
                None => (),
            },

            _ => (),
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
}