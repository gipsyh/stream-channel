#![feature(cursor_remaining)]
#![doc = include_str!("../README.md")]

use std::{
    io::{self, Cursor, Read, Write},
    sync::mpsc::{channel, Receiver, Sender},
};

pub mod async_sc;

pub struct StreamChannelRead {
    receiver: Receiver<Vec<u8>>,
    reader: Option<Cursor<Vec<u8>>>,
}

impl StreamChannelRead {
    fn new(receiver: Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            reader: None,
        }
    }
}

impl Read for StreamChannelRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.reader.is_none() || self.reader.as_ref().unwrap().is_empty() {
            self.reader =
                Some(Cursor::new(self.receiver.recv().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("{}", e))
                })?));
        }
        std::io::Read::read(&mut self.reader.as_mut().unwrap(), buf)
    }
}

pub struct StreamChannelWrite {
    sender: Sender<Vec<u8>>,
    writer: Vec<u8>,
}

impl StreamChannelWrite {
    fn new(sender: Sender<Vec<u8>>) -> Self {
        Self {
            sender,
            writer: vec![],
        }
    }

    pub fn write_data_in_vec(&mut self, vec: Vec<u8>) -> io::Result<()> {
        self.flush()?;
        self.sender
            .send(vec)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
    }
}

impl Write for StreamChannelWrite {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.writer.is_empty() {
            return Ok(());
        }
        let send = self.writer.to_owned();
        self.writer = vec![];
        self.sender
            .send(send)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
    }
}

pub struct StreamChannel {
    read: StreamChannelRead,
    write: StreamChannelWrite,
}

impl StreamChannel {
    pub fn new() -> (Self, Self) {
        let (ls, rr) = channel();
        let (rs, lr) = channel();
        let l = Self {
            read: StreamChannelRead::new(lr),
            write: StreamChannelWrite::new(ls),
        };
        let r = Self {
            read: StreamChannelRead::new(rr),
            write: StreamChannelWrite::new(rs),
        };
        (l, r)
    }

    pub fn write_data_in_vec(&mut self, vec: Vec<u8>) -> io::Result<()> {
        self.write.write_data_in_vec(vec)
    }

    pub fn split(self) -> (StreamChannelRead, StreamChannelWrite) {
        (self.read, self.write)
    }
}

impl Read for StreamChannel {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::io::Read::read(&mut self.read, buf)
    }
}

impl Write for StreamChannel {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write.writer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.write.writer.is_empty() {
            return Ok(());
        }
        let send = self.write.writer.to_owned();
        self.write.writer = vec![];
        self.write
            .sender
            .send(send)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
    }
}

#[cfg(test)]
mod tests {
    use crate::StreamChannel;
    use std::io::Write;

    #[test]
    fn test() {
        let (mut l, mut r) = StreamChannel::new();
        let send = vec![1, 2, 3, 4];
        l.write(&send).unwrap();
        l.flush().unwrap();
        let send = vec![5, 6, 7, 8];
        l.write(&send).unwrap();
        l.flush().unwrap();
        let mut recv = vec![0; 2];
        std::io::Read::read_exact(&mut r, recv.as_mut()).unwrap();
        assert_eq!(recv, vec![1, 2]);
        std::io::Read::read_exact(&mut r, recv.as_mut()).unwrap();
        assert_eq!(recv, vec![3, 4]);
        std::io::Read::read_exact(&mut r, recv.as_mut()).unwrap();
        assert_eq!(recv, vec![5, 6]);
        std::io::Read::read_exact(&mut r, recv.as_mut()).unwrap();
        assert_eq!(recv, vec![7, 8]);
    }
}
