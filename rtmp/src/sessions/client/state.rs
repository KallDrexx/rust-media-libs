pub enum ClientState {
    /// Client has not connected to an application on the server yet,
    Disconnected,

    /// The client has connected to the specified application on the server
    Connected { app_name: String },
}