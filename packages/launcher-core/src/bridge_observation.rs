//! Payload-free bridge diagnostics. No protocol parsing or lifecycle policy.
use std::{io, pin::Pin, sync::Arc, task::{Context, Poll}};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Local, Upstream }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal { ReadEof, ReadError, WriteError, FlushError, ShutdownError }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub side: Side,
    pub terminal: Terminal,
    pub category: &'static str,
    pub bytes_read: u64,
    pub bytes_written: u64,
}
pub type Observer = Arc<dyn Fn(Event) + Send + Sync>;

fn category(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::ConnectionReset => "reset",
        io::ErrorKind::ConnectionAborted => "aborted",
        io::ErrorKind::BrokenPipe => "broken_pipe",
        io::ErrorKind::TimedOut => "timeout",
        io::ErrorKind::UnexpectedEof => "unexpected_eof",
        io::ErrorKind::NotConnected => "not_connected",
        _ => "other",
    }
}

struct Observed<T> {
    inner: T, side: Side, notify: Observer,
    read_terminal: bool, write_terminal: bool, flush_terminal: bool, shutdown_terminal: bool,
    bytes_read: u64, bytes_written: u64,
}
impl<T> Observed<T> {
    fn new(inner:T,side:Side,notify:Observer)->Self {
        Self{inner,side,notify,read_terminal:false,write_terminal:false,flush_terminal:false,shutdown_terminal:false,bytes_read:0,bytes_written:0}
    }
    fn emit(&self,terminal:Terminal,category:&'static str) {
        (self.notify)(Event{side:self.side,terminal,category,bytes_read:self.bytes_read,bytes_written:self.bytes_written});
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for Observed<T> {
    fn poll_read(self:Pin<&mut Self>,cx:&mut Context<'_>,buf:&mut ReadBuf<'_>)->Poll<io::Result<()>> {
        let this=self.get_mut();let before=buf.filled().len();let room=buf.remaining();
        let result=Pin::new(&mut this.inner).poll_read(cx,buf);
        match &result {
            Poll::Ready(Ok(())) => {
                let count=buf.filled().len()-before;
                this.bytes_read=this.bytes_read.saturating_add(count as u64);
                if room>0 && count==0 && !this.read_terminal {
                    this.read_terminal=true;this.emit(Terminal::ReadEof,"eof");
                }
            }
            Poll::Ready(Err(err)) if !this.read_terminal => {
                this.read_terminal=true;this.emit(Terminal::ReadError,category(err.kind()));
            }
            _=>{}
        }
        result
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for Observed<T> {
    fn poll_write(self:Pin<&mut Self>,cx:&mut Context<'_>,buf:&[u8])->Poll<io::Result<usize>> {
        let this=self.get_mut();let result=Pin::new(&mut this.inner).poll_write(cx,buf);
        match &result {
            Poll::Ready(Ok(count))=>this.bytes_written=this.bytes_written.saturating_add(*count as u64),
            Poll::Ready(Err(err)) if !this.write_terminal=>{
                this.write_terminal=true;this.emit(Terminal::WriteError,category(err.kind()));
            }
            _=>{}
        }
        result
    }
    fn poll_flush(self:Pin<&mut Self>,cx:&mut Context<'_>)->Poll<io::Result<()>> {
        let this=self.get_mut();let result=Pin::new(&mut this.inner).poll_flush(cx);
        if let Poll::Ready(Err(err))=&result {
            if !this.flush_terminal {this.flush_terminal=true;this.emit(Terminal::FlushError,category(err.kind()));}
        }
        result
    }
    fn poll_shutdown(self:Pin<&mut Self>,cx:&mut Context<'_>)->Poll<io::Result<()>> {
        let this=self.get_mut();let result=Pin::new(&mut this.inner).poll_shutdown(cx);
        if let Poll::Ready(Err(err))=&result {
            if !this.shutdown_terminal {this.shutdown_terminal=true;this.emit(Terminal::ShutdownError,category(err.kind()));}
        }
        result
    }
}

/// Same Tokio relay as production, with observation-only read/write wrappers.
/// Caller's cancellation/close ownership remains unchanged. Observer must be a
/// short non-panicking metadata sink; never pass identity or payload to it.
pub async fn copy_observed<A,B>(a:&mut A,b:&mut B,notify:Observer)->io::Result<(u64,u64)>
where A: AsyncRead+AsyncWrite+Unpin, B: AsyncRead+AsyncWrite+Unpin {
    let mut a=Observed::new(a,Side::Local,notify.clone());
    let mut b=Observed::new(b,Side::Upstream,notify);
    tokio::io::copy_bidirectional(&mut a,&mut b).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    fn log()->(Arc<Mutex<Vec<Event>>>,Observer) {
        let events=Arc::new(Mutex::new(Vec::new()));let copy=events.clone();
        (events,Arc::new(move|event|copy.lock().unwrap().push(event)))
    }
    #[tokio::test]
    async fn upstream_eof_does_not_discard_local_half_close_data() {
        let (mut local,mut relay_local)=tokio::io::duplex(3);
        let (mut upstream,mut relay_upstream)=tokio::io::duplex(3);
        let (events,sink)=log();
        let task=tokio::spawn(async move{copy_observed(&mut relay_local,&mut relay_upstream,sink).await.unwrap()});
        upstream.write_all(b"reply").await.unwrap();upstream.shutdown().await.unwrap();
        let mut reply=Vec::new();local.read_to_end(&mut reply).await.unwrap();assert_eq!(reply,b"reply");
        local.write_all(b"after-eof").await.unwrap();local.shutdown().await.unwrap();
        let mut request=Vec::new();upstream.read_to_end(&mut request).await.unwrap();assert_eq!(request,b"after-eof");
        assert_eq!(task.await.unwrap(),(9,5));
        let events=events.lock().unwrap();assert_eq!(events.len(),2);
        assert_eq!(events[0].side,Side::Upstream);assert_eq!(events[0].terminal,Terminal::ReadEof);
        assert_eq!(events[1].side,Side::Local);assert_eq!(events[1].bytes_read,9);
    }
    #[tokio::test]
    async fn local_eof_first_is_identified_and_response_preserved() {
        let (mut local,mut relay_local)=tokio::io::duplex(64);
        let (mut upstream,mut relay_upstream)=tokio::io::duplex(64);
        let (events,sink)=log();
        let task=tokio::spawn(async move{copy_observed(&mut relay_local,&mut relay_upstream,sink).await.unwrap()});
        local.write_all(b"request").await.unwrap();local.shutdown().await.unwrap();
        let mut request=Vec::new();upstream.read_to_end(&mut request).await.unwrap();assert_eq!(request,b"request");
        upstream.write_all(b"response").await.unwrap();upstream.shutdown().await.unwrap();
        let mut reply=Vec::new();local.read_to_end(&mut reply).await.unwrap();assert_eq!(reply,b"response");
        assert_eq!(task.await.unwrap(),(7,8));
        let events=events.lock().unwrap();assert_eq!(events[0].side,Side::Local);assert_eq!(events[1].side,Side::Upstream);
    }
    #[tokio::test]
    async fn zero_capacity_read_is_not_reported_as_eof() {
        let stream=&b"not-eof"[..];let(events,sink)=log();let mut observed=Observed::new(stream,Side::Local,sink);
        observed.read(&mut []).await.unwrap();assert!(events.lock().unwrap().is_empty());
    }
    struct Reset;
    impl AsyncRead for Reset {
        fn poll_read(self:Pin<&mut Self>,_:&mut Context<'_>,_:&mut ReadBuf<'_>)->Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::ConnectionReset,"raw details must never be logged")))
        }
    }
    impl AsyncWrite for Reset {
        fn poll_write(self:Pin<&mut Self>,_:&mut Context<'_>,buf:&[u8])->Poll<io::Result<usize>>{Poll::Ready(Ok(buf.len()))}
        fn poll_flush(self:Pin<&mut Self>,_:&mut Context<'_>)->Poll<io::Result<()>>{Poll::Ready(Ok(()))}
        fn poll_shutdown(self:Pin<&mut Self>,_:&mut Context<'_>)->Poll<io::Result<()>>{Poll::Ready(Ok(()))}
    }
    #[tokio::test]
    async fn read_reset_origin_is_preserved_without_error_message() {
        let mut local=Reset;let (_peer,mut upstream)=tokio::io::duplex(8);let(events,sink)=log();
        let err=copy_observed(&mut local,&mut upstream,sink).await.unwrap_err();assert_eq!(err.kind(),io::ErrorKind::ConnectionReset);
        let events=events.lock().unwrap();assert_eq!(events.len(),1);assert_eq!(events[0].side,Side::Local);
        assert_eq!(events[0].category,"reset");assert!(!format!("{:?}",events[0]).contains("raw details"));
    }
}
