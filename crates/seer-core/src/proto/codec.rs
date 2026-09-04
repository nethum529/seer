use std::io::{self, Read, Write};

use serde::{Serialize, de::DeserializeOwned};

const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;
pub const MAX_PRE_AUTH_FRAME_SIZE: usize = 4 * 1024;

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
    decode_with_limit(reader, MAX_FRAME_SIZE)
}

pub fn decode_with_limit<R, T>(reader: &mut R, max_frame_size: usize) -> io::Result<T>
where
    R: Read,
    T: DeserializeOwned,
{
    let max_frame_size = max_frame_size.min(MAX_FRAME_SIZE);
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length > max_frame_size {
        return Err(frame_too_large_with_limit(max_frame_size));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(invalid_data)
}

fn frame_too_large_with_limit(max_frame_size: usize) -> io::Error {
    let size = match max_frame_size {
        MAX_FRAME_SIZE => "16 MiB".to_owned(),
        MAX_PRE_AUTH_FRAME_SIZE => "4 KiB".to_owned(),
        _ => format!("{max_frame_size} bytes"),
    };
    io::Error::new(io::ErrorKind::InvalidData, format!("frame exceeds {size}"))
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

    use super::{MAX_FRAME_SIZE, MAX_PRE_AUTH_FRAME_SIZE, decode, decode_with_limit, encode};
    use crate::proto::ClientMsg;

    #[test]
    fn decode_rejects_oversized_frame() {
        let length = u32::try_from(MAX_FRAME_SIZE + 1).expect("limit must fit in u32");
        let error = decode::<_, ClientMsg>(&mut length.to_be_bytes().as_slice())
            .expect_err("frame is too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "frame exceeds 16 MiB");

        let value = "a".repeat(MAX_FRAME_SIZE - 2);
        let mut frame = Vec::with_capacity(MAX_FRAME_SIZE + 4);
        frame.extend_from_slice(&(MAX_FRAME_SIZE as u32).to_be_bytes());
        frame.push(b'"');
        frame.extend_from_slice(value.as_bytes());
        frame.push(b'"');

        let decoded = decode::<_, String>(&mut frame.as_slice()).expect("frame must decode");
        assert_eq!(decoded, value);

        let message = "a".repeat(MAX_FRAME_SIZE - 1);
        assert!(encode(&mut Vec::new(), &message).is_err());
    }

    #[test]
    fn decode_with_limit_rejects_oversized_pre_auth_frame() {
        let length = u32::try_from(MAX_PRE_AUTH_FRAME_SIZE + 1).expect("limit must fit in u32");
        let error = decode_with_limit::<_, ClientMsg>(
            &mut length.to_be_bytes().as_slice(),
            MAX_PRE_AUTH_FRAME_SIZE,
        )
        .expect_err("pre-auth frame is too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "frame exceeds 4 KiB");
    }
}
