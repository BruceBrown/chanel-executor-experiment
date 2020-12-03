use super::*;
/// AsyncForward implements an async forwarder which can forward received message and notify
/// a Receiver when it has received a configured number of messages. There is a similar
/// SyncForwarder which mirrors this, as sync.
pub struct AsyncForwarder {
    /// a id, mosly used for logging
    id: usize,
    /// collection of senders, each will be sent any received message.
    senders: Vec<TestMessageSender>,
    /// received_count is the count of messages received by this forwarder.
    received_count: usize,
    /// send_count is the count of messages sent by this forwarder.
    send_count: usize,
    /// notify_count is compared against received_count for means of notifcation.
    notify_count: usize,
    /// notify_sender is sent a TestData message with the data being the number of messages received.
    notify_sender: Option<TestMessageSender>,
    /// forwarding multiplier
    forwarding_multiplier: usize,
    // for TestData, this is the next in sequence
    next_seq: usize,
}
impl AsyncForwarder {
    /// construct a new AsyncForwarder
    pub const fn new(id: usize) -> Self {
        Self {
            id,
            senders: Vec::<TestMessageSender>::new(),
            received_count: 0,
            send_count: 0,
            notify_count: 0,
            notify_sender: None,
            forwarding_multiplier: 1,
            next_seq: 0,
        }
    }

    /// get the id
    pub const fn id(&self) -> usize { self.id }

    /// if msg is TestData, validate the sequence or reset if 0
    pub fn validate_sequence(&mut self, msg: TestMessage) -> Result<TestMessage, TestMessage> {
        // if no senders, accept it and handle notification
        if self.senders.is_empty() {
            self.received_count += 1;
            return Ok(msg);
        }
        match msg {
            TestMessage::TestData(seq) if seq == self.next_seq => self.next_seq += 1,
            TestMessage::TestData(seq) if seq == 0 => self.next_seq = 1,
            TestMessage::TestData(_) => return Err(msg),
            _ => (),
        }
        // bump received count
        self.received_count += 1;
        Ok(msg)
    }

    /// If msg is a configuration msg, handle it otherwise return it as an error
    pub fn handle_config(&mut self, msg: TestMessage) -> Result<(), TestMessage> {
        match msg {
            TestMessage::Notify(sender, on_receive_count) => {
                self.notify_sender = Some(sender);
                self.notify_count = on_receive_count;
            },
            TestMessage::AddSender(sender) => {
                self.senders.push(sender);
            },
            msg => return Err(msg),
        }
        Ok(())
    }

    /// handle the action messages
    pub async fn handle_action(&mut self, cmd: TestMessage) {
        match cmd {
            TestMessage::TestData(_) => {
                for sender in &self.senders {
                    for _ in 0 .. self.forwarding_multiplier {
                        send_cmd_async(sender, TestMessage::TestData(self.send_count)).await;
                        self.send_count += 1;
                    }
                }
            },
            _ => panic!("unhandled action"),
        }
    }

    /// handle sending out a notification and resetting counters when notificaiton is sent
    pub async fn handle_notification(&mut self) {
        if self.received_count == self.notify_count {
            let count = self.get_and_clear_received_count();
            if let Some(notifier) = self.notify_sender.as_ref() {
                send_cmd_async(notifier, TestMessage::TestData(count)).await;
            }
        }
    }

    /// get the current received count and clear counters
    fn get_and_clear_received_count(&mut self) -> usize {
        let received_count = self.received_count;
        self.received_count = 0;
        self.send_count = 0;
        received_count
    }
}
