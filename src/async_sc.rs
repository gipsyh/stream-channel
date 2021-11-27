use std::{io::Cursor, pin::Pin, task::Poll};
use tokio::{
    io::{self, AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::mpsc::{channel, Receiver},
};
use tokio_util::sync::PollSender;
pub struct StreamChannelRead {
    receiver: Receiver<Box<[u8]>>,
    reader: Option<Cursor<Box<[u8]>>>,
}

impl StreamChannelRead {
    fn new(receiver: Receiver<Box<[u8]>>) -> Self {
        Self {
            receiver,
            reader: None,
        }
    }

    pub async fn read_slice(&mut self) -> io::Result<Box<[u8]>> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| io::ErrorKind::Other.into())
    }
}

impl AsyncRead for StreamChannelRead {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let sc = self.get_mut();
        if sc.reader.is_none() || sc.reader.as_ref().unwrap().is_empty() {
            sc.reader = match sc.receiver.poll_recv(cx) {
                Poll::Ready(data) => match data {
                    Some(data) => Some(Cursor::new(data)),
                    None => return Poll::Ready(Err(io::ErrorKind::Other.into())),
                },
                Poll::Pending => return Poll::Pending,
            }
        }
        let sz = std::io::Read::read(&mut sc.reader.as_mut().unwrap(), buf.initialize_unfilled())
            .unwrap();
        if sz > 0 {
            buf.advance(sz);
            Poll::Ready(Ok(()))
        } else {
            panic!()
        }
    }
}

pub struct StreamChannelWrite {
    sender: PollSender<Box<[u8]>>,
    writer: Vec<u8>,
}

impl StreamChannelWrite {
    fn new(sender: PollSender<Box<[u8]>>) -> Self {
        Self {
            sender,
            writer: vec![],
        }
    }

    pub async fn write_slice(&mut self, data: Box<[u8]>) -> io::Result<()> {
        self.flush().await?;
        self.sender
            .inner_ref()
            .ok_or(io::ErrorKind::Other)?
            .send(data)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
    }
}

impl AsyncWrite for StreamChannelWrite {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        self.get_mut().writer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let inner = self.get_mut();
        match inner.sender.poll_send_done(cx) {
            Poll::Ready(res) => match res {
                Ok(_) => {
                    if inner.writer.is_empty() {
                        return Poll::Ready(Ok(()));
                    }
                    let data = std::mem::take(&mut inner.writer).into_boxed_slice();
                    if let Err(e) = inner.sender.start_send(data) {
                        Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)))
                    } else {
                        inner
                            .sender
                            .poll_send_done(cx)
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
                    }
                }
                Err(e) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        todo!()
    }
}

pub struct StreamChannel {
    read: StreamChannelRead,
    write: StreamChannelWrite,
}

impl StreamChannel {
    pub fn new() -> (Self, Self) {
        let (ls, rr) = channel(1024);
        let (rs, lr) = channel(1024);
        let ls = PollSender::new(ls);
        let rs = PollSender::new(rs);
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

    pub fn split(self) -> (StreamChannelRead, StreamChannelWrite) {
        (self.read, self.write)
    }

    pub async fn write_slice(&mut self, data: Box<[u8]>) -> io::Result<()> {
        self.write.write_slice(data).await
    }

    pub async fn read_slice(&mut self) -> io::Result<Box<[u8]>> {
        self.read.read_slice().await
    }
}

impl AsyncRead for StreamChannel {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().read).poll_read(cx, buf)
    }
}

impl AsyncWrite for StreamChannel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use crate::async_sc::StreamChannel;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn async_test1() {
        let (mut l, mut r) = StreamChannel::new();
        let send = vec![1, 2, 3, 4];
        l.write(&send).await.unwrap();
        l.flush().await.unwrap();
        let mut recv = vec![0; 2];
        r.read_exact(recv.as_mut()).await.unwrap();
        assert_eq!(recv, vec![1, 2]);
        r.read_exact(recv.as_mut()).await.unwrap();
        assert_eq!(recv, vec![3, 4]);
    }

    #[tokio::test]
    async fn async_test2() {
        let (mut l, mut r) = StreamChannel::new();
        let send = vec![1, 2, 3, 4];
        l.write_slice(send.into_boxed_slice()).await.unwrap();
        l.flush().await.unwrap();
        let data = r.read_slice().await.unwrap();
        assert_eq!(Vec::from(data), vec![1, 2, 3, 4]);
    }
}
