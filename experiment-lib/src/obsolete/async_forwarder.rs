use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use futures::join;

use async_channel::{bounded, unbounded, Receiver, Sender};
use futures_lite::future::block_on;

// This is a variant driven smol driver. It performs configuration and validation using a Forwarder object.
// It is expected to be slightly slower than a smol driver, due to command handling logic.
#[derive(Debug)]
enum FwdMessage {
    // TestData contains a sequence number or count
    TestData(usize),

    // AddSender provides the receiver a forwarder for received messages
    AddSender(Sender<FwdMessage>),

    // Notify provides the receiver a notifier and message count for notifying the notifier.
    Notify(Sender<FwdMessage>, usize),
}
// Forwarder factory
#[derive(Default, Copy, Clone)]
struct Builder {
    pipeline: usize,
    lane: usize,
    verbosity: Verbosity,
}
impl Builder {
    pub fn new() -> Self { Self::default() }
    pub fn pipeline(mut self, pipeline: usize) -> Builder {
        self.pipeline = pipeline;
        self
    }
    pub fn lane(mut self, lane: usize) -> Builder {
        self.lane = lane;
        self
    }
    pub fn verbosity(mut self, verbosity: Verbosity) -> Builder {
        self.verbosity = verbosity;
        self
    }
    pub fn schedule(self, receiver: Receiver<FwdMessage>, pool: &MultiThreadedAsyncExecutorPool) {
        let mut forwarder = Forwarder {
            pipeline: self.pipeline,
            lane: self.lane,
            verbosity: self.verbosity,
            forwarding_multiplier: 1,
            ..Forwarder::default()
        };
        let future = async move {
            while let Ok(value) = receiver.recv().await {
                forwarder.receive(value).await;
            }
        };
        pool.spawn(future).detach();
    }
}

/// Forward implements a forwarder which can forward received message and notify
/// a Receiver when it has received a configured number of messages.
#[derive(Default)]
struct Forwarder {
    /// a pipeline id, mosly used for logging
    pipeline: usize,

    /// a lane id, mosly used for logging
    lane: usize,

    /// collection of senders, each will be sent any received message.
    senders: Vec<Sender<FwdMessage>>,

    /// received_count is the count of messages received by this forwarder.
    received_count: AtomicUsize,

    /// send_count is the count of messages sent by this forwarder.
    send_count: AtomicUsize,

    /// notify_count is compared against received_count for means of notifcation.
    notify_count: usize,

    /// notify_sender is sent a TestData message with the data being the number of messages received.
    notify_sender: Option<Sender<FwdMessage>>,

    /// forwarding multiplier
    forwarding_multiplier: usize,

    // for TestData, this is the next in sequence
    next_seq: usize,

    // logging verbosity
    verbosity: Verbosity,
}
impl Forwarder {
    /// get the pipeline id
    pub const fn pipeline(&self) -> usize { self.pipeline }

    /// get the lane id
    pub const fn lane(&self) -> usize { self.lane }

    pub fn tag(&self) -> String { format!("fwd pipeline {} lane {}", self.pipeline(), self.lane()) }

    /// if msg is TestData, validate the sequence or reset if 0
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

    /// If msg is a configuration msg, handle it otherwise return it as an error
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

    /// handle the action messages
    pub async fn handle_action(&self, cmd: FwdMessage) {
        match cmd {
            FwdMessage::TestData(_) => {
                for sender in &self.senders {
                    for _ in 0 .. self.forwarding_multiplier {
                        sender
                            .send(FwdMessage::TestData(self.send_count.fetch_add(1, Ordering::Relaxed)))
                            .await
                            .ok();
                    }
                }
            },
            _ => panic!("unhandled action"),
        }
    }

    /// handle sending out a notification and resetting counters when notificaiton is sent
    pub async fn handle_notification(&self) {
        if self.notify_count > 0 && self.received_count.load(Ordering::Relaxed) == self.notify_count {
            if self.verbosity != Verbosity::None {
                println!("{} sending notification", self.tag());
            }
            let notifier = self.notify_sender.as_ref().unwrap();
            notifier.send(FwdMessage::TestData(self.notify_count)).await.ok();
            self.clear_counters();
        }
    }

    /// clear all counters
    fn clear_counters(&self) {
        self.received_count.store(0, Ordering::Relaxed);
        self.send_count.store(0, Ordering::Relaxed);
    }
}

impl Forwarder {
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
