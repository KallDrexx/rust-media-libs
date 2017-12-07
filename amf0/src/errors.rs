use std::{io, string};

quick_error! {
    #[derive(Debug)]
    pub enum Amf0DeserializationError {
        UnknownMarker(marker: u8) {
            description("Encountered unknown marker")
        }

        UnexpectedEmptyObjectPropertyName {
            description("Unexpected empty object propery name")
        }

        UnexpectedEof {
            description("Hit end of the byte buffer but was expecting more data")
        }

        Io(err: io::Error) {
            cause(err)
            description(err.description())
            from()
        }

        FromUtf8Error(err: string::FromUtf8Error) {
            cause(err)
            description(err.description())
            from()
        }
    }
}

quick_error! {
    #[derive(Debug)]
    pub enum Amf0SerializationError {
        NormalStringTooLong {
            description("String length greater than 65,535")
        }

        Io(err: io::Error) {
            cause(err)
            description(err.description())
            from()
        }
    }
}
