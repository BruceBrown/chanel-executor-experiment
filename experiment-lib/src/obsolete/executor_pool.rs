use std::future::Future;
use std::sync::Arc;
use std::thread;
use std::panic::catch_unwind;

use smol::{block_on, channel, Executor, Task};

pub struct MultiThreadedExecutorPool {
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
    fn default() -> Self {
        Self::new()
    }
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
        for n in 1..=num_threads {
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
        self.threads
            .drain(..)
            .for_each(|t| t.join().ok().unwrap());
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
    pub fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) -> Task<T> {
        self.executor.spawn(future)
    }
}
