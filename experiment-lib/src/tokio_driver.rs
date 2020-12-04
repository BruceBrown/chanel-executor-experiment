use super::*;
/// For tokio, we create a task per element. So, for a 5 element pipeline with
/// 1000 lanes, there will be 5000 task plus some for the concentrator.
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
    fn name(&self) -> &str { "tokio" }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        let rt = &self.runtime;
        self.messages = messages;
        // create the concentrator and get its recevier running in a thread
        let (concentrator_s, mut concentrator_r) = tokio::sync::mpsc::channel::<TestMessage>(250);
        let forwarder = Forwarder::new(0);

        let task = rt.spawn(async move {
            while let Some(cmd) = concentrator_r.recv().await {
                forwarder.receive(cmd).await;
            }
        });
        self.tasks.push(task);
        // create all of the forwarders for all of the lanes
        for _ in 1 ..= lanes {
            let mut prev = None;
            for id in 1 ..= pipelines {
                // create the forwarder's sender and receiver
                let (s, mut r) = tokio::sync::mpsc::channel::<TestMessage>(250);
                let forwarder = Forwarder::new(id);
                let task = rt.spawn(async move {
                    while let Some(cmd) = r.recv().await {
                        forwarder.receive(cmd).await;
                    }
                });
                self.tasks.push(task);
                // if first, save head, otherwise forward previous to this sender
                match prev {
                    Some(sender) => {
                        let s = s.clone();
                        rt.spawn(async move {
                            send_cmd_async(&sender, TestMessage::AddSender(TestMessageSender::TokioSender(s.clone()))).await;
                        });
                    },
                    None => self.heads.push(TestMessageSender::TokioSender(s.clone())),
                }
                prev = Some(TestMessageSender::TokioSender(s));
            }
            // tell the last to notify the concentrator when it receives all of the messages
            let concentrator_s = TestMessageSender::TokioSender(concentrator_s.clone());
            let sender = prev.unwrap();
            rt.spawn(async move {
                send_cmd_async(&sender, TestMessage::Notify(concentrator_s, messages)).await;
            });
        }
        // finally, create a notifier, setup the concentrator to notify it and save it
        let (s, r) = tokio::sync::mpsc::channel::<TestMessage>(10);
        let s = TestMessageSender::TokioSender(s);
        let concentrator_s = TestMessageSender::TokioSender(concentrator_s);
        rt.spawn(async move {
            send_cmd_async(&concentrator_s, TestMessage::Notify(s, lanes)).await;
        });
        self.notifier = Some(TestMessageReceiver::TokioReceiver(r));
    }

    fn teardown(&mut self) {
        self.heads.clear();
        self.notifier = None;
        self.tasks.clear();
    }

    fn run(&mut self) {
        let rt = &self.runtime;
        let heads = self.heads.clone();
        let messages = self.messages;
        rt.spawn(async move {
            // send a message into each lane, repeat until we've sent all of the messages
            for id in 0 .. messages {
                for head in &heads {
                    send_cmd_async(head, TestMessage::TestData(id)).await;
                }
            }
        });
        // wait for the concentrator to send the notification message
        if let Some(ref mut notifier) = self.notifier {
            match notifier {
                TestMessageReceiver::TokioReceiver(ref mut receiver) => {
                    rt.block_on(receiver.recv());
                },
                _ => panic!("unexpected receiver"),
            }
        }
    }
}
