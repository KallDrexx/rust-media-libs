This crate provides functions for interacting with the Adobe RTMP protocol.  It contains both low-level and high-level
abstractions that can be utilized to integrate RTMP support into clients (publisher and players) and servers.

## Documentation

https://docs.rs/rml_rtmp/

## Installation

This crate works with Cargo and is on [crates.io](http://crates.io).  Add it to your `Cargo.toml` like so:

```toml
[dependencies]
rml_rtmp = "0.1"
```

## Performance

The intention is that this library is as high performance as possible.  We use the `Bytes` crate to keep allocations as 
minimal as possible, and according to the [video relay benchmark](../benchmarks/video-relay) on my Ryzen 5 1600X (on a single 
thread) I can relay 10KB video packets from one publisher to two subscribers with an average of 24 microseconds.  This should
leave ample cpu cycles for custom logic and for it to run on lower end devices.

## Examples

Two large examples can be found in the repository:

* [Threaded RTMP Server](https://github.com/KallDrexx/rust-media-libs/tree/master/examples/threaded_rtmp_server) - This
is a basic RTMP server that shows how to accept connections and route audio/video to players

* [Mio RTMP Server](https://github.com/KallDrexx/rust-media-libs/tree/master/examples/mio_rtmp_server) - This is a
relatively advanced RTMP server that shows how to integrate both RTMP `ClientSession`s and `ServerSession`s into a mio
application.  It supports
    * Receiving video from publishers and routing that video to any subscribed players
    * Pull video from a remote RTMP server and serve it to subscribed players
    * Receiving video from a publisher and republish that video out to an external RTMP server