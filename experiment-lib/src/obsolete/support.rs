use async_trait::async_trait;

/

#[derive(Default)]
pub struct ExperimentDrivers {
    pub config: ExperimentConfig,
    pub drivers: Vec<Box<dyn ExperimentDriver>>,
}

// Configuration for the experiment.
#[derive(Debug, Copy, Clone)]
pub struct ExperimentConfig {
    // Capacity of the message queue, zero is unbounded.
    pub capacity: usize,

    // Count of forwarders in a lane.
    pub pipelines: usize,

    // Count of lanes
    pub lanes: usize,

    // Count of messages per lane
    pub messages: usize,

    // Count of iterations of an experiment
    pub iterations: usize,

    // Count of threads the async executor will use
    pub threads: usize,

    // Verbosity level, mostly used for debugging
    pub verbosity: Verbosity,
}

impl Default for ExperimentConfig {
    fn default() -> Self {
        Self {
            capacity: QUEUE_CAPACITY,
            pipelines: PIPELINES,
            lanes: LANES,
            messages: MESSAGES,
            iterations: ITERATIONS,
            threads: ASYNC_THREAD_COUNT,
            verbosity: VERBOSITY,
        }
    }
}

// Each experiment will implement the ExperimentDriver trait
pub trait ExperimentDriver {
    fn name(&self) -> &'static str;
    fn setup(&mut self, config: ExperimentConfig);
    fn teardown(&mut self);
    fn run(&self);
}

// The Receiver is implemented to provide an injection point for received messages.
pub trait Receiver<T>
where
    T: Send + Sync,
{
    fn receive(&mut self, cmd: T);
}
// The Receiver is implemented to provide an injection point for received messages.
#[async_trait]
pub trait AsyncReceiver<T>
where
    T: Send + Sync,
{
    async fn receive(&mut self, cmd: T);
}
#[allow(unused_imports)] use d3::d3_derive::*;
#[derive(Debug, Clone)]
pub enum TestMessage {
    // TestData contains a sequence number or count
    TestData(usize),

    // AddSender provides the receiver a forwarder for received messages
    AddSender(TestMessageSender),

    // Notify provides the receiver a notifier and message count for notifying the notifier.
    Notify(TestMessageSender, usize),
}

// TestMessageSender are the flavors of senders that are used by the experiments.
#[derive(Debug, Clone)]
pub enum TestMessageSender {
    //    CrossbeamSender(crossbeam::channel::Sender<TestMessage>),
    // D3Sender(d3::core::machine_impl::Sender<TestMessage>),
    //    TokioSender(tokio::sync::mpsc::Sender<TestMessage>),
    //    FlumeSender(flume::Sender<TestMessage>),
    //    AsyncCrossbeamSender(async_channel::Sender<TestMessage>),
    //    FuturesSender(futures::channel::mpsc::Sender<TestMessage>),
    // SmolSender(smol::channel::Sender<TestMessage>),
    FastChannelSender(async_channel::Sender<TestMessage>),
}

// TestMessageReceiver are the flavors of receivers that are used by the experiments.
#[derive(Debug, Clone)]
pub enum TestMessageReceiver {
    //    CrossbeamReceiver(crossbeam::channel::Receiver<TestMessage>),
    //    D3Receiver(d3::core::machine_impl::Receiver<TestMessage>),
    //    TokioReceiver(tokio::sync::mpsc::Receiver<TestMessage>),
    //    FlumeReceiver(flume::Receiver<TestMessage>),
    //    AsyncCrossbeamReceiver(async_channel::Receiver<TestMessage>),
    //    FuturesReceiver(futures::channel::mpsc::Receiver<TestMessage>),
    // SmolReceiver(smol::channel::Receiver<TestMessage>),
    FastChannelReceiver(async_channel::Receiver<TestMessage>),
}

// The send_cmd function will send a message to a sender. It hides the sync/async nature of the sender
#[allow(unused_variables)]
pub fn send_cmd(sender: &TestMessageSender, cmd: TestMessage) {
    match sender {
        // TestMessageSender::StdMsgSender(sender) => if sender.send(cmd).is_err() {},
        // TestMessageSender::CrossbeamSender(sender) => if sender.send(cmd).is_err() {},

        // TestMessageSender::TokioSender(sender) => if sender.send(cmd).await.is_err() {},
        // TestMessageSender::FlumeSender(sender) => if sender.send_async(cmd).await.is_err() {},
        // fixme: (futures) This is likely wrong. Need to send cmd via Sender sender and await.
        // TestMessageSender::FuturesSender(_sender) => (), // if sender.send(cmd).await.is_err() {},
        // TestMessageSender::SmolSender(sender) => smol::future::block_on( async { sender.send(cmd).await.unwrap_or_else(|_e| println!("send failed"))}),
        // TestMessageSender::FastChannelSender(sender) => futures_lite::future::block_on( async {
        // sender.send(cmd).await.unwrap_or_else(|_e| println!("send failed"));
        // }),
        //
        // TestMessageSender::D3Sender(sender) => sender.send(cmd).unwrap_or_else(|_e| println!("send failed")),
        _ => panic!("unhandled sender"),
    }
}

pub async fn send_cmd_async(sender: &TestMessageSender, cmd: TestMessage) {
    match sender {
        TestMessageSender::FastChannelSender(sender) => sender.send(cmd).await.ok().unwrap(),
        #[allow(unreachable_patterns)]
        _ => panic!("unhandled sender"),
    }
}
