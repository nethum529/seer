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
