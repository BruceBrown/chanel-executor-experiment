use std::future::Future;
use crossbeam::channel;

pub fn bounded<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::with_capacity(cap));
    let s = Sender { channel: channel.clone() }
}

pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::new());
    let s = Sender { channel: channel.clone() };
    let r = Receiver { channel: channel };
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
                        Ok(()) => return Poll::Ready(()),
                        Err(TrySendError::Disconnected(msg)) => {
                            self.msg = Some(msg);
                            return Poll::Pending;
                        }
                        Err(TrySendError::Full(msg)) => {
                            self.msg = Some(msg);
                            // Insert this send operation.
                            self.opt_key = Some(self.channel.send_wakers.insert(cx));
                            // If the channel is still full and not disconnected, return.
                            if self.channel.is_full() && !self.channel.is_disconnected() {
                                return Poll::Pending;
                            }
                        }
                    }
                }
            }
        }
        impl<T> Drop for SendFuture<'_, T> {
            fn drop(&mut self) {
                // If the current task is still in the set, that means it is being cancelled now.
    
                // Wake up another task instead.
    
                if let Some(key) = self.opt_key {
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
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.channel.try_send(msg)
    }
    pub fn capacity(&self) -> usize {
        self.channel.cap
    }
    pub fn is_empty(&self) -> bool {
        self.channel.is_empty()
    }
    pub fn is_full(&self) -> bool {
        self.channel.is_full()
    }
    pub fn len(&self) -> usize {
        self.channel.len()
    }
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Sender { .. }")
    }
}
pub struct Receiver<T> {
    /// The inner channel.
    channel: Arc<Channel<T>>,
    /// The key for this receiver in the `channel.stream_wakers` set.
    opt_key: Option<usize>,
}
impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.channel.try_recv()
    }
    pub fn capacity(&self) -> usize {
        self.channel.cap
    }
    pub fn is_empty(&self) -> bool {
        self.channel.is_empty()
    }
    pub fn is_full(&self) -> bool {
        self.channel.is_full()
    }
    pub fn len(&self) -> usize {
        self.channel.len()
    }
}
impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Receiver<T> {
        let count = self.channel.receiver_count.fetch_add(1, Ordering::Relaxed);
        // Make sure the count never overflows, even if lots of receiver clones are leaked.
        if count > isize::MAX as usize {
            process::abort();
        }
        Receiver {
            channel: self.channel.clone(),
            opt_key: None,
        }
    }
}
impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Receiver { .. }")
    }
}

struct Channel<T> {
    sender: crossbeam::channel::Sender<T>,
    receiver: crossbeam::channel::Receiver<T>,
    sender_count: AtomicUsize,
    receiver_count: AtomicUsize,
    /// Indicates that dropping a `Channel<T>` may drop values of type `T`.
    _marker: PhantomData<T>,
}
impl<T> Channel<T> {
    fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded::<T>();
        Self { sender, receiver }
    }
    fn with_capacity(cap usize) -> Self {
        let (sender, receiver) = crossbeam::channel::bounded::<T>(cap);
        Self { sender, receiver }  
    }
    fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(msg)
    }
    fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }
    fn capacity(&self) -> usize {
        self.receiver.cap
    }
    fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }
    fn is_full(&self) -> bool {
        self.receiver.is_full()
    }
    fn len(&self) -> usize {
        self.receiver.len()
    }
}