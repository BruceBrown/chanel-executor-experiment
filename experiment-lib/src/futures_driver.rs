use super::*;
use futures::channel::mpsc;
use futures::executor::ThreadPool;

/// For this futures test we'll use futures channels and futures runtime, and we create a task per element.
/// So, for a 10 element pipeline with 1000 lanes, there will be 4000 task plus some for the concentrator.
/// 
/// Unfortunately, this isn't working. See "todo: (futures)" for details. Bascially, we need a way to
/// await on a Sender's send, and a wait to await on a Receiver's recv. Perhaps we need to use Sink and
/// Stream traits. 
struct Forwarder {
    inner_async: async_std::sync::Mutex<AsyncForwarder>,
}

impl Forwarder {
    pub fn new(id: usize) -> Self {
        Self {
            inner_async: async_std::sync::Mutex::new(AsyncForwarder::new(id)),
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
                }
                Err(msg) => panic!("sequence error fwd {}, msg {:#?}", mutable.id(), msg),
            },
        }
    }
}

pub struct ServerSimulator {
    executor: ThreadPool,
    messages: usize,
    heads: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
    tasks: Vec<async_std::task::JoinHandle<()>>,
}

impl Default for ServerSimulator {
    fn default() -> Self {
        Self {
            executor: ThreadPool::new().unwrap(),
            messages: 0,
            heads: Vec::default(),
            notifier: None,
            tasks: Vec::default(),
        }
    }
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str {
        "futures"
    }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        self.messages = messages;
        let (concentrator_s, mut concentrator_r) = mpsc::channel::<TestMessage>(250);
        let forwarder = Forwarder::new(0);

        self.executor.spawn_ok(async move {
            loop {
                // todo: (futures) This is likely wrong. Need to await on Receiver concentrator_r.
                match concentrator_r.try_next() {
                    Ok(Some(cmd)) => forwarder.receive(cmd).await,
                    Ok(None) => break,
                    Err(mpsc::TryRecvError{..}) => break,
                }
            }
        });

        for _ in 1..=lanes {
            let mut prev = None;
            for id in 1..=pipelines {
                let (s, mut r) = mpsc::channel::<TestMessage>(250);
                let forwarder = Forwarder::new(id);
                self.executor.spawn_ok(async move {
                    loop {
                        // todo: (futures) This is likely wrong. Need to await on Receiver r.
                        match r.try_next() {
                            Ok(Some(cmd)) => forwarder.receive(cmd).await,
                            Ok(None) => break,
                            Err(mpsc::TryRecvError{..}) => break,
                        }
                    }
                });
                if prev.is_none() {
                    self.heads.push(TestMessageSender::FuturesSender(s.clone()));
                } else {
                    let sender = prev.unwrap();
                    let s = s.clone();
                    self.executor.spawn_ok(async move {
                        send_cmd_async( &sender, TestMessage::AddSender(TestMessageSender::FuturesSender(s))).await;
                    });
                }
                prev = Some(TestMessageSender::FuturesSender(s));
            }
            let concentrator_s = concentrator_s.clone();
            self.executor.spawn_ok(async move {
                send_cmd_async(
                    &prev.unwrap(),
                    TestMessage::Notify(TestMessageSender::FuturesSender(concentrator_s), messages)
                    ).await;
            });
        }
        let (s, r) = mpsc::channel::<TestMessage>(10);
        let s = TestMessageSender::FuturesSender(s);
        let concentrator_s = TestMessageSender::FuturesSender(concentrator_s);
        futures::executor::block_on( async move {
        send_cmd_async(
            &concentrator_s,
            TestMessage::Notify(s, lanes),
        ).await;});

        self.notifier = Some(TestMessageReceiver::FuturesReceiver(r));
    }

    fn teardown(&mut self) {
        self.heads.clear();
        self.notifier = None;
        self.tasks.clear();
    }

    fn run(&mut self) {
        let heads = self.heads.clone();
        let messages = self.messages;
        if let Some(ref mut notifier) = self.notifier {
            futures::executor::block_on( async move {
                for id in 0..messages {
                    for head in &heads {
                        send_cmd_async(head, TestMessage::TestData(id)).await;
                    }
                }
                match notifier {
                    TestMessageReceiver::FuturesReceiver(ref mut receiver) => {
                        // todo: (futures) This is likely wrong. Need to await on Receiver receiver.
                        match receiver.try_next() {
                            Ok(Some(_cmd)) => println!("got notification"),
                            Ok(None) => println!("got None"),
                            Err(mpsc::TryRecvError{..}) => println!("got TryRecvError"),
                        }       
                    }
                    _ => panic!("unexpected receiver"),
                }
            });
        }
    }
}
