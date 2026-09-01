use std::io::{self, Read};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, codec};

const RECEIVE_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_millis(10);

pub(crate) fn receive(stream: &mut TcpStream) -> ClientMsg {
    let mut reader = DeadlineReader {
        inner: stream,
        deadline: Instant::now() + RECEIVE_TIMEOUT,
    };
    codec::decode(&mut reader).expect("client message must decode")
}

struct DeadlineReader<R> {
    inner: R,
    deadline: Instant,
}

impl<R: Read> Read for DeadlineReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.inner.read(buffer) {
                Err(error) if is_timeout(&error) && Instant::now() < self.deadline => {
                    thread::sleep(RETRY_DELAY);
                }
                Err(error) if is_timeout(&error) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "server receive timed out",
                    ));
                }
                result => return result,
            }
        }
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TimeoutThenByte {
        kind: Option<io::ErrorKind>,
    }

    impl Read for TimeoutThenByte {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if let Some(kind) = self.kind.take() {
                return Err(io::Error::from(kind));
            }
            buffer[0] = 7;
            Ok(1)
        }
    }

    #[test]
    fn retries_both_socket_timeout_kinds() {
        for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
            let mut reader = DeadlineReader {
                inner: TimeoutThenByte { kind: Some(kind) },
                deadline: Instant::now() + Duration::from_secs(1),
            };
            let mut byte = [0];

            reader.read_exact(&mut byte).expect("read must retry");

            assert_eq!(byte, [7]);
        }
    }

    #[test]
    fn stops_retrying_at_the_deadline() {
        let mut reader = DeadlineReader {
            inner: TimeoutThenByte {
                kind: Some(io::ErrorKind::WouldBlock),
            },
            deadline: Instant::now(),
        };

        let error = reader.read(&mut [0]).expect_err("read must time out");

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "server receive timed out");
    }
}
