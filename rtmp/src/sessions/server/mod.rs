mod events;
mod config;
mod errors;
mod result;

use std::time::SystemTime;
use rml_amf0::Amf0Value;
use ::chunk_io::{ChunkSerializer, ChunkDeserializer};
use ::messages::{RtmpMessage, UserControlEventType, PeerBandwidthLimitType};
use ::time::RtmpTimestamp;

pub use self::errors::{ServerSessionError, ServerSessionErrorKind};
pub use self::events::ServerSessionEvent;
pub use self::config::ServerSessionConfig;
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
    pub fn accept_request(&mut self, _request_id: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        unimplemented!()
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

    fn handle_amf0_command(&self, _name: String, _transaction_id: f64, _command_object: Amf0Value, _additional_args: Vec<Amf0Value>)
        -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_amf0_data(&self, _data: Vec<Amf0Value>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_audio_data(&self, _data: Vec<u8>) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
        Ok(Vec::new())
    }

    fn handle_set_chunk_size(&self, _size: u32) -> Result<Vec<ServerSessionResult>, ServerSessionError> {
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
    use rml_amf0::Amf0Value;
    use ::messages::{RtmpMessage, PeerBandwidthLimitType, UserControlEventType};
    use ::chunk_io::{ChunkDeserializer};

    const DEFAULT_CHUNK_SIZE: u32 = 1111;
    const DEFAULT_PEER_BANDWIDTH: u32 = 2222;
    const DEFAULT_WINDOW_ACK_SIZE: u32 = 3333;

    #[test]
    fn new_config_creates_initial_responses() {
        let config = get_basic_config();
        let mut deserializer = ChunkDeserializer::new();
        let (_, results) = ServerSession::new(config).unwrap();

        let responses = get_rtmp_messages(&mut deserializer, &results);

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

    fn get_basic_config() -> ServerSessionConfig {
        ServerSessionConfig {
            chunk_size: DEFAULT_CHUNK_SIZE,
            fms_version: "fms_version".to_string(),
            peer_bandwidth: DEFAULT_PEER_BANDWIDTH,
            window_ack_size: DEFAULT_WINDOW_ACK_SIZE,
        }
    }

    fn get_rtmp_messages(deserializer: &mut ChunkDeserializer, results: &Vec<ServerSessionResult>) -> Vec<RtmpMessage> {
        let mut responses = Vec::new();
        for result in results.iter() {
            match *result {
                ServerSessionResult::OutboundResponse(ref packet) => {
                    let payload = deserializer.get_next_message(&packet.bytes[..]).unwrap().unwrap();
                    let message = payload.to_rtmp_message().unwrap();
                    responses.push(message);
                }
                _ => (),
            };
        }

        responses
    }
}