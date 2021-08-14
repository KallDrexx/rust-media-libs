pub struct PlayerDetails {
    pub connection_id: i32,
    pub has_received_video_keyframe: bool,
}

impl PlayerDetails {
    pub fn new(connection_id: i32) -> Self {
        PlayerDetails {
            connection_id,
            has_received_video_keyframe: false,
        }
    }
}
