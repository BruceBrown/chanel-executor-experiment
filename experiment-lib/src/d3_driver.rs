use super::*;
use d3::core::machine_impl::Machine;

/// For d3, we create a task per element. So, for a 10 element pipeline with
/// 1000 lanes, there will be 4000 task plus some for the concentrator. D3
/// expects a Machine<T> implementation and calls into it when messages are
/// received. It hides all of the async, so that it appears to be sync.
pub struct Forwarder {
    inner_sync: std::sync::Mutex<SyncForwarder>,
}

impl Forwarder {
    fn new(id: usize) -> Self {
        Self {
            inner_sync: std::sync::Mutex::new(SyncForwarder::new(id)),
        }
    }
}

impl Machine<TestMessage> for Forwarder {
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
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str { "d3" }

    fn setup(&mut self, pipelines: usize, lanes: usize, messages: usize) {
        self.messages = messages;
        d3::core::executor::start_server();
        let (_, concentrator_s) = d3::core::executor::connect_with_capacity(Forwarder::new(0), 250);

        for _ in 1 ..= lanes {
            let mut prev = None;
            for id in 1 ..= pipelines {
                let (_, s) = d3::core::executor::connect_with_capacity(Forwarder::new(id), 250);
                if prev.is_none() {
                    self.heads.push(TestMessageSender::D3Sender(s.clone()));
                } else {
                    let sender = prev.unwrap();
                    send_cmd(&sender, TestMessage::AddSender(TestMessageSender::D3Sender(s.clone())));
                }
                prev = Some(TestMessageSender::D3Sender(s));
            }
            send_cmd(
                &prev.unwrap(),
                TestMessage::Notify(TestMessageSender::D3Sender(concentrator_s.clone()), messages),
            );
        }
        let (s, r) = d3::core::machine_impl::channel_with_capacity::<TestMessage>(10);
        let s = TestMessageSender::D3Sender(s);
        let concentrator_s = TestMessageSender::D3Sender(concentrator_s);
        send_cmd(&concentrator_s, TestMessage::Notify(s, lanes));
        self.notifier = Some(TestMessageReceiver::D3Receiver(r));
    }

    fn teardown(&mut self) {
        self.heads.clear();
        self.notifier = None;
        d3::core::executor::stop_server();
    }

    fn run(&mut self) {
        for id in 0 .. self.messages {
            for lane in &self.heads {
                send_cmd(lane, TestMessage::TestData(id));
            }
        }
        if let Some(ref notifier) = self.notifier {
            match notifier {
                TestMessageReceiver::D3Receiver(receiver) => if receiver.recv().is_err() {},
                _ => (),
            }
        }
    }
}
