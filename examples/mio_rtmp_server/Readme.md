# Mio Rtmp Server

The Mio RTMP server is an example of how to create a `mio` based application that supports the RTMP protocol.  It acts
as both a client and server depending on the circumstances.

Encoders can connect to the server and publish video (tested with OBS and Ffmpeg).  Video players can then connect
to those same streams and watch the live video that is being published.

The server also supports pulling live video from an external RTMP server (and relaying that video to actively
subscribed players).  It also supports taking a video stream that is being published to it by an encoder and
republishing that video stream to an external RTMP server.  In that way it shows how consumers can create applications
that act as RTMP publishing clients and playback clients.

## Usage

The most basic usage to start an RTMP server is to use `cargo run`

```
cargo run
    Finished dev [unoptimized + debuginfo] target(s) in 0.0 secs
     Running `\target\debug\mio-rtmp-server.exe`
Application options: AppOptions { log_io: false, pull: None, push: None }
Listening for connections
```

Once you see `Listening for connections` you are free to connect any RTMP encoders or players as desired.

There are several command line options that can be provided:

* `--log-io` - If provided it will log all raw binary that the RTMP server sends and receives to files.  The files
are split on a per connection per direction basis, so each connection will result in 2 files.
* `pull -a <app> -h <host> -s <stream> -t <target>` - When provided the server will immediately create a client that
will connect to an RTMP server at the specified IP address, request a connection to the specified application name,
then request playback for the source stream name.  Assuming the external server does not reject our request all audio
and video it receives will be routed to players subscribed to the target stream name on the local server (this application).
* `push push -a <app> -h <host> -s <source_stream> -t <target_stream>` - When provided and an encoder starts pushing video
to the source stream, the server will create a client that will connect to the RTMP server at the specified IP address,
connect to the specified application name, and request publishing to the target stream.  If accepted all video from
the local encoder will be relayed to the external RTMP server by this application.

2 things to note:

* IP addresses must be used for push and pull arguments.  This is just due to me not bothering to add in dns resolution.
* Pulled feeds cannot be automatically pushed out to a different RTMP server.  This is not due to any technical limitation
but mostly due to me not wanting to copy and paste the relay logic into the pull code flow, but it isn't difficult to do.