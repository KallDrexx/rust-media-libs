/*!

This crate contains APIs that can be used for low-level and high-level implementations
of Adobe's RTMP Protocol.  Most of the code implements the
[1.0 RTMP specification](https://www.adobe.com/content/dam/acom/en/devnet/rtmp/pdf/rtmp_specification_1.0.pdf).
However, some deviations were made by observing real world network traffic seen in modern
RTMP clients and servers.  There is also the more complicated handshaking mechanism that
Adobe Flash requires to play h.264 video (see the `handshake` module for more info).

These APIs are networking library agnostic, and are made to just work against incoming and
outgoing bytes.  This allows RTMP clients and servers to be built with `mio`, `tokio`, or even
std's networking APIs.  They can even be built to run over serial or file I/O if desired
(the latter being useful for debugging).

The API is partitioned into 3 logical areas:

* Handshaking
* Low level APIs
* High level APIs

## Handshaking

Before any RTMP traffic can be sent or received both sides must complete the handshaking process.
This is taken care of by the `Handshake` struct in the `handshake` module.  Regardless of if
you are working with the high or low level APIs the handshaking process must be implemented.

## Low Level APIs

The RTMP protocol has 2 sub-protocols that revolve around the transmission of data over the
network, chunks and messages.  RTMP messages are the actual details of what is being transmitted
(e.g. video data, commands, etc...).  These messages are then wrapped inside of chunks with headers
denoting timestamps, message type, size, etc...

The RTMP chunk format is pretty complicated.  It assumes information about previous chunks it has
received in order to properly read the current chunk, as well as has the tendancy to split a single
RTMP message across multiple chunks.

The `chunk_io` module allows you to serialize RTMP message payloads into packets that can be sent to
the peer, and deserialize incoming bytes representing RTMP chunks into their inner RTMP message
payloads.

The `messages` module allows the unwrapping of inbound message payloads into proper `RtmpMessage`s,
as well as wrapping outbound messages into their payloads (to be then wrapped back into an RTMP
chunk).

## High Level APIs

Part of the RTMP protocol describes particular flows of messages back and forth between the client
and the server.  For example when a client wants to publish a video stream to the server there
are several requests they must make first:

1. Request a connection to an "application"
2. Request to open a stream to publish on
3. Request access to publish with a particular stream name

In reality there is quite a lot of back and forth that goes into these three steps, as well as
more generalized aspects (such as handling pings and window acknowledgements).

To make this easier the `sessions` module contains the high level constructs of a `ClientSession`
and `ServerSession`.  These allow consumers to code against the higher level logic their
application requires and react to events that may be pertinent to them.  It automatically handles
pings, window acknowledgements, and other plumbing that are standard for most RTMP applications.

These higher level structs are meant to be integrated *AFTER* a successful handshaking process.

*/

#[macro_use] extern crate failure;
extern crate byteorder;
extern crate bytes;
extern crate rand;
extern crate ring;
extern crate rml_amf0;

#[cfg(test)]
#[macro_use]
mod test_utils {
    #[macro_use] pub mod assert_vec_match_macro;
    #[macro_use] pub mod assert_vec_contains_macro;
}

pub mod time;
pub mod handshake;
pub mod messages;
pub mod chunk_io;
pub mod sessions;
