mod errors;

use std::io::{Cursor, Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use rand;
use rand::Rng;
use self::errors::HandshakeError;

#[derive(Eq,PartialEq, Debug)]
enum State {
    WaitingForPacket0,
    WaitingForPacket1,
    WaitingForPacket2,
    Successful
}

enum ParsedPacket{
    Incomplete,
    Valid {time: u32, time2: u32, random: [u8; 1528]}
}

#[derive(PartialEq, Eq, Debug)]
pub struct Response(Vec<u8>);

pub struct Handshake {
    pub my_epoch: u32,
    pub their_epoch: u32,
    pub is_completed: bool,

    current_state: State,
    my_random: [u8; 1528],
    their_random: [u8; 1528],
    buffer: Vec<u8>
}

impl Handshake {
    pub fn new() -> Result<(Handshake, Response), HandshakeError> {
        let my_epoch = 0;
        let random_data = create_random_data();
        let mut response_bytes = vec![3_u8];
        let mut packet1 = create_packet_bytes(my_epoch, 0, &random_data)?;
        response_bytes.append(&mut packet1);

        let handshake = Handshake {
            is_completed: false,
            current_state: State::WaitingForPacket0,
            my_epoch,
            their_epoch: 0,
            my_random: random_data,
            their_random: [0; 1528],
            buffer: Vec::new()
        };

        Ok((handshake, Response(response_bytes)))
    }

    pub fn process_bytes(&mut self, data: &[u8]) -> Result<Response, HandshakeError> {
        for index in 0..data.len() {
            self.buffer.push(data[index].clone());
        }

        let result = match self.current_state {
            State::WaitingForPacket0 => process_packet_0(self),
            State::WaitingForPacket1 => process_packet_1(self),
            State::WaitingForPacket2 => process_packet_2(self),
            State::Successful => Ok(Response(vec![]))
        };

        if result.is_ok() && self.buffer.len() >= 1528 {
            self.process_bytes(&[])
        }
        else {
            result
        }
    }
}

fn process_packet_0(handshake: &mut Handshake) -> Result<Response, HandshakeError> {
    if handshake.buffer.len() < 1 {
        return Ok(Response(vec![]));
    }

    if handshake.buffer[0] != 3 {
        return Err(HandshakeError::BadVersionId)
    }

    handshake.current_state = State::WaitingForPacket1;
    handshake.buffer.remove(0);
    Ok(Response(vec![]))
}

fn create_random_data() -> [u8; 1528] {
    let mut random_data = [0_u8;1528];     
    let mut rng = rand::thread_rng();
    for x in 0..1528 {
        let value = rng.gen();
        random_data[x] = value;
    }

    random_data
}

fn process_packet_1(handshake: &mut Handshake) -> Result<Response, HandshakeError> {
    let packet = parse_packet(&mut handshake.buffer)?;
    match packet {
        ParsedPacket::Incomplete => Ok(Response(vec![])),
        ParsedPacket::Valid{time, time2, random} => {
            if time2 != 0 {
                return Err(HandshakeError::NonZeroedTimeInPacket1)
            }

            handshake.their_epoch = time;
            handshake.their_random = random;
            handshake.current_state = State::WaitingForPacket2;

            // Form outgoing packet 2
            let data = create_packet_bytes(handshake.their_epoch, handshake.my_epoch, &random)?;
            Ok(Response(data))
        }
    }
}

fn process_packet_2(handshake: &mut Handshake) -> Result<Response, HandshakeError> {
    let packet = parse_packet(&mut handshake.buffer)?;
    match packet {
        ParsedPacket::Incomplete => Ok(Response(vec![])),
        ParsedPacket::Valid{time, time2: _, random} => {
            if time != handshake.my_epoch {
                return Err(HandshakeError::IncorrectPeerTime)
            }

            for index in 0..1528 {
                if random[index] != handshake.my_random[index] {
                    println!("Index {} expects {} found {}", index, random[index], handshake.my_random[index]);
                    return Err(HandshakeError::IncorrectRandomData);
                }
            }

            handshake.is_completed = true;
            handshake.current_state = State::Successful;
            Ok(Response(vec![]))
        }
    }
}

fn create_packet_bytes(time1: u32, time2: u32, random: &[u8; 1528]) -> Result<Vec<u8>, HandshakeError> {
    let mut response_bytes = Cursor::new(Vec::new());
    response_bytes.write_u32::<BigEndian>(time1)?;
    response_bytes.write_u32::<BigEndian>(time2)?;
    response_bytes.write(random)?;

    Ok(response_bytes.into_inner())
}

fn parse_packet(bytes: &mut Vec<u8>) -> Result<ParsedPacket, HandshakeError> {
    if bytes.len() < 1536 {
        return Ok(ParsedPacket::Incomplete);
    }

    let packet: Vec<u8> = bytes.drain(0..1536).collect();
    let mut cursor = Cursor::new(packet);
    let mut random = [0_u8; 1528];
    let time = cursor.read_u32::<BigEndian>()?;
    let time2 = cursor.read_u32::<BigEndian>()?;
    cursor.read(&mut random)?;

    Ok(ParsedPacket::Valid {
        time,
        time2,
        random
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{State};
    use std::io::{Cursor, Read, Write};
    use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
    use rand;
    use rand::Rng;
    use super::errors::HandshakeError;

    #[test]
    fn starts_in_waitingforpacket0_stage() {
        let (handshake, _) = Handshake::new().unwrap();
        assert_eq!(handshake.current_state, State::WaitingForPacket0);
    }

    #[test]
    fn returns_valid_packet_0_and_1_response() {
        let (handshake, Response(data)) = Handshake::new().unwrap();
        assert_eq!(data.len(), 1537);

        let mut cursor = Cursor::new(data);
        let version = cursor.read_u8().unwrap();
        let time = cursor.read_u32::<BigEndian>().unwrap();
        let zeros = cursor.read_u32::<BigEndian>().unwrap();

        let mut random = [0_u8; 1528];
        cursor.read(&mut random).unwrap();

        assert_eq!(version, 3);
        assert_eq!(time, handshake.my_epoch);
        assert_eq!(zeros, 0);

        for index in 0..1528 {
            assert_eq!(random[index], handshake.my_random[index]);
        }        
    }

    #[test]
    fn accepts_packet_0_if_its_value_is_3() {
        let (mut handshake, _) = Handshake::new().unwrap();
        let data = create_packet_0(3);
        let Response(bytes) = handshake.process_bytes(&data).unwrap();

        assert_eq!(handshake.current_state, State::WaitingForPacket1);
        assert_eq!(bytes, vec![]);
    }

    #[test]
    fn returns_error_if_invalid_version_value_processed() {
        let (mut handshake, _) = Handshake::new().unwrap();
        let data = create_packet_0(4);
        match handshake.process_bytes(&data) {
            Err(HandshakeError::BadVersionId) => assert!(true),
            Ok(_) => assert!(false, "Expected an error but received an Ok"),
            Err(x) => assert!(false, "Expected BadVersionId error, instead recieved {}", x)
        }
    }

    #[test]
    fn accepts_valid_packet_1_and_returns_packet_2_response() {
        let epoch = 15;
        let (mut handshake, _) = Handshake::new().unwrap();
        let p0 = create_packet_0(3);
        let (p1, local_random) = create_packet_1(epoch);

        handshake.process_bytes(&p0).unwrap();
        let Response(mut bytes) = handshake.process_bytes(&p1).unwrap();
        let packet = super::parse_packet(&mut bytes).unwrap();
        let (time, time2, random) = match packet {
            super::ParsedPacket::Incomplete => panic!("Incomplete packet parsed"),
            super::ParsedPacket::Valid {time, time2, random} => (time, time2, random)
        };

        assert_eq!(handshake.current_state, State::WaitingForPacket2);
        assert_eq!(epoch, time);
        assert_eq!(0, time2);

        for index in 0..1528 {
            if random[index] != local_random[index] {
                assert!(false, "Expected random byte {} to be {} but found {}", index, local_random[index], random[index]);
            }
        }
    }

    #[test]
    fn accepts_valid_handshake_packets() {
        let epoch = 15;
        let (mut handshake, _) = Handshake::new().unwrap();
        let p0 = create_packet_0(3);
        let (p1, local_random) = create_packet_1(epoch);
        let p2 = create_packet_2(handshake.my_epoch, epoch, &handshake.my_random);

        // packet 0 test
        handshake.process_bytes(&p0).unwrap();

        // Packet 1 test
        let Response(mut bytes) = handshake.process_bytes(&p1).unwrap();
        let packet = super::parse_packet(&mut bytes).unwrap();
        let (time, time2, random) = match packet {
            super::ParsedPacket::Incomplete => panic!("Incomplete packet parsed"),
            super::ParsedPacket::Valid {time, time2, random} => (time, time2, random)
        };

        assert_eq!(handshake.current_state, State::WaitingForPacket2);
        assert_eq!(epoch, time);
        assert_eq!(0, time2);

        for index in 0..1528 {
            if random[index] != local_random[index] {
                assert!(false, "Expected random byte {} to be {} but found {}", index, local_random[index], random[index]);
            }
        }

        // Packet 2 test
        let Response(bytes) = handshake.process_bytes(&p2).unwrap();
        assert_eq!(0, bytes.len());
        assert!(handshake.is_completed, "Handshake not marked as completed");
    }

    #[test]
    fn error_when_bad_time_in_packet_2() {
        let epoch = 15;
        let (mut handshake, _) = Handshake::new().unwrap();
        let p0 = create_packet_0(3);
        let (p1, _) = create_packet_1(epoch);
        let p2 = create_packet_2(handshake.my_epoch + 1, epoch, &handshake.my_random);

        handshake.process_bytes(&p0).unwrap();
        handshake.process_bytes(&p1).unwrap();
        match handshake.process_bytes(&p2) {
            Err(HandshakeError::IncorrectPeerTime) => assert!(true),
            Ok(_) => panic!("Expected HandshakeError::IncorrectPeerTime but got Ok()"),
            Err(x) => panic!("Expected HandshakeError::IncorrectPeerTime but got Err({})", x)
        }
    }

    #[test]
    fn error_when_bad_random_data_in_packet_2() {
        let epoch = 15;
        let (mut handshake, _) = Handshake::new().unwrap();
        let mut random_copy = [0_u8; 1528];
        for index in 0..1528 {
            random_copy[index] = handshake.my_random[index]
        }

        random_copy[0] = random_copy[0] + 1;

        let p0 = create_packet_0(3);
        let (p1, _) = create_packet_1(epoch);
        let p2 = create_packet_2(handshake.my_epoch, epoch, &random_copy);

        handshake.process_bytes(&p0).unwrap();
        handshake.process_bytes(&p1).unwrap();
        match handshake.process_bytes(&p2) {
            Err(HandshakeError::IncorrectRandomData) => assert!(true),
            Ok(_) => panic!("Expected HandshakeError::IncorrectRandomData but got Ok()"),
            Err(x) => panic!("Expected HandshakeError::IncorrectRandomData but got Err({})", x)
        }
    }

    #[test]
    fn two_handshake_instances_can_successfully_complete_against_each_other() {
        let (mut server, Response(s0_and_s1)) = Handshake::new().unwrap();
        let (mut client, Response(c0_and_c1)) = Handshake::new().unwrap();

        let Response(s2) = server.process_bytes(&c0_and_c1).unwrap();
        let Response(c2) = client.process_bytes(&s0_and_s1).unwrap();

        server.process_bytes(&c2).unwrap();
        client.process_bytes(&s2).unwrap();

        assert!(server.is_completed, "server not completed");
        assert!(client.is_completed, "client not completed");
    }

    fn create_packet_0(version_id: u8) -> Vec<u8> {
        vec![version_id]
    }

    fn create_packet_1(epoch: u32) -> (Vec<u8>, [u8; 1528])  {
        let mut bytes = Cursor::new(Vec::new());
        bytes.write_u32::<BigEndian>(epoch).unwrap();
        bytes.write_u32::<BigEndian>(0).unwrap(); 

        let mut random_data = [0_u8;1528];     
        let mut rng = rand::thread_rng();
        for x in 0..1528 {
            let value = rng.gen();
            random_data[x] = value;
        }

        bytes.write(&random_data).unwrap();
        (bytes.into_inner(), random_data)
    }

    fn create_packet_2(epoch: u32, epoch2: u32, random: &[u8; 1528]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        bytes.write_u32::<BigEndian>(epoch).unwrap();
        bytes.write_u32::<BigEndian>(epoch2).unwrap();
        bytes.write(random).unwrap();

        bytes.into_inner()
    }
}