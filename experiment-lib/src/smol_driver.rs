use super::*;

/// For this smol test we'll use the smol runtime, and we create a task per element.
/// So, for a 5 element pipeline with 1000 lanes, there will be 5000 task plus some for the concentrator.
///
/// There may be an issue with the way I setup the multi-threaded executor.
struct Forwarder {
    inner_async: smol::lock::Mutex<AsyncForwarder>,
}

impl Forwarder {
    pub const fn new(id: usize) -> Self {
        Self {
            inner_async: smol::lock::Mutex::new(AsyncForwarder::new(id)),
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
    executor: Arc<smol::Executor<'static>>,
    signal: smol::channel::Sender<()>,
    messages: usize,
    heads: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
    threads: Vec<std::thread::JoinHandle<()>>,
    tasks: Vec<smol::Task<()>>,
}

impl Default for ServerSimulator {
    fn default() -> Self {
        let executor = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let num_threads = num_cpus::get().max(1);
        let mut threads = Vec::default();
        for _ in 0 .. num_threads {
            let ex = executor.clone();
            let s = shutdown.clone();
            let handle: std::thread::JoinHandle<()> = std::thread::spawn(move || {
                let task = ex.spawn(async move { if s.recv().await.is_err() {} });
                smol::block_on(ex.run(task));
            });
            threads.push(handle);
        }
        Self {
            executor,
            signal,
            messages: 0,
            heads: Vec::default(),
            notifier: None,
            threads,
            tasks: Vec::default(),
        }
    }
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str { "smol" }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        self.messages = messages;
        let (concentrator_s, concentrator_r) = smol::channel::bounded::<TestMessage>(250);
        let forwarder = Forwarder::new(0);

        let task = self.executor.spawn(async move {
            while let Ok(cmd) = concentrator_r.recv().await {
                forwarder.receive(cmd).await;
            }
        });
        self.tasks.push(task);
        for _ in 1 ..= lanes {
            let mut prev = None;
            for id in 1 ..= pipelines {
                let (s, r) = smol::channel::bounded::<TestMessage>(250);
                let forwarder = Forwarder::new(id);
                let task = self.executor.spawn(async move {
                    while let Ok(cmd) = r.recv().await {
                        forwarder.receive(cmd).await;
                        while let Ok(cmd) = r.try_recv() {
                            forwarder.receive(cmd).await;
                        }
                    }
                });
                self.tasks.push(task);
                // if first, save head, otherwise forward previous to this sender
                match prev {
                    Some(sender) => smol::block_on(send_cmd_async(
                        &sender,
                        TestMessage::AddSender(TestMessageSender::SmolSender(s.clone())),
                    )),
                    None => self.heads.push(TestMessageSender::SmolSender(s.clone())),
                }
                prev = Some(TestMessageSender::SmolSender(s));
            }
            smol::block_on(send_cmd_async(
                &prev.unwrap(),
                TestMessage::Notify(TestMessageSender::SmolSender(concentrator_s.clone()), messages),
            ));
        }
        let (s, r) = smol::channel::bounded::<TestMessage>(10);
        let s = TestMessageSender::SmolSender(s);
        let concentrator_s = TestMessageSender::SmolSender(concentrator_s);
        smol::block_on(send_cmd_async(&concentrator_s, TestMessage::Notify(s, lanes)));
        self.notifier = Some(TestMessageReceiver::SmolReceiver(r));
    }

    fn teardown(&mut self) {
        let (signal, _) = smol::channel::unbounded::<()>();
        self.threads.clear();
        self.signal = signal;
        self.heads.clear();
        self.notifier = None;
    }

    fn run(&mut self) {
        for id in 0 .. self.messages {
            for head in &self.heads {
                smol::block_on(send_cmd_async(head, TestMessage::TestData(id)));
            }
        }
        if let Some(ref mut notifier) = self.notifier {
            match notifier {
                TestMessageReceiver::SmolReceiver(ref mut receiver) => if smol::block_on(receiver.recv()).is_err() {},
                _ => panic!("unexpected receiver"),
            }
        }
    }
}
