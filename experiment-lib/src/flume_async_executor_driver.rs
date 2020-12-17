use super::*;
use std::thread;

use flume::{bounded, unbounded};
use futures_lite::future::block_on;

/// The ServerSimulator simulate an asynchronous server in which there are multiple stages processing
/// data and parallel processing occurring. To give this concreteness, consider that you want to simulate
/// audio processing, where you have 4000 connections and there are 5 descrete processing elements between
/// receiving audio in and sending audio out. The simulator would reprsent that at a pipeline of 5, and
/// 4000 lanes. It would then driver some number of packets through that configuration, representing
/// audio data to be processed.
///
/// The simulator uses flume for channels and asycn_executor for task execution.
#[derive(Default)]
pub struct ServerSimulator {
    pool: MultiThreadedAsyncExecutorPool,
    messages: usize,
    lanes: Vec<ChannelSender>,
    notifier: Option<ChannelReceiver>,
    verbosity: Verbosity,
}
impl ServerSimulator {
    fn create_channel(config: ExperimentConfig) -> (ChannelSender, ChannelReceiver) {
        let (s, r) = if config.capacity == 0 {
            unbounded::<FwdMessage>()
        } else {
            bounded::<FwdMessage>(config.capacity)
        };
        (ChannelSender::FlumeSender(s), ChannelReceiver::FlumeReceiver(r))
    }
}

impl Drop for ServerSimulator {
    fn drop(&mut self) { self.teardown(); }
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &'static str { "flume w/ async" }

    fn setup(&mut self, config: ExperimentConfig) {
        self.messages = config.messages;
        self.verbosity = config.verbosity;
        self.pool.start(config.threads);
        let mut senders = Vec::new();
        // setup the pipeline lanes, the last in each lane sends to the common concentrator

        let (concentrator_sender, receiver) = Self::create_channel(config);
        // build the concentrator
        Builder::new().verbosity(config.verbosity).schedule(receiver, &self.pool);

        for lane in 1 ..= config.lanes {
            for pipeline in 1 ..= config.pipelines {
                let (sender, receiver) = Self::create_channel(config);
                // build the forwarder
                Builder::new()
                    .pipeline(pipeline)
                    .lane(lane)
                    .verbosity(config.verbosity)
                    .schedule(receiver, &self.pool);
                senders.push(sender);
            }
            // configure the forwarders
            senders.push(concentrator_sender.clone());
            for _ in (2 ..= config.pipelines).rev() {
                let sender = senders.pop().unwrap();
                let last_sender = senders.last().unwrap().clone();
                let future = async move {
                    last_sender.send_async(FwdMessage::AddSender(sender)).await.ok();
                };
                self.pool.spawn(future).detach();
            }
        }
        // senders are now just the head sender of each lane, save it
        self.lanes = senders;
        // configure the concentrator
        let (notifier_sender, notifier_receiver) = Self::create_channel(config);
        self.pool
            .spawn(async move {
                concentrator_sender
                    .send_async(FwdMessage::Notify(notifier_sender, config.lanes * config.messages))
                    .await
                    .ok()
            })
            .detach();
        self.notifier = Some(notifier_receiver);
    }

    fn teardown(&mut self) {
        if self.verbosity != Verbosity::None {
            println!("starting teardown for {}", self.name());
        }
        self.notifier = None;
        self.lanes.clear();
        while !self.pool.is_empty() {
            thread::sleep(std::time::Duration::from_millis(20));
        }
        if self.verbosity != Verbosity::None {
            println!("all tasks completed, {} shutdown complete", self.name());
        }
        self.pool.stop();
    }

    fn run(&self) {
        let messages = self.messages;
        // parallelize driving the lanes
        for sender in &self.lanes {
            let sender = sender.clone();
            let future = async move {
                for msg_id in 0 .. messages {
                    sender.send_async(FwdMessage::TestData(msg_id)).await.unwrap();
                }
            };
            self.pool.spawn(future).detach();
        }
        // wait for the notifier to get a count
        if let Some(ref notifier) = self.notifier {
            let notifier = notifier.clone();
            let notifier = async move { notifier.recv_async().await };
            let task = self.pool.spawn(notifier);
            let _result = block_on(async { task.await }).unwrap_or(FwdMessage::TestData(0));
        }
    }
}
