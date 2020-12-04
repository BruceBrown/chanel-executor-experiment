use super::*;

/// For crossbeam, we can't create a thread per element, instead we'll create a thread for the
/// Concentrator and one for each pipeline position in a lane. So, for a 5 element pipeline with
/// 1000 lanes, there will be 5+1 pipeline threads, each handling a select of 1000 receivers + a concentrator.
///  
struct Forwarder {
    inner_sync: std::sync::Mutex<SyncForwarder>,
}
impl Forwarder {
    fn new(id: usize) -> Self {
        Self {
            inner_sync: std::sync::Mutex::new(SyncForwarder::new(id)),
        }
    }
}

impl SyncReceiver<TestMessage> for Forwarder {
    fn receive(&self, cmd: TestMessage) {
        let mut mutable = self.inner_sync.lock().unwrap();
        match mutable.handle_config(cmd) {
            Ok(_) => (),
            Err(msg) => match mutable.validate_sequence(msg) {
                Ok(msg) => {
                    mutable.handle_action(msg);
                    mutable.handle_notification();
                },
                Err(msg) => panic!("sequence error fwd {}, msg {:#?}", mutable.id(), msg),
            },
        }
    }
}

#[derive(Default)]
pub struct ServerSimulator {
    messages: usize,
    heads: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str { "crossbeam" }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        self.messages = messages;
        // create the concentrator and get its recevier running in a thread
        let (concentrator_s, concentrator_r) = crossbeam::channel::bounded::<TestMessage>(250);
        let forwarder = Forwarder::new(0);

        let handle = std::thread::spawn(move || {
            while let Ok(cmd) = concentrator_r.recv() {
                forwarder.receive(cmd);
            }
        });
        self.threads.push(handle);

        // create a vector of a vector of receivers, essentially [pipelines][lanes]
        let mut recvs: Vec<Vec<crossbeam::channel::Receiver<TestMessage>>> = Vec::new();
        let mut forwarders: Vec<Vec<Forwarder>> = Vec::new();
        for _ in 1 ..= pipelines {
            recvs.push(Vec::new());
            forwarders.push(Vec::new());
        }
        for _ in 1 ..= lanes {
            let mut prev = None;
            for id in 0 .. pipelines {
                // create the forwarder's sender and receiver
                let (s, r) = crossbeam::channel::bounded::<TestMessage>(250);
                recvs[id].push(r);
                let forwarder = Forwarder::new(id + 1);
                forwarders[id].push(forwarder);
                // if first, save head, otherwise forward previous to this sender
                match prev {
                    Some(sender) => send_cmd(&sender, TestMessage::AddSender(TestMessageSender::CrossbeamSender(s.clone()))),
                    None => self.heads.push(TestMessageSender::CrossbeamSender(s.clone())),
                }
                prev = Some(TestMessageSender::CrossbeamSender(s));
            }
            // tell the last to notify the concentrator when it receives all of the messages
            let concentrator_s = TestMessageSender::CrossbeamSender(concentrator_s.clone());
            let sender = prev.unwrap();
            send_cmd(&sender, TestMessage::Notify(concentrator_s, messages));
        }
        // launch a thread for each pipeline
        for _ in 0 .. pipelines {
            let recv = recvs.remove(0);
            let forwarder = forwarders.remove(0);
            let handle = std::thread::spawn(move || {
                // build a select and add each receiver to it -- this could be 1000's
                let mut select = crossbeam::channel::Select::new();
                for r in &recv {
                    select.recv(r);
                }
                // loop waiting for a receiver to become ready
                loop {
                    let index = select.ready();
                    let r = &recv[index];
                    match r.try_recv() {
                        Ok(cmd) => forwarder[index].receive(cmd),
                        Err(crossbeam::channel::TryRecvError::Disconnected) => break,
                        Err(_) => (),
                    }
                }
            });
            self.threads.push(handle);
        }
        // finally, create a notifier, setup the concentrator to notify it and save it
        let (s, r) = crossbeam::channel::bounded::<TestMessage>(10);
        let s = TestMessageSender::CrossbeamSender(s);
        let concentrator_s = TestMessageSender::CrossbeamSender(concentrator_s);
        send_cmd(&concentrator_s, TestMessage::Notify(s, lanes));
        self.notifier = Some(TestMessageReceiver::CrossbeamReceiver(r));
    }

    fn teardown(&mut self) {
        self.heads.clear();
        self.notifier = None;
        self.threads.clear();
    }

    fn run(&mut self) {
        // send a message into each lane, repeat until we've sent all of the messages
        for id in 0 .. self.messages {
            for head in &self.heads {
                send_cmd(head, TestMessage::TestData(id));
            }
        }
        // wait for the concentrator to send the notification message
        if let Some(ref notifier) = self.notifier {
            if let TestMessageReceiver::CrossbeamReceiver(receiver) = notifier {
                if receiver.recv().is_err() {}
            }
        }
    }
}
