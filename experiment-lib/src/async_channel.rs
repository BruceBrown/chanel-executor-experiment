use parking_lot::Mutex;
use slab::Slab;
use std::fmt;
/// This is bare-bones. It is just enough to allow crossbeam to operate in async.
/// There may be some significant omissions here, which could lead to crases.
/// This implements async send and recv as futures with the underlying channels
/// being wrapped crossbeam channels. All that being said, for our limited case,
/// it seems to work well.
///
/// It is built upon futures 0.3
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crossbeam::channel::{RecvError, TryRecvError, TrySendError};

struct WakerSet {
    slab: Mutex<Slab<Option<Waker>>>,
}
impl WakerSet {
    fn new() -> Self {
        Self {
            slab: Mutex::new(Slab::with_capacity(4)),
        }
    }
    fn insert(&self, ctx: &Context<'_>) -> usize {
        let w = ctx.waker().clone();
        let key = self.slab.lock().insert(Some(w));
        key
    }
    const fn cancel(&self, _key: usize) {}
    fn remove(&self, key: usize) { self.slab.lock().remove(key); }
    fn notify(&self) -> bool {
        let mut slab = self.slab.lock();
        for (_, opt_waker) in slab.iter_mut() {
            if let Some(w) = opt_waker.take() {
                w.wake();
                return true;
            }
        }
        false
    }
}
pub fn bounded<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::with_capacity(cap));
    let s = Sender { channel: channel.clone() };
    let r = Receiver {
        channel,
        // opt_key: None,
    };
    (s, r)
}

pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::new());
    let s = Sender { channel: channel.clone() };
    let r = Receiver {
        channel,
        // opt_key: None,
    };
    (s, r)
}

pub struct Sender<T> {
    /// The inner channel.
    channel: Arc<Channel<T>>,
}
impl<T> Sender<T> {
    pub async fn send(&self, msg: T) {
        struct SendFuture<'a, T> {
            channel: &'a Channel<T>,
            msg: Option<T>,
            opt_key: Option<usize>,
        }
        impl<T> Unpin for SendFuture<'_, T> {}
        impl<T> Future for SendFuture<'_, T> {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                loop {
                    let msg = self.msg.take().unwrap();

                    // If the current task is in the set, remove it.
                    if let Some(key) = self.opt_key.take() {
                        self.channel.send_wakers.remove(key);
                    }

                    // Try sending the message.
                    match self.channel.try_send(msg) {
                        Ok(()) => {
                            self.channel.recv_wakers.notify();
                            return Poll::Ready(());
                        },
                        Err(TrySendError::Disconnected(msg)) => {
                            self.msg = Some(msg);
                            return Poll::Pending;
                        },
                        Err(TrySendError::Full(msg)) => {
                            self.msg = Some(msg);
                            // Insert this send operation.
                            self.opt_key = Some(self.channel.send_wakers.insert(cx));
                            // If the channel is still full and not disconnected, return.
                            if self.channel.is_full() && !self.channel.is_disconnected() {
                                return Poll::Pending;
                            }
                        },
                    }
                }
            }
        }
        impl<T> Drop for SendFuture<'_, T> {
            fn drop(&mut self) {
                // If the current task is still in the set, that means it is being cancelled now.

                // Wake up another task instead.

                if let Some(key) = self.opt_key {
                    // fixme: fix this
                    self.channel.send_wakers.cancel(key);
                }
            }
        }
        SendFuture {
            channel: &self.channel,
            msg: Some(msg),
            opt_key: None,
        }
        .await
    }
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> { self.channel.try_send(msg) }
    pub fn capacity(&self) -> Option<usize> { self.channel.capacity() }
    pub fn is_empty(&self) -> bool { self.channel.is_empty() }
    pub fn is_full(&self) -> bool { self.channel.is_full() }
    pub fn len(&self) -> usize { self.channel.len() }
}
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Decrement the sender count and disconnect the channel if it drops down to zero.
        if self.channel.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.channel.disconnect();
        }
    }
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        let count = self.channel.sender_count.fetch_add(1, Ordering::Relaxed);

        // Make sure the count never overflows, even if lots of sender clones are leaked.
        if count > isize::MAX as usize {
            process::abort();
        }

        Sender {
            channel: self.channel.clone(),
        }
    }
}
impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.pad("Sender { .. }") }
}
pub struct Receiver<T> {
    /// The inner channel.
    channel: Arc<Channel<T>>,
    /* The key for this receiver in the `channel.stream_wakers` set.
     * opt_key: Option<usize>, */
}
impl<T> Receiver<T> {
    pub async fn recv(&self) -> Result<T, RecvError> {
        struct RecvFuture<'a, T> {
            channel: &'a Channel<T>,
            opt_key: Option<usize>,
        }
        impl<T> Future for RecvFuture<'_, T> {
            type Output = Result<T, RecvError>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                poll_recv(self.channel, &self.channel.recv_wakers, &mut self.opt_key, cx)
            }
        }
        impl<T> Drop for RecvFuture<'_, T> {
            fn drop(&mut self) {
                // If the current task is still in the set, that means it is being cancelled now.
                if let Some(key) = self.opt_key {
                    self.channel.recv_wakers.cancel(key);
                }
            }
        }

        RecvFuture {
            channel: &self.channel,
            opt_key: None,
        }
        .await
    }
    pub fn try_recv(&self) -> Result<T, TryRecvError> { self.channel.try_recv() }
    pub fn capacity(&self) -> Option<usize> { self.channel.capacity() }
    pub fn is_empty(&self) -> bool { self.channel.is_empty() }
    pub fn is_full(&self) -> bool { self.channel.is_full() }
    pub fn len(&self) -> usize { self.channel.len() }
}
fn poll_recv<T>(channel: &Channel<T>, wakers: &WakerSet, opt_key: &mut Option<usize>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
    loop {
        // If the current task is in the set, remove it.
        if let Some(key) = opt_key.take() {
            wakers.remove(key);
        }

        // Try receiving a message.
        match channel.try_recv() {
            Ok(msg) => {
                channel.send_wakers.notify();
                return Poll::Ready(Ok(msg));
            },
            Err(TryRecvError::Disconnected) => return Poll::Ready(Err(RecvError {})),
            Err(TryRecvError::Empty) => {
                // Insert this receive operation.
                *opt_key = Some(wakers.insert(cx));

                // If the channel is still empty and not disconnected, return.
                if channel.is_empty() && !channel.is_disconnected() {
                    return Poll::Pending;
                }
            },
        }
    }
}
impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let count = self.channel.receiver_count.fetch_add(1, Ordering::Relaxed);
        // Make sure the count never overflows, even if lots of receiver clones are leaked.
        if count > isize::MAX as usize {
            process::abort();
        }
        Self {
            channel: self.channel.clone(),
            // opt_key: None,
        }
    }
}
impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.pad("Receiver { .. }") }
}

struct Channel<T> {
    sender: crossbeam::channel::Sender<T>,
    receiver: crossbeam::channel::Receiver<T>,
    sender_count: AtomicUsize,
    receiver_count: AtomicUsize,
    send_wakers: WakerSet,
    recv_wakers: WakerSet,
    /// Indicates that dropping a `Channel<T>` may drop values of type `T`.
    _marker: PhantomData<T>,
}
impl<T> Channel<T> {
    fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded::<T>();
        Self {
            sender,
            receiver,
            sender_count: AtomicUsize::new(1),
            receiver_count: AtomicUsize::new(1),
            send_wakers: WakerSet::new(),
            recv_wakers: WakerSet::new(),
            _marker: PhantomData,
        }
    }
    fn with_capacity(cap: usize) -> Self {
        let (sender, receiver) = crossbeam::channel::bounded::<T>(cap);
        Self {
            sender,
            receiver,
            sender_count: AtomicUsize::new(1),
            receiver_count: AtomicUsize::new(1),
            send_wakers: WakerSet::new(),
            recv_wakers: WakerSet::new(),
            _marker: PhantomData,
        }
    }
    fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> { self.sender.try_send(msg) }
    fn try_recv(&self) -> Result<T, TryRecvError> { self.receiver.try_recv() }
    fn capacity(&self) -> Option<usize> { self.receiver.capacity() }
    fn is_empty(&self) -> bool { self.receiver.is_empty() }
    fn is_full(&self) -> bool { self.receiver.is_full() }
    fn len(&self) -> usize { self.receiver.len() }

    pub const fn is_disconnected(&self) -> bool { false }
    const fn disconnect(&self) {}
}
