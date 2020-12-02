
use experiment_lib::*;

// Count of elements in a single daisy chain.
const PIPELINES: usize = 10;
// Count of parallel daisy chains
const LANES: usize = 1000;
// Count of messages to send into each lane
const MESSAGES: usize = 100;
// Count of how many times to inject messages into the lanes
const ITERATIONS: usize = 5;

// generic experiment
fn experiment<T: Default + ExperimentDriver>(mut driver: T) {
    driver.setup(PIPELINES, LANES, MESSAGES);
    for _ in 1 ..= ITERATIONS {
        let start = std::time::Instant::now();
        driver.run();
        let elapsed = start.elapsed();
        println!("{} completed in {:#?}", driver.name(), elapsed);
    }
    driver.teardown();
}

fn main() {
    // run each of the experiments
    //experiment(futures_daisy_chain::ServerSimulator::default());
    experiment(tokio_driver::ServerSimulator::default());
    experiment(flume_tokio_driver::ServerSimulator::default());
    experiment(flume_async_std_driver::ServerSimulator::default());
    experiment(d3_driver::ServerSimulator::default());
    experiment(crossbeam_driver::ServerSimulator::default());
}
