#[derive(PartialEq, Debug, Clone)]
pub enum State {
    Waiting,
    Connected { app_name: String },
    PublishRequested { app_name: String, stream_key: String, request_id: u32 },
    Publishing { app_name: String, stream_key: String },
    PlaybackRequested { app_name: String, stream_key: String, request_id: u32, stream_id: u32 },
    Playing { app_name: String, stream_key: String, stream_id: u32 },
}
