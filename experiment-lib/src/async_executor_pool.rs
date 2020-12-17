// A multi-threaded executor, utilizing the async_executor::executor. This threadpool can
// be dropped.

use std::future::Future;
use std::panic::catch_unwind;
use std::sync::Arc;
use std::thread;

use async_channel::{unbounded, Receiver, Sender};
use async_executor::Executor;
use async_task::Task;
use futures_lite::future::block_on;

pub struct MultiThreadedAsyncExecutorPool {
    executor: Arc<Executor<'static>>,
    sender: Option<Sender<usize>>,
    receiver: Receiver<usize>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl MultiThreadedAsyncExecutorPool {
    fn new() -> Self {
        let (sender, receiver) = unbounded::<usize>();
        Self {
            executor: Arc::new(Executor::new()),
            sender: Some(sender),
            receiver,
            threads: Vec::new(),
        }
    }
}

impl Default for MultiThreadedAsyncExecutorPool {
    fn default() -> Self { Self::new() }
}

impl Drop for MultiThreadedAsyncExecutorPool {
    fn drop(&mut self) { self.stop(); }
}

impl MultiThreadedAsyncExecutorPool {
    /// Start a pool of executors.
    ///
    /// # Examples
    ///
    /// ```
    /// use MultiThreadedAsyncExecutorPool;
    /// use num_cpus;
    ///
    /// let pool = MultiThreadedAsyncExecutorPool::new();
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
    /// use MultiThreadedAsyncExecutorPool;
    ///
    /// let pool = MultiThreadedAsyncExecutorPool::new();
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
    /// use MultiThreadedAsyncExecutorPool;
    ///
    /// let pool = MultiThreadedAsyncExecutorPool::new();
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
    /// use MultiThreadedAsyncExecutorPool;
    ///
    /// let pool = MultiThreadedAsyncExecutorPool::new();
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
