use super::*;

#[derive(Default)]
pub struct ServerSimulator {
    messages: usize,
    heads: Vec<TestMessageSender>,
    notifier: Option<TestMessageReceiver>,
}

impl ExperimentDriver for ServerSimulator {
    fn name(&self) -> &str { "d3" }

    fn setup(&mut self, config: ExperimentConfig) {
        self.messages = config.messages;
        d3::core::executor::start_server();
        let (_, concentrator_s) = d3::core::executor::connect_with_capacity(Forwarder::new(0,0), config.capacity);

        for lane in 1 ..= config.lanes {
            let mut prev = None;
            for id in 1 ..= config.pipelines {
                let (_, s) = d3::core::executor::connect_with_capacity(Forwarder::new(id, lane), config.capacity);
                // if first, save head, otherwise forward previous to this sender
                match prev {
                    Some(sender) => send_cmd(&sender, TestMessage::AddSender(TestMessageSender::D3Sender(s.clone()))),
                    None => self.heads.push(TestMessageSender::D3Sender(s.clone())),
                }
                prev = Some(TestMessageSender::D3Sender(s));
            }
            send_cmd(
                &prev.unwrap(),
                TestMessage::Notify(TestMessageSender::D3Sender(concentrator_s.clone()), config.messages),
            );
        }
        let (s, r) = d3::core::machine_impl::channel_with_capacity::<TestMessage>(10);
        let s = TestMessageSender::D3Sender(s);
        let concentrator_s = TestMessageSender::D3Sender(concentrator_s);
        send_cmd(&concentrator_s, TestMessage::Notify(s, config.lanes));
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
            if let TestMessageReceiver::D3Receiver(receiver) = notifier {
                if receiver.recv().is_err() {}
            }
        }
    }
}
