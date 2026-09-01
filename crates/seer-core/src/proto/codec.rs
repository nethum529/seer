use std::io::{self, Read, Write};

use serde::{Serialize, de::DeserializeOwned};

const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

pub fn encode<W, T>(writer: &mut W, message: &T) -> io::Result<()>
where
    W: Write,
    T: Serialize,
{
    let body = serde_json::to_vec(message).map_err(invalid_data)?;
    if body.len() > MAX_FRAME_SIZE {
        return Err(frame_too_large());
    }
    let length = body.len() as u32;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&body)
}

pub fn decode<R, T>(reader: &mut R) -> io::Result<T>
where
    R: Read,
    T: DeserializeOwned,
{
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length > MAX_FRAME_SIZE {
        return Err(frame_too_large());
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(invalid_data)
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn frame_too_large() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "frame exceeds 16 MiB")
}

#[cfg(test)]
mod tests {
    use std::io;

    use serde::Serialize;
    use serde::ser::{Error, Serializer};

    use super::{MAX_FRAME_SIZE, decode, encode};
    use crate::proto::ClientMsg;

    struct InvalidMessage;

    impl Serialize for InvalidMessage {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(S::Error::custom("invalid message"))
        }
    }

    #[test]
    fn decode_rejects_truncated_frame() {
        let frames = [vec![0, 0, 0], vec![0, 0, 0, 4, b'n', b'u']];

        for frame in frames {
            let error =
                decode::<_, ClientMsg>(&mut frame.as_slice()).expect_err("frame is truncated");
            assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        }
    }

    #[test]
    fn decode_rejects_oversized_frame() {
        let length = u32::try_from(MAX_FRAME_SIZE + 1).expect("limit must fit in u32");
        let error = decode::<_, ClientMsg>(&mut length.to_be_bytes().as_slice())
            .expect_err("frame is too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "frame exceeds 16 MiB");
    }

    #[test]
    fn decode_accepts_maximum_frame() {
        let value = "a".repeat(MAX_FRAME_SIZE - 2);
        let mut frame = Vec::with_capacity(MAX_FRAME_SIZE + 4);
        frame.extend_from_slice(&(MAX_FRAME_SIZE as u32).to_be_bytes());
        frame.push(b'"');
        frame.extend_from_slice(value.as_bytes());
        frame.push(b'"');

        let decoded = decode::<_, String>(&mut frame.as_slice()).expect("frame must decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn decode_rejects_invalid_json() {
        let frame = [0, 0, 0, 1, b'{'];
        let error = decode::<_, ClientMsg>(&mut frame.as_slice()).expect_err("JSON is invalid");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn encode_rejects_oversized_frame() {
        let message = "a".repeat(MAX_FRAME_SIZE - 1);
        let error = encode(&mut Vec::new(), &message).expect_err("frame is too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "frame exceeds 16 MiB");
    }

    #[test]
    fn encode_reports_serialization_failure() {
        let error = encode(&mut Vec::new(), &InvalidMessage).expect_err("message is invalid");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
