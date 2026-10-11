use std::io::{self, Read, Write};

use serde::{Serialize, de::DeserializeOwned};

const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;
pub const MAX_PRE_AUTH_FRAME_SIZE: usize = 4 * 1024;
// A JSON frame length is at most 16 MiB, so its first byte is 0 or 1. A raw
// stream frame sets the top bit of the first byte, keeps its kind in that
// byte, and has a 24 bit length.
const RAW_FLAG: u8 = 0x80;
const RAW_OUTPUT: u8 = 0x80;
const RAW_START: u8 = 0x81;
const RAW_SIZE: u8 = 0x82;
const RAW_SYNC_END: u8 = 0x83;
const RAW_MARK: u8 = 0x84;
pub const MAX_RAW_OUTPUT: usize = 0xff_ffff;

/// One event of the raw stream of a pane, in the order of the PTY output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RawEvent {
    Output(Vec<u8>),
    /// A new screen of this size. The next output rebuilds the screen.
    Start {
        cols: u16,
        rows: u16,
    },
    Size {
        cols: u16,
        rows: u16,
    },
    /// The runtime ended a synchronized update after its timeout.
    SyncEnd,
    /// The window sends ClientMsg::Parsed with this number when it parsed
    /// all output before it. The answer to a query waits for that.
    Mark(u64),
}

pub enum Incoming<T> {
    Message(T),
    Raw(RawEvent),
}

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
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    decode_body(reader, prefix, max_frame_size)
}

pub fn decode_incoming<R, T>(reader: &mut R) -> io::Result<Incoming<T>>
where
    R: Read,
    T: DeserializeOwned,
{
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    if prefix[0] & RAW_FLAG == 0 {
        return decode_body(reader, prefix, MAX_FRAME_SIZE).map(Incoming::Message);
    }
    let length = u32::from_be_bytes([0, prefix[1], prefix[2], prefix[3]]) as usize;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    raw_event(prefix[0], body).map(Incoming::Raw)
}

pub fn encode_raw(event: &RawEvent) -> io::Result<Vec<u8>> {
    let mark;
    let (kind, body) = match event {
        RawEvent::Output(bytes) => (RAW_OUTPUT, bytes.as_slice()),
        RawEvent::Mark(number) => {
            mark = number.to_be_bytes();
            (RAW_MARK, mark.as_slice())
        }
        RawEvent::Start { cols, rows } => return Ok(raw_size(RAW_START, *cols, *rows)),
        RawEvent::Size { cols, rows } => return Ok(raw_size(RAW_SIZE, *cols, *rows)),
        RawEvent::SyncEnd => (RAW_SYNC_END, [].as_slice()),
    };
    if body.len() > MAX_RAW_OUTPUT {
        return Err(frame_too_large(MAX_RAW_OUTPUT));
    }
    let length = (body.len() as u32).to_be_bytes();
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&[kind, length[1], length[2], length[3]]);
    frame.extend_from_slice(body);
    Ok(frame)
}

fn raw_size(kind: u8, cols: u16, rows: u16) -> Vec<u8> {
    let [c0, c1] = cols.to_be_bytes();
    let [r0, r1] = rows.to_be_bytes();
    vec![kind, 0, 0, 4, c0, c1, r0, r1]
}

fn raw_event(kind: u8, body: Vec<u8>) -> io::Result<RawEvent> {
    match (kind, body.as_slice()) {
        (RAW_OUTPUT, _) => Ok(RawEvent::Output(body)),
        (RAW_START | RAW_SIZE, [c0, c1, r0, r1]) => {
            let (cols, rows) = (
                u16::from_be_bytes([*c0, *c1]),
                u16::from_be_bytes([*r0, *r1]),
            );
            Ok(if kind == RAW_START {
                RawEvent::Start { cols, rows }
            } else {
                RawEvent::Size { cols, rows }
            })
        }
        (RAW_SYNC_END, []) => Ok(RawEvent::SyncEnd),
        (RAW_MARK, mark) if mark.len() == 8 => {
            let mut number = [0; 8];
            number.copy_from_slice(mark);
            Ok(RawEvent::Mark(u64::from_be_bytes(number)))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unknown raw stream frame",
        )),
    }
}

fn decode_body<R, T>(reader: &mut R, prefix: [u8; 4], max_frame_size: usize) -> io::Result<T>
where
    R: Read,
    T: DeserializeOwned,
{
    let max_frame_size = max_frame_size.min(MAX_FRAME_SIZE);
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
