use std::{
    self,
    io::{self, Cursor},
    task::Poll,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::mpsc::{channel, Receiver, Sender},
};

pub struct StreamChannel {
    sender: Sender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
    reader: Option<Cursor<Vec<u8>>>,
    writer: Vec<u8>,
}

impl StreamChannel {
    pub fn new(buffer: usize) -> (Self, Self) {
        let (ls, rr) = channel(buffer);
        let (rs, lr) = channel(buffer);
        let l = Self {
            sender: ls,
            receiver: lr,
            reader: None,
            writer: vec![],
        };
        let r = Self {
            sender: rs,
            receiver: rr,
            reader: None,
            writer: vec![],
        };
        (l, r)
    }

    pub async fn write_data_in_vec(&mut self, vec: Vec<u8>) {
        self.flush().await.unwrap();
        self.sender.send(vec).await.unwrap();
    }
}

impl AsyncRead for StreamChannel {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let sc = self.get_mut();
        if sc.reader.is_none() || sc.reader.as_ref().unwrap().is_empty() {
            sc.reader = match sc.receiver.poll_recv(cx) {
                Poll::Ready(vec) => Some(Cursor::new(vec.unwrap())),
                Poll::Pending => return Poll::Pending,
            };
        }
        let sz = std::io::Read::read(&mut sc.reader.as_mut().unwrap(), buf.initialize_unfilled())
            .unwrap();
        if sz > 0 {
            buf.advance(sz);
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

impl AsyncWrite for StreamChannel {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let sc = self.get_mut();
        sc.writer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let sc = self.get_mut();
        if sc.writer.is_empty() {
            return Poll::Ready(Ok(()));
        }
        let send = sc.writer.to_owned();
        sc.writer = vec![];
        match sc.sender.try_send(send) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(_) => Poll::Pending,
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::tokio::StreamChannel;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test() {
        let (mut l, mut r) = StreamChannel::new(100);
        let send = vec![1, 2, 3, 4];
        l.write(&send).await.unwrap();
        l.flush().await.unwrap();
        let send = vec![5, 6, 7, 8];
        l.write(&send).await.unwrap();
        l.flush().await.unwrap();
        let mut recv = vec![0; 2];
        r.read_exact(recv.as_mut()).await.unwrap();
        assert_eq!(recv, vec![1, 2]);
        r.read_exact(recv.as_mut()).await.unwrap();
        assert_eq!(recv, vec![3, 4]);
        r.read_exact(recv.as_mut()).await.unwrap();
        assert_eq!(recv, vec![5, 6]);
        r.read_exact(recv.as_mut()).await.unwrap();
        assert_eq!(recv, vec![7, 8]);
        r.read_exact(recv.as_mut()).await.unwrap();
    }
}
