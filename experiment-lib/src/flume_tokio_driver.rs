use super::*;
/// For this flume test we'll use flume channels and tokio runtime, and we create a task per element.
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
    runtime: Arc<tokio::runtime::Runtime>,
    messages: usize,
    heads: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Default for ServerSimulator {
    fn default() -> Self {
        Self {
            runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
            messages: 0,
            heads: Vec::default(),
            notifier: None,
            tasks: Vec::default(),
        }
    }
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str { "flume w/ tokio" }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        let rt = &self.runtime;
        self.messages = messages;
        let (concentrator_s, concentrator_r) = flume::bounded::<TestMessage>(250);
        let forwarder = Forwarder::new(0);

        let task = rt.spawn(async move {
            loop {
                let cmd = concentrator_r.recv_async().await;
                if cmd.is_ok() {
                    forwarder.receive(cmd.unwrap()).await;
                } else {
                    break;
                }
            }
        });
        self.tasks.push(task);

        for _ in 1 ..= lanes {
            let mut prev = None;
            for id in 1 ..= pipelines {
                let (s, r) = flume::bounded::<TestMessage>(250);
                let forwarder = Forwarder::new(id);
                let task = rt.spawn(async move {
                    loop {
                        let cmd = r.recv_async().await;
                        if cmd.is_ok() {
                            forwarder.receive(cmd.unwrap()).await;
                        } else {
                            break;
                        }
                    }
                });
                self.tasks.push(task);
                if prev.is_none() {
                    self.heads.push(TestMessageSender::FlumeSender(s.clone()));
                } else {
                    let sender = prev.unwrap();
                    rt.block_on(send_cmd_async(
                        &sender,
                        TestMessage::AddSender(TestMessageSender::FlumeSender(s.clone())),
                    ));
                }
                prev = Some(TestMessageSender::FlumeSender(s));
            }
            rt.block_on(send_cmd_async(
                &prev.unwrap(),
                TestMessage::Notify(TestMessageSender::FlumeSender(concentrator_s.clone()), messages),
            ));
        }
        let (s, r) = flume::bounded::<TestMessage>(10);
        let s = TestMessageSender::FlumeSender(s);
        let concentrator_s = TestMessageSender::FlumeSender(concentrator_s);
        rt.block_on(send_cmd_async(&concentrator_s, TestMessage::Notify(s, lanes)));
        self.notifier = Some(TestMessageReceiver::FlumeReceiver(r));
    }

    fn teardown(&mut self) {
        self.heads.clear();
        self.notifier = None;
        self.tasks.clear();
    }

    fn run(&mut self) {
        let rt = &self.runtime;
        for id in 0 .. self.messages {
            for head in &self.heads {
                rt.block_on(send_cmd_async(head, TestMessage::TestData(id)));
            }
        }
        if let Some(ref mut notifier) = self.notifier {
            match notifier {
                TestMessageReceiver::FlumeReceiver(ref mut receiver) => {
                    if rt.block_on(receiver.recv_async()).is_err() {}
                },
                _ => panic!("unexpected receiver"),
            }
        }
    }
}
