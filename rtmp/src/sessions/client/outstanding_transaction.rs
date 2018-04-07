
pub enum TransactionPurpose {
    PlayRequest {
        stream_key: String,
    },
}

pub enum OutstandingTransaction {
    ConnectionRequested {
        app_name: String,
    },

    CreateStream {
        purpose: TransactionPurpose,
    },
}