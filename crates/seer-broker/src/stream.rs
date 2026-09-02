use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use seer_net::Stream;

#[derive(Clone)]
pub(crate) enum BrokerStream {
    Tcp(Arc<TcpStream>),
    Iroh(Arc<UnixStream>),
}

impl From<TcpStream> for BrokerStream {
    fn from(stream: TcpStream) -> Self {
        Self::Tcp(Arc::new(stream))
    }
}

impl From<UnixStream> for BrokerStream {
    fn from(stream: UnixStream) -> Self {
        Self::Iroh(Arc::new(stream))
    }
}

impl Read for BrokerStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => Read::read(&mut stream.as_ref(), buffer),
            Self::Iroh(stream) => Read::read(&mut stream.as_ref(), buffer),
        }
    }
}

impl Write for BrokerStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => Write::write(&mut stream.as_ref(), buffer),
            Self::Iroh(stream) => Write::write(&mut stream.as_ref(), buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => Write::flush(&mut stream.as_ref()),
            Self::Iroh(stream) => Write::flush(&mut stream.as_ref()),
        }
    }
}

impl Stream for BrokerStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.set_read_timeout(timeout),
            Self::Iroh(stream) => stream.set_read_timeout(timeout),
        }
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.set_write_timeout(timeout),
            Self::Iroh(stream) => stream.set_write_timeout(timeout),
        }
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.shutdown(how),
            Self::Iroh(stream) => stream.shutdown(how),
        }
    }
}
