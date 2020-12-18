use super::*;
use futures::join;
use std::sync::atomic::{AtomicUsize, Ordering};

/// ChannelSender is a variant that provides normalization for different channel implementations of sending messages asynchronously.
#[derive(Debug, Clone)]
pub enum ChannelSender {
    /// Sender for async_channel.
    AsyncChannelSender(async_channel::Sender<FwdMessage>),

    /// Sender for flume.
    FlumeSender(flume::Sender<FwdMessage>),

    /// Sender for tokio bounded receiver.
    TokioSender(tokio::sync::mpsc::Sender<FwdMessage>),

    /// Sender for tokio unbounded receiver.
    TokioUnboundedSender(tokio::sync::mpsc::UnboundedSender<FwdMessage>),
}

impl ChannelSender {
    /// Send a message asynchronously.
    pub async fn send_async(&self, msg: FwdMessage) -> Result<(), std::sync::mpsc::SendError<FwdMessage>> {
        match self {
            Self::AsyncChannelSender(sender) => match sender.send(msg).await {
                Ok(_) => Ok(()),
                Err(async_channel::SendError(msg)) => Err(std::sync::mpsc::SendError(msg)),
            },
            Self::FlumeSender(sender) => match sender.send_async(msg).await {
                Ok(_) => Ok(()),
                Err(flume::SendError(msg)) => Err(std::sync::mpsc::SendError(msg)),
            },
            Self::TokioSender(sender) => match sender.send(msg).await {
                Ok(_) => Ok(()),
                Err(tokio::sync::mpsc::error::SendError(msg)) => Err(std::sync::mpsc::SendError(msg)),
            },
            Self::TokioUnboundedSender(sender) => match sender.send(msg) {
                Ok(_) => Ok(()),
                Err(tokio::sync::mpsc::error::SendError(msg)) => Err(std::sync::mpsc::SendError(msg)),
            },
            #[allow(unreachable_patterns)]
            _ => panic!("unhandled ChannelSender"),
        }
    }
}

/// ChannelReceiver is a variant that provides normalization for different channel implementations of receiving messages asynchronously.
#[derive(Debug, Clone)]
pub enum ChannelReceiver {
    /// Receiver for async_channel.
    AsyncChannelReceiver(async_channel::Receiver<FwdMessage>),

    /// Receiver for flume.
    FlumeReceiver(flume::Receiver<FwdMessage>),

    /// Receiver for tokio bounded receiver.
    TokioReceiver(Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<FwdMessage>>>),

    /// Receiver for tokio unbounded receiver.
    TokioUnboundedReceiver(Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<FwdMessage>>>),
}

impl ChannelReceiver {
    /// Receive a message asynchronously.
    pub async fn recv_async(&self) -> Result<FwdMessage, std::sync::mpsc::RecvError> {
        match self {
            Self::AsyncChannelReceiver(receiver) => match receiver.recv().await {
                Ok(msg) => Ok(msg),
                Err(async_channel::RecvError) => Err(std::sync::mpsc::RecvError),
            },
            Self::FlumeReceiver(receiver) => match receiver.recv_async().await {
                Ok(msg) => Ok(msg),
                Err(_) => Err(std::sync::mpsc::RecvError),
            },
            Self::TokioReceiver(receiver) => {
                let mut receiver = receiver.lock().await;
                match receiver.recv().await {
                    Some(msg) => Ok(msg),
                    None => Err(std::sync::mpsc::RecvError),
                }
            },
            Self::TokioUnboundedReceiver(receiver) => {
                let mut receiver = receiver.lock().await;
                match receiver.recv().await {
                    Some(msg) => Ok(msg),
                    None => Err(std::sync::mpsc::RecvError),
                }
            },
            #[allow(unreachable_patterns)]
            _ => panic!("unhandled channel receiver type"),
        }
    }
}

/// FwdMessage is a variant which the Forwarder treats as an instruction set.
#[derive(Debug)]
pub enum FwdMessage {
    /// TestData contains a sequence number or count
    TestData(usize),

    /// AddSender provides the receiver a forwarder for received messages
    AddSender(ChannelSender),

    /// Notify provides the receiver a notifier and message count for notifying the notifier.
    Notify(ChannelSender, usize),
}

// Builder is a Forwarder factory.
#[derive(Default)]
pub struct Builder {
    /// The Forwarder's pipeline id.
    pipeline: usize,

    /// The Forwarder's lane id.
    lane: usize,

    /// The Forwarder's logging verbosity.
    verbosity: Verbosity,
}

impl Builder {
    /// Construct the Builder
    pub fn new() -> Self { Self::default() }

    /// Override the pipeline identifier for the Forwarder
    pub fn pipeline(mut self, pipeline: usize) -> Builder {
        self.pipeline = pipeline;
        self
    }

    /// Override the lane identifier for the Forwarder.
    pub fn lane(mut self, lane: usize) -> Builder {
        self.lane = lane;
        self
    }

    /// Override the verbosity used in logging by the Forwarder.
    pub fn verbosity(mut self, verbosity: Verbosity) -> Builder {
        self.verbosity = verbosity;
        self
    }

    /// Schedule the Forwarder to receive and process FwdMessage instructions.
    pub fn schedule(self, receiver: ChannelReceiver, pool: &MultiThreadedAsyncExecutorPool) {
        let mut forwarder = Forwarder {
            pipeline: self.pipeline,
            lane: self.lane,
            verbosity: self.verbosity,
            forwarding_multiplier: 1,
            ..Forwarder::default()
        };
        let future = async move {
            while let Ok(value) = receiver.recv_async().await {
                forwarder.receive(value).await;
            }
        };
        pool.spawn(future).detach();
    }

    /// Schedule the Forwarder to receive and process FwdMessage instructions.
    pub fn schedule_tokio(self, receiver: ChannelReceiver, pool: &Arc<tokio::runtime::Runtime>) {
        let mut forwarder = Forwarder {
            pipeline: self.pipeline,
            lane: self.lane,
            verbosity: self.verbosity,
            forwarding_multiplier: 1,
            ..Forwarder::default()
        };
        let future = async move {
            while let Ok(value) = receiver.recv_async().await {
                forwarder.receive(value).await;
            }
        };
        let _task = pool.spawn(future);
    }
}

/// Forward implements a programmable object which can forward received message and notify
/// when it has received a configured number of messages.
#[derive(Default)]
struct Forwarder {
    /// The pipeline id, mosly used for logging.
    pipeline: usize,

    /// The lane id, mosly used for logging.
    lane: usize,

    /// The collection of senders, each will be sent any received message.
    senders: Vec<ChannelSender>,

    /// The count of messages received.
    received_count: AtomicUsize,

    /// The count of messages sent.
    send_count: AtomicUsize,

    /// The count of received messages that triggers notification.
    notify_count: usize,

    /// The notifier, which is sent the total number of messages received.
    notify_sender: Option<ChannelSender>,

    /// The forwarding multiplier is the count of messages to send for every message received.
    forwarding_multiplier: usize,

    /// The expected next sequence number for FwdMessage::TestData.
    next_seq: usize,

    /// The logging verbosity.
    verbosity: Verbosity,
}

impl Forwarder {
    /// Get the pipeline id.
    pub const fn pipeline(&self) -> usize { self.pipeline }

    /// Get the lane id.
    pub const fn lane(&self) -> usize { self.lane }

    /// Get a tag that can be used in logging.
    pub fn tag(&self) -> String { format!("fwd pipeline {} lane {}", self.pipeline(), self.lane()) }

    /// For FwdMessage::TestData, validate the sequence number. A sequence number of 0, is
    /// treated as a reset, setting the next expected sequence to 1. An Ok return indicates
    /// proper sequencing, while an Err indicates improper sequencing.
    pub fn validate_sequence(&mut self, msg: FwdMessage) -> Result<FwdMessage, FwdMessage> {
        // if we forward, validate the sequence number, otherwise just accept it as we're likely a notifier
        if !self.senders.is_empty() {
            match msg {
                FwdMessage::TestData(0) => self.next_seq = 1,
                FwdMessage::TestData(seq) if seq == self.next_seq => self.next_seq += 1,
                FwdMessage::TestData(_) => return Err(msg),
                _ => (),
            }
        }
        // bump received count
        self.received_count.fetch_add(1, Ordering::Relaxed);
        Ok(msg)
    }

    /// Handle a configuration instruction, adding a sender or notifier. An error is returned
    /// if the message is not a configuration message.
    pub fn handle_config(&mut self, msg: FwdMessage) -> Result<(), FwdMessage> {
        match msg {
            FwdMessage::Notify(sender, on_receive_count) => {
                self.notify_sender = Some(sender);
                self.notify_count = on_receive_count;
            },
            FwdMessage::AddSender(sender) => self.senders.push(sender),
            msg => return Err(msg),
        }
        Ok(())
    }

    /// Handle an action instruction, forwarding the message to any senders.
    pub async fn handle_action(&self, cmd: FwdMessage) {
        match cmd {
            FwdMessage::TestData(_) => {
                for sender in &self.senders {
                    for _ in 0 .. self.forwarding_multiplier {
                        sender
                            .send_async(FwdMessage::TestData(self.send_count.fetch_add(1, Ordering::Relaxed)))
                            .await
                            .ok();
                    }
                }
            },
            _ => panic!("unhandled action"),
        }
    }

    /// Handle sending out a notification.
    pub async fn handle_notification(&self) {
        if self.notify_count > 0 && self.received_count.load(Ordering::Relaxed) == self.notify_count {
            if self.verbosity != Verbosity::None {
                println!("{} sending notification", self.tag());
            }
            let notifier = self.notify_sender.as_ref().unwrap();
            notifier.send_async(FwdMessage::TestData(self.notify_count)).await.ok();
            self.clear_counters();
        }
    }

    /// Clear all counters.
    fn clear_counters(&self) {
        self.received_count.store(0, Ordering::Relaxed);
        self.send_count.store(0, Ordering::Relaxed);
    }
}

impl Forwarder {
    /// Handle a received FwdMessage command.
    async fn receive(&mut self, cmd: FwdMessage) {
        match self.handle_config(cmd) {
            Ok(_) => (),
            Err(msg) => match self.validate_sequence(msg) {
                Ok(msg) => {
                    join!(self.handle_action(msg), self.handle_notification());
                },
                Err(msg) => panic!("{} sequence error: expecting {} not cmd {:#?}", self.tag(), self.next_seq, msg),
            },
        }
    }
}
