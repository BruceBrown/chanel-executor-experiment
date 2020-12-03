use super::*;
use futures::executor;

/// For this async_channel test we'll use the futures runtime, and we create a task per element.
/// So, for a 10 element pipeline with 1000 lanes, there will be 4000 task plus some for the concentrator.
///  
struct Forwarder {
    inner_async: tokio::sync::Mutex<AsyncForwarder>,
}

impl Forwarder {
    pub fn new(id: usize) -> Self {
        Self {
            inner_async: tokio::sync::Mutex::new(AsyncForwarder::new(id)),
        }
    }
}

#[async_trait]
impl AsyncReceiver<TestMessage> for Forwarder {
    async fn receive(&self, cmd: TestMessage) {
        let mut mutable = self.inner_async.lock().await;
        match mutable.handle_config(cmd) {
            Ok(_) => (),
            Err(msg) => match mutable.validate_sequence(msg) {
                Ok(msg) => {
                    mutable.handle_action(msg).await;
                    mutable.handle_notification().await;
                },
                Err(msg) => panic!("sequence error fwd {}, msg {:#?}", mutable.id(), msg),
            },
        }
    }
}

pub struct ServerSimulator {
    threadpool: Arc<executor::ThreadPool>,
    messages: usize,
    heads: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Default for ServerSimulator {
    fn default() -> Self {
        Self {
            threadpool: Arc::new(executor::ThreadPool::new().unwrap()),
            messages: 0,
            heads: Vec::default(),
            notifier: None,
            tasks: Vec::default(),
        }
    }
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str { "async_channel w/ futures" }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        let rt = &self.threadpool;
        self.messages = messages;
        let (concentrator_s, concentrator_r) = async_channel::bounded::<TestMessage>(250);
        let forwarder = Forwarder::new(0);

        let _task = rt.spawn_ok(async move {
            while let Ok(cmd) = concentrator_r.recv().await {
                forwarder.receive(cmd).await;
            }
        });
        for _ in 1 ..= lanes {
            let mut prev = None;
            for id in 1 ..= pipelines {
                let (s, r) = async_channel::bounded::<TestMessage>(250);
                let forwarder = Forwarder::new(id);
                let _task = rt.spawn_ok(async move {
                    while let Ok(cmd) = r.recv().await {
                        forwarder.receive(cmd).await;
                        while let Ok(cmd) = r.try_recv() {
                            forwarder.receive(cmd).await;
                        }
                    }
                });
                // if first, save head, otherwise forward previous to this sender
                match prev {
                    Some(sender) => executor::block_on(send_cmd_async(
                        &sender,
                        TestMessage::AddSender(TestMessageSender::AsyncCrossbeamSender(s.clone())),
                    )),
                    None => self.heads.push(TestMessageSender::AsyncCrossbeamSender(s.clone())),
                }
                prev = Some(TestMessageSender::AsyncCrossbeamSender(s));
            }
            executor::block_on(send_cmd_async(
                &prev.unwrap(),
                TestMessage::Notify(TestMessageSender::AsyncCrossbeamSender(concentrator_s.clone()), messages),
            ));
        }
        let (s, r) = async_channel::bounded::<TestMessage>(10);
        let s = TestMessageSender::AsyncCrossbeamSender(s);
        let concentrator_s = TestMessageSender::AsyncCrossbeamSender(concentrator_s);
        executor::block_on(send_cmd_async(&concentrator_s, TestMessage::Notify(s, lanes)));
        self.notifier = Some(TestMessageReceiver::AsyncCrossbeamReceiver(r));
    }

    fn teardown(&mut self) {
        self.heads.clear();
        self.notifier = None;
        self.tasks.clear();
    }

    fn run(&mut self) {
        for id in 0 .. self.messages {
            for head in &self.heads {
                executor::block_on(send_cmd_async(head, TestMessage::TestData(id)));
            }
        }
        if let Some(ref mut notifier) = self.notifier {
            match notifier {
                TestMessageReceiver::AsyncCrossbeamReceiver(ref mut receiver) => if executor::block_on(receiver.recv()).is_err() {},
                _ => panic!("unexpected receiver"),
            }
        }
    }
}
