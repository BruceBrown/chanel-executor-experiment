// Constants, Configuration, and Traits used by the experiments.
//

/// Default capacity of the message queue, 0 is unbounded.
const QUEUE_CAPACITY: usize = 5000;

/// Default count of elements in a single daisy chain.
const PIPELINES: usize = 5;

/// Default count of parallel daisy chains.
const LANES: usize = 4000;

/// Default count of messages to send into each lane.
const MESSAGES: usize = 100;

/// Default count of how many times to inject messages into the lanes.
const ITERATIONS: usize = 1;

/// Default count of threads the async executor will use
const ASYNC_THREAD_COUNT: usize = 1;

/// Logging vebosity
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Verbosity {
    None,
    All,
    Notify,
}
/// Default logging vebosity
const VERBOSITY: Verbosity = Verbosity::None;

impl Default for Verbosity {
    fn default() -> Self { VERBOSITY }
}

/// Configuration for the experiment.
#[derive(Debug, Copy, Clone)]
pub struct ExperimentConfig {
    /// Capacity of the message queue, zero is unbounded.
    pub capacity: usize,

    /// Count of forwarders in a lane.
    pub pipelines: usize,

    /// Count of lanes
    pub lanes: usize,

    /// Count of messages per lane
    pub messages: usize,

    /// Count of iterations of an experiment
    pub iterations: usize,

    /// Count of threads the async executor will use
    pub threads: usize,

    /// Verbosity level, mostly used for debugging
    pub verbosity: Verbosity,
}

impl Default for ExperimentConfig {
    fn default() -> Self {
        Self {
            capacity: QUEUE_CAPACITY,
            pipelines: PIPELINES,
            lanes: LANES,
            messages: MESSAGES,
            iterations: ITERATIONS,
            threads: ASYNC_THREAD_COUNT,
            verbosity: VERBOSITY,
        }
    }
}

/// Configuration for the experiments and vector of experiment drivers.
#[derive(Default)]
pub struct ExperimentDrivers {
    pub config: ExperimentConfig,
    pub drivers: Vec<Box<dyn ExperimentDriver>>,
}

// Each experiment must implement the ExperimentDriver trait
pub trait ExperimentDriver {
    fn name(&self) -> &'static str;
    fn setup(&mut self, config: ExperimentConfig);
    fn teardown(&mut self);
    fn run(&self);
}
