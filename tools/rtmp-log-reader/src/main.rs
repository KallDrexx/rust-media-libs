extern crate rml_rtmp;

use std::env;
use std::io::Read;
use std::fs::File;
use rml_rtmp::chunk_io::{ChunkDeserializer};
use rml_rtmp::messages::RtmpMessage;

fn main() {
    println!("RTMP Log reader");
    println!("This reads raw binary logged from a single direction in an RTMP stream,");
    println!("usually logged via one of the Rust Media Libraries example applications");
    println!();

    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        println!("No file specified to read.  Pass the path to the file you wish to read");
        return;
    }

    println!("Reading file: {}", args[1]);
    println!();

    let file_name = args[1].clone();
    let mut file = File::open(file_name).unwrap();
    let mut deserializer = ChunkDeserializer::new();
    let mut message_number = 1;

    // only read one byte at a time to get a byte index for each message
    let mut buffer = [0; 1];
    let mut current_index = 0;
    let mut last_message_end_index = 0;

    loop {
        current_index += 1;
        let bytes_read = file.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            println!("Finished reading log file!");
            return;
        }

        let mut has_read_one_payload = false;
        loop {
            let bytes = if has_read_one_payload {
                &[0_u8; 0]
            } else {
                &buffer[..bytes_read]
            };

            let payload = match deserializer.get_next_message(bytes).unwrap() {
                Some(payload) => payload,
                None => break,
            };

            println!("Message: {}   Timestamp: {}   Type: {}    Stream_Id: {}   index: {} ({:x})",
                     message_number,
                     payload.timestamp.value,
                     payload.type_id,
                     payload.message_stream_id,
                     last_message_end_index,
                     last_message_end_index);

            let message = payload.to_rtmp_message().unwrap();
            match message {
                RtmpMessage::Unknown {type_id, data}
                    => {
                    print!("Unknown {{ type_id: {}, data: ", type_id);
                    for x in 0..data.len() {
                        if x > 100 {
                            print!(".. ({}) ", data.len());
                            break;
                        }

                        print!("{:02x}", data[x]);
                    }
                    println!("}}");
                },

                RtmpMessage::Abort {stream_id}
                    => println!("Abort {{ stream_id: {} }}", stream_id),

                RtmpMessage::Acknowledgement { sequence_number }
                    => println!("Acknowledgement {{ sequence_number: {} }}", sequence_number),

                RtmpMessage::Amf0Command { command_name, transaction_id, command_object, additional_arguments }
                    => println!("Amf0Command {{ command_name: {}, transaction_id: {}, command_object: {:?}, additional_arguments: {:?} }}",
                               command_name, transaction_id, command_object, additional_arguments),

                RtmpMessage::Amf0Data { values }
                    => println!("RtmpMessage::Amf0Data {{ values: {:?} }}", values),

                RtmpMessage::AudioData { data }
                    => {
                    print!("AudioData: {{ data: ");
                    for x in 0..data.len() {
                        if x > 100 {
                            print!(".. ({}) ", data.len());
                            break;
                        }

                        print!("{:02x}", data[x]);
                    }
                    println!("}}", )
                },

                RtmpMessage::SetChunkSize { size }
                    => {
                    deserializer.set_max_chunk_size(size as usize).unwrap();
                    println!("SetChunkSize {{ size: {} }}", size)
                },

                RtmpMessage::SetPeerBandwidth { size, limit_type }
                    => println!("SetPeerBandwidth {{ size: {}, limit_type: {:?} }}", size, limit_type),

                RtmpMessage::UserControl { event_type, stream_id, buffer_length, timestamp }
                    => println!("UserControl {{ event_type: {:?}, stream_id: {:?}, buffer_length: {:?}, timestamp: {:?} }}",
                                event_type, stream_id, buffer_length, timestamp),

                RtmpMessage::VideoData { data }
                    => {
                    print!("VideoData {{ data: ");
                    for x in 0..data.len() {
                        if x > 100 {
                            print!(".. ({}) ", data.len());
                            break;
                        }

                        print!("{:02x}", data[x]);
                    }
                    println!("}}")
                },

                RtmpMessage::WindowAcknowledgement { size }
                    => println!("WindowAcknowledgement {{ size: {} }}", size),
            }

            println!();
            println!("Press enter to read next message");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();

            message_number += 1;
            has_read_one_payload = true;
            last_message_end_index = current_index;
        }
    }
}
