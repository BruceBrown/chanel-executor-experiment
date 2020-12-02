use super::*;
use crate::async_channel::*;
use crossbeam::channel::TryRecvError;

struct CrossbeamDaisyChain {
    inner_async: async_std::sync::Mutex<AsyncDaisyChainInner>,
}

impl CrossbeamDaisyChain {
    pub fn new(id: usize) -> Self {
        Self {
            inner_async: async_std::sync::Mutex::new(AsyncDaisyChainInner::new(id)),
        }
    }
}

#[async_trait]
impl AsyncMachine<TestMessage> for CrossbeamDaisyChain {
    async fn receive(&self, cmd: TestMessage) {
        let mut mutable = self.inner_async.lock().await;
        match mutable.handle_config(cmd) {
            Ok(_) => (),
            Err(msg) => match mutable.validate_sequence(msg) {
                Ok(msg) => {
                    mutable.handle_action(msg).await;
                    mutable.handle_notification().await;
                },
                Err(msg) => panic!("sequence error fwd {}, msg {:#?}", mutable.id, msg),
            },
        }
    }
}

#[derive(Default)]
pub struct CrossbeamDaisyChainDriver {
    message_count: usize,
    first: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
    tasks: Vec<async_std::task::JoinHandle<()>>,
}

impl DaisyChain for CrossbeamDaisyChainDriver {
    fn name(&self) -> &str { "crossbeam w/ async-std" }

    fn setup(&mut self, pipeline_count: usize, lane_count: usize, message_count: usize) {
        self.message_count = message_count;
        let (concentrator_s, concentrator_r) = async_channel::bounded::<TestMessage>(250);
        let machine = CrossbeamDaisyChain::new(0);

        let task = async_std::task::spawn(async move {
            loop {
                let cmd = concentrator_r.try_recv();
                if cmd.is_ok() {
                    machine.receive(cmd.unwrap()).await;
                } else {
                    break;
                }
            }
        });
        self.tasks.push(task);

        for id in 1 ..= lane_count {
            let mut prev = None;
            for id in 1 ..= pipeline_count {
                let (s, r) = async_channel::bounded::<TestMessage>(250);
                let machine = CrossbeamDaisyChain::new(id);
                let task = async_std::task::spawn(async move {
                    loop {
                        match r.try_recv() {
                            Ok(cmd) => machine.receive(cmd).await,
                            Err(TryRecvError::Disconnected) => break,
                            _ => (),
                        }
                    }
                });
                self.tasks.push(task);
                if prev.is_none() {
                    self.first.push(TestMessageSender::AsyncCrossbeamSender(s.clone()));
                } else {
                    let sender = prev.unwrap();
                    async_std::task::block_on(send_cmd_async(
                        &sender,
                        TestMessage::AddSender(TestMessageSender::AsyncCrossbeamSender(s.clone())),
                    ));
                }
                prev = Some(TestMessageSender::AsyncCrossbeamSender(s));
            }
            async_std::task::block_on(send_cmd_async(
                &prev.unwrap(),
                TestMessage::Notify(TestMessageSender::AsyncCrossbeamSender(concentrator_s.clone()), message_count),
            ));
        }
        let (s, r) = async_channel::bounded::<TestMessage>(10);
        let s = TestMessageSender::AsyncCrossbeamSender(s);
        let concentrator_s = TestMessageSender::AsyncCrossbeamSender(concentrator_s);
        async_std::task::block_on(send_cmd_async(&concentrator_s, TestMessage::Notify(s, lane_count)));
        self.notifier = Some(TestMessageReceiver::AsyncCrossbeamReceiver(r));
        println!("setup complete");
    }

    fn teardown(&mut self) {
        self.first.clear();
        self.notifier = None;
        self.tasks.clear();
    }

    fn run(&mut self) {
        for id in 0 .. self.message_count {
            for lane in &self.first {
                async_std::task::block_on(send_cmd_async(lane, TestMessage::TestData(id)));
            }
        }
        if let Some(ref mut notifier) = self.notifier {
            match notifier {
                TestMessageReceiver::FlumeReceiver(ref mut receiver) => {
                    async_std::task::block_on(receiver.recv_async());
                },
                _ => panic!("unexpected receiver"),
            }
        }
    }
}
