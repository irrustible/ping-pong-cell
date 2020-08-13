#![feature(test)]

use futures_micro::{sleep, waker};
use futures_lite::*;
use ping_pong_cell::PingPongCell;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::thread::spawn;

pub fn oneshot<T: Send>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner { cell: PingPongCell::new(None) });
    let sender = Sender { inner: Some(inner.clone()) };
    let receiver = Receiver { inner: Some(inner) };
    (sender, receiver)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Closed();

#[derive(Debug)]
pub enum WaitError {
    AlreadyAwaited(Waker),
}

pub struct Sender<T: Send> {
    inner: Option<Arc<Inner<T>>>,
}

// Can be polled as a Future to wait for a receiver to be listening.
impl<T: Send> Sender<T> {

    /// Closes the channel by causing an immediate drop
    pub fn close(self) { }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    pub async fn wait(self) -> Result<Self, Closed> {
        while let Some(ref inner) = &self.inner {
            let wake_me = waker().await;
            let ret = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => Poll::Ready(Err(Closed())),
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Waker(waker));
                        Poll::Ready(Ok(()))
                    }
                    _ => {
                        *state = Some(State::Waker(wake_me));
                        Poll::Pending
                    }
                }
            });
            match ret {
                Poll::Pending => sleep().await,
                Poll::Ready(Ok(())) => { return Ok(self); }
                Poll::Ready(Err(Closed())) => {return Err(Closed()); }
            }
        }
        Err(Closed())
    }

    /// Sends a message on the channel
    pub fn send(self, value: T) -> Result<(), Closed> {
        if let Some(ref inner) = &self.inner {
            let ret = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => Err(Closed()),
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Ready(value));
                        Ok(Some(waker))
                    }
                    _ => {
                        *state = Some(State::Ready(value));
                        Ok(None)
                    }
                }
            });
            match ret {
                Ok(Some(waker)) => {
                    waker.wake();
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(e) => Err(e),
            }
        } else {
            Err(Closed()) // not sure how you got here tbh
        }
    }
}

pub struct Receiver<T: Send> {
    inner: Option<Arc<Inner<T>>>,
}

impl<T: Send> Receiver<T> {
    /// Closes the channel by causing an immediate drop
    pub fn close(self) { }
}

enum State<T> {
    Waker(Waker),
    Ready(T),
    Closed,
}

struct Inner<T> {
    cell: PingPongCell<State<T>>,
}

impl<T: Send> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            let ret = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Closed);
                        Some(waker)
                    }
                    // Could be Ready or Closed, either is fine
                    Some(other) => {
                        *state = Some(other);
                        None
                    }
                    None => {
                        *state = Some(State::Closed);
                        None
                    }
                }
            });
            if let Some(waker) = ret {
                waker.wake();
            }
        }
    }
}

impl<T: Send> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            let ret = inner.cell.transact(|state| {
                if let Some(State::Waker(waker)) = state.take() {
                    *state = Some(State::Closed);
                    Some(waker)
                } else {
                    *state = Some(State::Closed);
                    None
                }
            });
            if let Some(waker) = ret {
                waker.wake();
            }
        }
    }
}

impl<T: Send> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        if let Some(inner) = &this.inner {
            inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => Poll::Ready(Err(Closed())),
                    Some(State::Ready(value)) => Poll::Ready(Ok(value)),
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Waker(context.waker().clone()));
                        waker.wake();
                        Poll::Pending
                    }
                    None => {
                        *state = Some(State::Waker(context.waker().clone()));
                        Poll::Pending
                    }
                }
            })
        } else {
            Poll::Ready(Err(Closed()))
        }
    }
}


#[test]
fn success_one_thread() {
    let (s,r) = oneshot::<bool>();
    assert_eq!((), s.send(true).unwrap());
    assert_eq!(Ok(true), future::block_on(r));
}

#[test]
fn close_sender_one_thread() {
    let (s,r) = oneshot::<bool>();
    s.close();
    assert_eq!(Err(Closed()), future::block_on(r));
}

#[test]
fn close_receiver_one_thread() {
    let (s,r) = oneshot::<bool>();
    r.close();
    assert_eq!(Err(Closed()), s.send(true));
}

#[test]
fn success_two_threads() {
    let (s,r) = oneshot::<bool>();
    let t = spawn(|| future::block_on(r));
    assert_eq!((), s.send(true).unwrap());
    assert_eq!(Ok(true), t.join().unwrap());
}

#[test]
fn close_sender_two_threads() {
    let (s,r) = oneshot::<bool>();
    let j = spawn(|| future::block_on(r));
    s.close();
    assert_eq!(Err(Closed()), j.join().unwrap());
}

#[test]
fn wait_for_receiver() {
    let (s,r) = oneshot::<bool>();
    let j = spawn(move || future::block_on(async {
        let s = s.wait().await?;
        s.send(true)
    }));
    assert_eq!(Ok(true), future::block_on(r));
    assert_eq!(Ok(()), j.join().unwrap());
}

