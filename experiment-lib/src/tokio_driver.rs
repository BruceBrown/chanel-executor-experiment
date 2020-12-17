use super::*;
use tokio::sync::mpsc::{channel, unbounded_channel};

/// The ServerSimulator simulate an asynchronous server in which there are multiple stages processing
/// data and parallel processing occurring. To give this concreteness, consider that you want to simulate
/// audio processing, where you have 4000 connections and there are 5 descrete processing elements between
/// receiving audio in and sending audio out. The simulator would reprsent that at a pipeline of 5, and
/// 4000 lanes. It would then driver some number of packets through that configuration, representing
/// audio data to be processed.
///
/// The simulator uses tokio for channels and tokio for task execution.
pub struct ServerSimulator {
    runtime: Arc<tokio::runtime::Runtime>,
    messages: usize,
    lanes: Vec<ChannelSender>,
    notifier: Option<ChannelReceiver>,
    verbosity: Verbosity,
}

impl ServerSimulator {
    fn create_channel(config: ExperimentConfig) -> (ChannelSender, ChannelReceiver) {
        if config.capacity == 0 {
            let (s, r) = unbounded_channel::<FwdMessage>();
            (
                ChannelSender::TokioUnboundedSender(s),
                ChannelReceiver::TokioUnboundedReceiver(Arc::new(tokio::sync::Mutex::new(r))),
            )
        } else {
            let (s, r) = channel::<FwdMessage>(config.capacity);
            (
                ChannelSender::TokioSender(s),
                ChannelReceiver::TokioReceiver(Arc::new(tokio::sync::Mutex::new(r))),
            )
        }
    }
}

impl Default for ServerSimulator {
    fn default() -> Self {
        Self {
            runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
            messages: 0,
            lanes: Vec::default(),
            notifier: None,
            verbosity: Verbosity::default(),
        }
    }
}

impl Drop for ServerSimulator {
    fn drop(&mut self) { self.teardown(); }
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &'static str { "tokio" }

    fn setup(&mut self, config: ExperimentConfig) {
        self.messages = config.messages;
        self.verbosity = config.verbosity;
        let mut senders = Vec::new();

        let (concentrator_sender, receiver) = Self::create_channel(config);
        // build the concentrator
        Builder::new().verbosity(config.verbosity).schedule_tokio(receiver, &self.runtime);

        for lane in 1 ..= config.lanes {
            for pipeline in 1 ..= config.pipelines {
                let (sender, receiver) = Self::create_channel(config);
                // build the forwarder
                Builder::new()
                    .pipeline(pipeline)
                    .lane(lane)
                    .verbosity(config.verbosity)
                    .schedule_tokio(receiver, &self.runtime);
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
                let _task = self.runtime.spawn(future);
            }
        }
        // senders are now just the head sender of each lane, save it
        self.lanes = senders;
        // configure the concentrator
        let (notifier_sender, notifier_receiver) = Self::create_channel(config);
        let _task = self.runtime.spawn(async move {
            concentrator_sender
                .send_async(FwdMessage::Notify(notifier_sender, config.lanes * config.messages))
                .await
                .ok()
        });
        self.notifier = Some(notifier_receiver);
    }

    fn teardown(&mut self) {
        if self.verbosity != Verbosity::None {
            println!("starting teardown for {}", self.name());
        }
        self.notifier = None;
        self.lanes.clear();
        let name = self.name();
        if self.verbosity != Verbosity::None {
            println!("all tasks completed, {} shutdown complete", name);
        }
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
            let _task = self.runtime.spawn(future);
        }
        // wait for the notifier to get a count
        if let Some(ref notifier) = self.notifier {
            let notifier = notifier.clone();
            let notifier = async move { notifier.recv_async().await };
            let task = self.runtime.spawn(notifier);
            let _result = self.runtime.block_on(async { task.await });
        }
    }
}
