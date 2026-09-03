use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub enum Socket {
    Tcp(Arc<TcpStream>),
    Iroh(Arc<UnixStream>),
}

impl From<TcpStream> for Socket {
    fn from(stream: TcpStream) -> Self {
        Self::Tcp(Arc::new(stream))
    }
}

impl From<UnixStream> for Socket {
    fn from(stream: UnixStream) -> Self {
        Self::Iroh(Arc::new(stream))
    }
}

impl Read for Socket {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => Read::read(&mut stream.as_ref(), buffer),
            Self::Iroh(stream) => Read::read(&mut stream.as_ref(), buffer),
        }
    }
}

impl Write for Socket {
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

pub trait Stream: Read + Write + Send + 'static {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn shutdown(&self, how: Shutdown) -> io::Result<()>;
}

impl Stream for TcpStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, timeout)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_write_timeout(self, timeout)
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        TcpStream::shutdown(self, how)
    }
}

impl Stream for UnixStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        UnixStream::set_read_timeout(self, timeout)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        UnixStream::set_write_timeout(self, timeout)
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        UnixStream::shutdown(self, how)
    }
}

impl Stream for Socket {
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
