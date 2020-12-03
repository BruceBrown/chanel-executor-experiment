use super::*;
/// This is a bunch of supporting enums, traits, functions, etc that are needed by main and bench.

// Each experiment will implement the ExperimentDriver trait
pub trait ExperimentDriver {
    fn name(&self) -> &str;
    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize);
    fn teardown(&mut self);
    fn run(&mut self);
}

// Async drivers will implement AsyncReceiver to call into the forwarder
#[async_trait]
pub trait AsyncReceiver<T> {
    async fn receive(&self, cmd: T);
}

// Sync drivers will implement SyncReceiver to call into the forwarder
pub trait SyncReceiver<T> {
    fn receive(&self, cmd: T);
}

use d3::core::machine_impl::*;
#[allow(unused_imports)] use d3::d3_derive::*;
#[derive(Debug, Clone, MachineImpl)]
pub enum TestMessage {
    // TestData has a single parameter, as a tuple
    TestData(usize),
    // AddSender can be implemented to push a sender onto a list of senders
    AddSender(TestMessageSender),
    // Notify, is setup for a notification via TestData, where usize is a message count
    Notify(TestMessageSender, usize),
}

// These are the flavors of senders that the forward has to deal with
#[derive(Debug, Clone)]
pub enum TestMessageSender {
    CrossbeamSender(crossbeam::channel::Sender<TestMessage>),
    D3Sender(d3::core::machine_impl::Sender<TestMessage>),
    TokioSender(tokio::sync::mpsc::Sender<TestMessage>),
    FlumeSender(flume::Sender<TestMessage>),
    AsyncCrossbeamSender(async_channel::Sender<TestMessage>),
    FuturesSender(futures::channel::mpsc::Sender<TestMessage>),
    SmolSender(smol::channel::Sender<TestMessage>),
}

// These are the flavors of receivers that the forward has to deal with
#[derive(Debug)]
pub enum TestMessageReceiver {
    CrossbeamReceiver(crossbeam::channel::Receiver<TestMessage>),
    D3Receiver(d3::core::machine_impl::Receiver<TestMessage>),
    TokioReceiver(tokio::sync::mpsc::Receiver<TestMessage>),
    FlumeReceiver(flume::Receiver<TestMessage>),
    AsyncCrossbeamReceiver(async_channel::Receiver<TestMessage>),
    FuturesReceiver(futures::channel::mpsc::Receiver<TestMessage>),
    SmolReceiver(smol::channel::Receiver<TestMessage>),
}

// handy function to sync send a message
pub fn send_cmd(sender: &TestMessageSender, cmd: TestMessage) {
    match sender {
        // TestMessageSender::StdMsgSender(sender) => if sender.send(cmd).is_err() {},
        TestMessageSender::CrossbeamSender(sender) => if sender.send(cmd).is_err() {},
        TestMessageSender::D3Sender(sender) => if sender.send(cmd).is_err() {},
        _ => panic!("unhandled sync sender"),
    }
}

// handy function to async send a message
pub async fn send_cmd_async(sender: &TestMessageSender, cmd: TestMessage) {
    match sender {
        TestMessageSender::TokioSender(sender) => if sender.send(cmd).await.is_err() {},
        TestMessageSender::FlumeSender(sender) => if sender.send_async(cmd).await.is_err() {},
        // fixme: (futures) This is likely wrong. Need to send cmd via Sender sender and await.
        TestMessageSender::FuturesSender(_sender) => (), // if sender.send(cmd).await.is_err() {},
        TestMessageSender::AsyncCrossbeamSender(sender) => sender.send(cmd).await,
        TestMessageSender::SmolSender(sender) => if sender.send(cmd).await.is_err() {},
        _ => panic!("unhandled async sender"),
    }
}
