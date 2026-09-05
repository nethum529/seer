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
        return Err(frame_too_large(MAX_FRAME_SIZE));
    }
    let length = body.len() as u32;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&body);
    writer.write_all(&frame)
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
        return Err(frame_too_large(max_frame_size));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(invalid_data)
}

fn frame_too_large(max_frame_size: usize) -> io::Error {
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

#[cfg(test)]
mod tests {
    use std::io;

    use super::{MAX_FRAME_SIZE, decode, encode};
    use crate::proto::ClientMsg;

    #[test]
    fn cell_frames_keep_text_styles_and_cursor_through_the_wire() {
        use crate::{Cell, Color, Cursor, TerminalFrame, TerminalModes};
        let cell = Cell {
            character: 'x',
            fg: Color::Indexed(2),
            bg: Color::Rgb {
                red: 1,
                green: 2,
                blue: 3,
            },
            bold: true,
            italic: true,
            underline: true,
            dim: true,
            inverse: true,
            hidden: true,
            strikeout: true,
        };
        let mut row = vec![cell; 80];
        row[40].character = '\u{754c}';
        row[41].character = ' ';
        let frame = TerminalFrame {
            rows: vec![row; 24],
            cursor: Cursor {
                row: 2,
                column: 40,
                visible: true,
                ..Cursor::default()
            },
            modes: TerminalModes::default(),
        };
        let mut bytes = Vec::new();
        encode(&mut bytes, &frame).unwrap();
        assert_eq!(
            decode::<_, TerminalFrame>(&mut bytes.as_slice()).unwrap(),
            frame
        );
    }

    #[test]
    fn decode_rejects_oversized_frame() {
        let length = u32::try_from(MAX_FRAME_SIZE + 1).expect("limit must fit in u32");
        let error = decode::<_, ClientMsg>(&mut length.to_be_bytes().as_slice())
            .expect_err("frame is too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

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
}
