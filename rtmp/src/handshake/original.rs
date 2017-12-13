/// This module allows the handling of original RTMP handshakes, as specified
/// in the official RTMP specification.
///
/// Note that this handshake format does *NOT* work for h.264 video.
/// For h.264 video the digest handshake method must be used.

use super::errors::{HandshakeError};
use super::{HandshakeProcessResult, HandshakeHandler};
use std::io::{Cursor, Write, Read};
use rand;
use rand::Rng;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

const RANDOM_DATA_SIZE: usize = 1528;
const PACKET_SIZE: usize = 8 + RANDOM_DATA_SIZE;

#[derive(Eq, PartialEq, Debug, Clone)]
enum Stage {
    NeedToSendP0AndP1,
    WaitingForPacket0,
    WaitingForPacket1,
    WaitingForPacket2,
    Complete,
}

pub struct Handshake {
    my_epoch: u32,
    current_stage: Stage,
    my_random: [u8; RANDOM_DATA_SIZE],
    buffer: Vec<u8>,
}

impl Handshake {
    /// Creates a new original handshake instance
    pub fn new() -> Handshake {
        Handshake {
            my_epoch: 0,
            current_stage: Stage::NeedToSendP0AndP1,
            my_random: create_random_data(),
            buffer: Vec::new(),
        }
    }

    fn parse_p0(&mut self) -> Result<HandshakeProcessResult, HandshakeError> {
        if self.buffer.len() == 0 {
            return Ok(HandshakeProcessResult::InProgress {response_bytes: Vec::new()});
        }

        match self.buffer.remove(0) {
            3 => {
                self.current_stage = Stage::WaitingForPacket1;
                Ok(HandshakeProcessResult::InProgress {response_bytes: Vec::new()})
            },

            _ => Err(HandshakeError::BadVersionId)
        }
    }

    fn parse_p1(&mut self) -> Result<HandshakeProcessResult, HandshakeError> {
        if self.buffer.len() < PACKET_SIZE {
            return Ok(HandshakeProcessResult::InProgress {response_bytes: Vec::new()});
        }

        let data : Vec<u8> = self.buffer.drain(..PACKET_SIZE).collect();

        // Send the exact 1528 bytes back to complete the handshake (packet 2)
        let mut response = Vec::new();
        response.extend(data);

        self.current_stage = Stage::WaitingForPacket2;

        Ok(HandshakeProcessResult::InProgress {response_bytes: response})
    }

    fn parse_p2(&mut self) -> Result<HandshakeProcessResult, HandshakeError> {
        if self.buffer.len() < PACKET_SIZE {
            return Ok(HandshakeProcessResult::InProgress {response_bytes: Vec::new()});
        }

        let data : Vec<u8> = self.buffer.drain(..).collect();
        let mut cursor = Cursor::new(data);

        let time = cursor.read_u32::<BigEndian>()?;
        if time != self.my_epoch {
            return Err(HandshakeError::IncorrectPeerTime);
        }

        let _ = cursor.read_u32::<BigEndian>()?; // zeros
        let mut random_data = [0_u8; RANDOM_DATA_SIZE];
        let _ = cursor.read(&mut random_data)?;

        if &random_data[..] != &self.my_random[..] {
            return Err(HandshakeError::IncorrectRandomData);
        }

        let mut remaining_bytes = Vec::new();
        let _ = cursor.read_to_end(&mut remaining_bytes)?;

        self.current_stage = Stage::Complete;
        Ok(HandshakeProcessResult::Completed {remaining_bytes})
    }
}

impl HandshakeHandler for Handshake {
    fn generate_outbound_p0_and_p1(&mut self) -> Result<HandshakeProcessResult, HandshakeError> {
        let mut response_bytes = Cursor::new(Vec::new());
        response_bytes.write_u8(3_u8)?;
        response_bytes.write_u32::<BigEndian>(self.my_epoch)?;
        response_bytes.write_u32::<BigEndian>(0)?;
        response_bytes.write(&self.my_random)?;

        self.current_stage = Stage::WaitingForPacket0;
        let result = HandshakeProcessResult::InProgress {
            response_bytes: response_bytes.into_inner()
        };

        Ok(result)
    }

    fn process_bytes(&mut self, data: &[u8]) -> Result<HandshakeProcessResult, HandshakeError> {
        self.buffer.extend_from_slice(data);

        let mut bytes_for_response : Vec<u8> = Vec::new();
        let mut left_over_bytes : Vec<u8> = Vec::new();

        loop {
            let starting_stage = self.current_stage.clone();
            let result = match self.current_stage {
                Stage::NeedToSendP0AndP1 => self.generate_outbound_p0_and_p1(),
                Stage::WaitingForPacket0 => self.parse_p0(),
                Stage::WaitingForPacket1 => self.parse_p1(),
                Stage::WaitingForPacket2 => self.parse_p2(),
                Stage::Complete => Err(HandshakeError::HandshakeAlreadyCompleted)
            };

            match result {
                Err(x) => return Err(x),
                Ok(x) => match x {
                    HandshakeProcessResult::InProgress {response_bytes: bytes} => bytes_for_response.extend(bytes),
                    HandshakeProcessResult::Completed {remaining_bytes: bytes} => left_over_bytes.extend(bytes),
                }
            };

            if self.current_stage == Stage::Complete || starting_stage == self.current_stage {
                // If we are still on the same stage assume that we didn't have
                // enough bytes to process the current packet and wait to try again
                break;
            }
        }

        if self.current_stage == Stage::Complete {
            Ok(HandshakeProcessResult::Completed {remaining_bytes: left_over_bytes})
        } else {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes_for_response})
        }
    }
}

fn create_random_data() -> [u8; RANDOM_DATA_SIZE] {
    let mut random_data = [0_u8;RANDOM_DATA_SIZE];
    let mut rng = rand::thread_rng();
    for x in 0..RANDOM_DATA_SIZE {
        let value = rng.gen();
        random_data[x] = value;
    }

    random_data
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

    #[test]
    fn handshake_starts_with_expected_data()
    {
        let handshake = Handshake::new();
        let is_not_all_zeroes = handshake.my_random.into_iter().any(|&x| x != 0_u8);

        assert_eq!(handshake.current_stage, Stage::NeedToSendP0AndP1);
        assert_eq!(handshake.my_epoch, 0);
        assert_eq!(is_not_all_zeroes, true);
    }

    #[test]
    fn random_data_is_different_for_every_handshake()
    {
        let handshake1 = Handshake::new();
        let handshake2 = Handshake::new();

        assert_ne!(&handshake1.my_random[..], &handshake2.my_random[..]);
    }

    #[test]
    fn can_generate_outbound_p0_and_p1()
    {
        let mut handshake = Handshake::new();
        let data = match handshake.generate_outbound_p0_and_p1() {
            Err(x) => panic!("Unexpected error: {:?}", x),
            Ok(HandshakeProcessResult::InProgress {response_bytes: data}) => data,
            Ok(x) => panic!("Unexpected handshake result: {:?}", x)
        };

        let mut cursor = Cursor::new(data);
        let version = cursor.read_u8().unwrap();
        let time = cursor.read_u32::<BigEndian>().unwrap();
        let _ = cursor.read_u32::<BigEndian>().unwrap();

        let mut random_data = [0_u8; RANDOM_DATA_SIZE];
        cursor.read(&mut random_data).unwrap();

        assert_eq!(handshake.current_stage, Stage::WaitingForPacket0);
        assert_eq!(version, 3);
        assert_eq!(time, handshake.my_epoch);

        for index in 0..RANDOM_DATA_SIZE {
            assert_eq!(random_data[index], handshake.my_random[index]);
        }
    }

    #[test]
    fn gives_error_on_incoming_p0_with_non_3_version()
    {
        let mut handshake = Handshake::new();
        let input = [4_u8];

        let result = handshake.process_bytes(&input);

        match result {
            Ok(_) => panic!("No error was returned"),
            Err(HandshakeError::BadVersionId) => {},
            Err(x) => panic!("Unexpected error received of {:?}", x)
        };
    }

    #[test]
    fn gives_no_error_on_incoming_p0_with_version_of_3()
    {
        let mut handshake = Handshake::new();
        let input = [3_u8];

        let result = handshake.process_bytes(&input);

        match result {
            Ok(_) => {},
            Err(x) => panic!("Unexpected error received of {:?}", x)
        };
    }

    #[test]
    fn sends_outbound_p0_p1_if_p0_received_and_outbound_p0_and_p1_not_yet_sent()
    {
        let mut handshake = Handshake::new();
        let incoming_p0 = [3_u8];

        let result = handshake.process_bytes(&incoming_p0);

        let p0_response = match result {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        let mut cursor = Cursor::new(p0_response);
        let version = cursor.read_u8().unwrap();
        let time = cursor.read_u32::<BigEndian>().unwrap();
        let _ = cursor.read_u32::<BigEndian>().unwrap();

        let mut response_bytes = [0_u8; RANDOM_DATA_SIZE];
        cursor.read(&mut response_bytes).unwrap();

        assert_eq!(handshake.current_stage, Stage::WaitingForPacket1);
        assert_eq!(version, 3);
        assert_eq!(time, handshake.my_epoch);

        for index in 0..RANDOM_DATA_SIZE {
            assert_eq!(response_bytes[index], handshake.my_random[index]);
        }
    }

    #[test]
    fn can_process_p0_and_p1_in_one_call()
    {
        let mut cursor = Cursor::new(Vec::new());

        cursor.write_u8(3_u8).unwrap();
        cursor.write_u32::<BigEndian>(0).unwrap();
        cursor.write_u32::<BigEndian>(0).unwrap();
        cursor.write(&create_random_data()).unwrap();

        let packet = cursor.into_inner();
        println!("packet size: {}", packet.len());

        let mut handshake = Handshake::new();
        let _ = handshake.generate_outbound_p0_and_p1(); // so p0/p1 aren't sent as a response
        let result = handshake.process_bytes(&packet);
        let response = match result {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        assert_eq!(response.len(), PACKET_SIZE, "p2 expected");
        assert_eq!(handshake.current_stage, Stage::WaitingForPacket2);
    }

    #[test]
    fn can_process_full_handshake()
    {
        let mut p1_cursor = Cursor::new(Vec::new());
        p1_cursor.write_u32::<BigEndian>(0).unwrap(); // time
        p1_cursor.write_u32::<BigEndian>(0).unwrap(); // zeros
        p1_cursor.write(&create_random_data()).unwrap();

        let incoming_p0 = [3_u8];
        let incoming_p1 = p1_cursor.into_inner();

        let mut handshake = Handshake::new();
        let outbound_p0_and_p1 = match handshake.generate_outbound_p0_and_p1() {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            Ok(x) => panic!("Unexpected response of {:?}", x),
            Err(x) => panic!("Unexpected error of {:?}", x),
        };

        assert_eq!(handshake.current_stage, Stage::WaitingForPacket0);

        // simulate sending incoming packet 0
        let p0_response = match handshake.process_bytes(&incoming_p0) {
            Ok(HandshakeProcessResult::InProgress {response_bytes: data}) => data,
            Ok(x) => panic!("Unexpected response of {:?}", x),
            Err(x) => panic!("Unexpected error of {:?}", x),
        };

        assert_eq!(p0_response.len(), 0, "Non-empty p0 response found");
        assert_eq!(handshake.current_stage, Stage::WaitingForPacket1);

        // simulate sending incoming packet 1
        let outbound_p2 = match handshake.process_bytes(&incoming_p1) {
            Ok(HandshakeProcessResult::InProgress {response_bytes: data}) => data,
            Ok(x) => panic!("Unexpected response of {:?}", x),
            Err(x) => panic!("Unexpected error of {:?}", x),
        };

        // outbound p2 must match incoming p1
        assert_eq!(&outbound_p2[..], &incoming_p1[..]);
        assert_eq!(handshake.current_stage, Stage::WaitingForPacket2);

        // simulate sending incoming packet 2
        let mut cursor = Cursor::new(Vec::new());
        cursor.write(&outbound_p0_and_p1[1..]).unwrap();

        let packet = cursor.into_inner();

        match handshake.process_bytes(&packet) {
            Ok(HandshakeProcessResult::Completed {remaining_bytes: data}) => {assert_eq!(data, Vec::new())},
            Ok(x) => panic!("Unexpected response of {:?}", x),
            Err(x) => panic!("Unexpected error of {:?}", x),
        }

        assert_eq!(handshake.current_stage, Stage::Complete);
    }

    #[test]
    fn extra_p2_bytes_returned_on_handshake_completion()
    {
        let extra_bytes = [5_u8; 10];

        let mut handshake = Handshake::new();
        let outbound_p0_and_p1 = match handshake.generate_outbound_p0_and_p1() {
            Err(x) => panic!("Unexpected error: {:?}", x),
            Ok(HandshakeProcessResult::InProgress {response_bytes: data}) => data,
            Ok(x) => panic!("Unexpected handshake result: {:?}", x)
        };

        let mut p1_cursor = Cursor::new(Vec::new());
        p1_cursor.write_u32::<BigEndian>(0).unwrap(); // time
        p1_cursor.write_u32::<BigEndian>(0).unwrap(); // zeros
        p1_cursor.write(&create_random_data()).unwrap();

        let mut p2_cursor = Cursor::new(Vec::new());
        p2_cursor.write(&outbound_p0_and_p1[1..]).unwrap();
        p2_cursor.write(&extra_bytes).unwrap();

        let incoming_p0 = [3_u8];
        let incoming_p1 = p1_cursor.into_inner();
        let incoming_p2 = p2_cursor.into_inner();

        match handshake.process_bytes(&incoming_p0) {
            Ok(HandshakeProcessResult::InProgress {response_bytes: _}) => (),
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        match handshake.process_bytes(&incoming_p1) {
            Ok(HandshakeProcessResult::InProgress {response_bytes: _}) => (),
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        let result = handshake.process_bytes(&incoming_p2);
        let response = match result {
            Ok(HandshakeProcessResult::Completed {remaining_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        assert_eq!(response.len(), extra_bytes.len());
        assert_eq!(&response[..], &extra_bytes[..]);
    }

    #[test]
    fn two_handshakes_can_talk_to_each_other()
    {
        let mut handshake1 = Handshake::new();
        let mut handshake2 = Handshake::new();

        let h1_p0_and_p1 = match handshake1.generate_outbound_p0_and_p1() {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        let h2_p0_and_p1 = match handshake2.generate_outbound_p0_and_p1() {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        let h1_p2 = match handshake1.process_bytes(&h2_p0_and_p1) {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        let h2_p2 = match handshake2.process_bytes(&h1_p0_and_p1) {
            Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => bytes,
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        match handshake1.process_bytes(&h2_p2) {
            Ok(HandshakeProcessResult::Completed {remaining_bytes: _}) => {},
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        match handshake2.process_bytes(&h1_p2) {
            Ok(HandshakeProcessResult::Completed {remaining_bytes: _}) => {},
            x => panic!("Unexpected process_bytes response: {:?}", x),
        };

        assert_eq!(handshake1.current_stage, Stage::Complete);
        assert_eq!(handshake2.current_stage, Stage::Complete);
    }
}
