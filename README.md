# rust-media-libs
Rust based libraries for misc media functionality

## License
This project is distributed under the terms of both MIT license and the Apache License (Version 2.0).

## Libraries
There are currently 2 supported libraries in this project:

* **[rml_amf0](amf0)** - Crate supporting the serialization and deserialization of amf0 encoded data.
* **[rml_rtmp](rtmp)** - Crate providing high and low level APIs for supporting the Adobe RTMP protocol.

## Examples
Several examples have been created that utilize these libraries

* **[tokio_rtmp_server](examples/tokio_rtmp_server)** - This is an example of using the library to create an RTMP server
with async rust and Tokio.  Clients can connect, publish video to a stream, and other clients can connect and play the
stream back.  

* **[mio_rtmp_server](examples/mio_rtmp_server)** - This is a semi-advanced example of creating a mio application that
can act as both a client and a server.  It supports:
    * Clients can connect and publish video to a stream.
    * Clients can connect and play video that is being published to a stream.
    * The server can pull live video from a remote server and relay the video stream to subscribed players.
    * The server can take a video stream that a client is publishing and republish that out to another RTMP server.

* **[threaded_rtmp_server](examples/threaded_rtmp_server)** - This is a very simple RTMP server that allows clients
to publish video and players to watch video.

## Tools
Several tools are provided in this repository:

* **[rtmp-log-reader](tools/rtmp-log-reader)** - Allows the reading of raw RTMP binary that are encoded in a file.  This
is used for debugging RTMP conversations between two parties.

* **[handshake-tester](tools/handshake-tester)** - Tool to verify handshaking can be performed with another RTMP server.

