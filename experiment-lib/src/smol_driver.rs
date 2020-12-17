use super::*;
use smol::{block_on, channel, future, Executor, Task};
use std::future::Future;
use std::panic::catch_unwind;
use std::thread;

// This is a smol driver. Its about as bare-bones as you can get. The setup performs all of
// the wiring and the channels perform recv and send. Only the concentrator performs any
// calaulation; its an increment and compare. This should provide a baseline performance, in
// which any additional processing can be attributed to that processing.

// A multi-threaded executor, which can be dropped
struct MultiThreadedExecutorPool {
    executor: Arc<Executor<'static>>,
    sender: Option<channel::Sender<usize>>,
    receiver: channel::Receiver<usize>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl MultiThreadedExecutorPool {
    fn new() -> Self {
        let (sender, receiver) = channel::unbounded::<usize>();
        Self {
            executor: Arc::new(Executor::new()),
            sender: Some(sender),
            receiver,
            threads: Vec::new(),
        }
    }
}

impl Default for MultiThreadedExecutorPool {
    fn default() -> Self { Self::new() }
}

impl Drop for MultiThreadedExecutorPool {
    fn drop(&mut self) { self.stop(); }
}

impl MultiThreadedExecutorPool {
    /// Start a pool of executors.
    ///
    /// # Examples
    ///
    /// ```
    /// use MultiThreadedExecutorPool;
    /// use num_cpus;
    ///
    /// let pool = MultiThreadedExecutorPool::new();
    /// let num_threads = std::cmp::max(1, num_cpus::get());
    /// pool.start(num_threads);
    /// ```
    pub fn start(&mut self, num_threads: usize) {
        for n in 1 ..= num_threads {
            let receiver = self.receiver.clone();
            let executor = self.executor.clone();
            let handle = thread::Builder::new()
                .name(format!("worker-{}", n))
                .spawn(move || {
                    catch_unwind(|| block_on(executor.run(receiver.recv()))).ok();
                })
                .expect("cannont spawn executor thread");
            self.threads.push(handle);
        }
    }

    /// Stop all executors in a pool.
    ///
    /// # Examples
    ///
    /// use MultiThreadedExecutorPool;
    ///
    /// let pool = MultiThreadedExecutorPool::new();
    /// pool.start(4);
    /// pool.stop();
    /// ```
    pub fn stop(&mut self) {
        self.sender = None;
        self.threads.drain(..).for_each(|t| t.join().ok().unwrap());
    }

    /// Spawns a task onto the executor.
    ///
    /// # Examples
    ///
    /// ```
    /// use MultiThreadedExecutorPool;
    ///
    /// let pool = MultiThreadedExecutorPool::new();
    /// pool.setup(4);
    ///
    /// let task = pool.spawn(async {
    ///     println!("Hello world");
    /// });
    /// ```
    pub fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) -> Task<T> { self.executor.spawn(future) }

    /// Returns `true` if there are no unfinished tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use MultiThreadedExecutorPool;
    ///
    /// let pool = MultiThreadedExecutorPool::new();
    /// pool.setup(4);
    ///
    /// let task = pool.spawn(async {
    ///     println!("Hello world");
    /// });
    /// // wait for all tasks to complete
    /// while !pool.is_empty() {
    ///     std::thread::sleep(std::time::Duration::from_millis(20))
    /// }
    /// ```
    pub fn is_empty(&self) -> bool { self.executor.is_empty() }
}

/// For this smol test we'll use the smol runtime, and we create a task per element.
/// So, for a 5 element pipeline with 1000 lanes, there will be 5000 task plus some for the concentrator.

#[derive(Default)]
pub struct ServerSimulator {
    pool: MultiThreadedExecutorPool,
    messages: usize,
    lanes: Vec<channel::Sender<usize>>,
    notifier: Option<channel::Receiver<usize>>,
    verbosity: Verbosity,
}

impl ServerSimulator {
    fn create_channel(config: ExperimentConfig) -> (channel::Sender<usize>, channel::Receiver<usize>) {
        if config.capacity == 0 {
            channel::unbounded::<usize>()
        } else {
            channel::bounded::<usize>(config.capacity)
        }
    }
}

impl Drop for ServerSimulator {
    fn drop(&mut self) { self.teardown(); }
}

impl ExperimentDriver for ServerSimulator {
    // get the driver name
    fn name(&self) -> &'static str { "smol" }
    // setup the driver for running an experiment
    fn setup(&mut self, config: ExperimentConfig) {
        self.messages = config.messages;
        self.verbosity = config.verbosity;
        self.pool.start(config.threads);
        let mut senders = Vec::new();
        let mut receivers = Vec::new();
        // setup the pipeline lanes, the last in each lane sends to the common concentrator
        let (concentrator_sender, concentrator_receiver) = Self::create_channel(config);
        for lane in 1 ..= config.lanes {
            for _ in 1 ..= config.pipelines {
                let (sender, receiver) = Self::create_channel(config);
                senders.push(sender);
                receivers.push(receiver);
            }
            // push the concentrator. Now, when we pop we end up with a head sender remaining
            senders.push(concentrator_sender.clone());

            for pipeline in (1 ..= config.pipelines).rev() {
                let sender = senders.pop().unwrap();
                let receiver = receivers.pop().unwrap();
                // forward whatever we receive
                let future = async move {
                    while let Ok(value) = receiver.recv().await {
                        sender.send(value).await.ok();
                    }
                    if config.verbosity == Verbosity::All {
                        println!("fwd {} lane {} closed", pipeline, lane);
                    }
                };
                self.pool.spawn(future).detach();
            }
        }
        // senders are now just the head sender of each lane, save it
        self.lanes = senders;

        // setup the final notifier, which is sent to once per-run
        let (notifier_sender, notifier_receiver) = Self::create_channel(config);
        let messages = config.messages;
        let concentrator = async move {
            let mut lane_notifications = 0;
            let mut msg_recv = 0;
            if config.verbosity != Verbosity::None {
                println!("notifier is active")
            }
            loop {
                match concentrator_receiver.recv().await {
                    Ok(value) => {
                        msg_recv += 1;
                        if config.verbosity == Verbosity::All {
                            println!("notifier received {} of {}", value, config.messages);
                        }
                        // for 5 messages, we send values 0, 1, 2, 3, 4
                        if value == messages {
                            lane_notifications += 1;
                            if config.verbosity != Verbosity::None {
                                println!("received notification {}", lane_notifications);
                            }
                            if lane_notifications == config.lanes {
                                if config.verbosity != Verbosity::None {
                                    println!("received all notifications");
                                }
                                notifier_sender.send(msg_recv).await.ok();
                                lane_notifications = 0;
                                msg_recv = 0;
                            }
                        }
                    },
                    Err(_) => {
                        if config.verbosity != Verbosity::None {
                            println!("notifier closed");
                        }
                        break;
                    },
                };
            }
            if config.verbosity != Verbosity::None {
                println!("notifier is inactive");
            }
        };
        self.pool.spawn(concentrator).detach();
        self.notifier = Some(notifier_receiver);
    }
    // teardown the driver, including threads and tasks
    fn teardown(&mut self) {
        if self.verbosity != Verbosity::None {
            println!("starting teardown for {}", self.name());
        }
        self.notifier = None;
        self.lanes.clear();
        while !self.pool.is_empty() {
            thread::sleep(std::time::Duration::from_millis(20));
        }
        if self.verbosity != Verbosity::None {
            println!("all tasks completed, {} shutdown complete", self.name());
        }
        self.pool.stop();
    }

    fn run(&self) {
        let messages = self.messages;
        // parallelize driving the lanes
        for sender in &self.lanes {
            let sender = sender.clone();
            let future = async move {
                for msg_id in 1 ..= messages {
                    sender.send(msg_id).await.unwrap();
                }
            };
            self.pool.spawn(future).detach();
        }
        // wait for the notifier to get a count
        if let Some(ref notifier) = self.notifier {
            let notifier = notifier.clone();
            let notifier = async move { notifier.recv().await };
            let task = self.pool.spawn(notifier);
            let _count = future::block_on(async { task.await }).unwrap_or(0);
        }
    }
}
