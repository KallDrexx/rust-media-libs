use rml_amf0::Amf0Value;

/// Events that can be raised by the client session so that custom business logic can be written
/// to react to it
#[derive(PartialEq, Debug)]
pub enum ClientSessionEvent {
    /// Raised when a connection request has been accepted by the server
    ConnectionRequestAccepted,

    /// The server has rejected the connection request
    ConnectionRequestRejected {
        description: String,
    },

    /// The server sent an Amf0 command that was not able to be handled
    UnhandleableAmf0Command {
        command_name: String,
        transaction_id: f64,
        command_object: Amf0Value,
        additional_values: Vec<Amf0Value>,
    },

    /// The server sent us a result to a transaction that we don't know about
    UnknownTransactionResultReceived {
        transaction_id: f64,
        command_object: Amf0Value,
        additional_values: Vec<Amf0Value>,
    },
}