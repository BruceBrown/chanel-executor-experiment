use experiment_lib::*;

// Count of elements in a single daisy chain.
const PIPELINES: usize = 10;
// Count of parallel daisy chains
const LANES: usize = 500;
// Count of messages to send into each lane
const MESSAGES: usize = 100;
// Count of how many times to inject messages into the lanes
const ITERATIONS: usize = 50;

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
    println!("pausing for 15 seconds, cpu should go to near zero");
    std::thread::sleep(std::time::Duration::from_secs(15))
}

fn main() {
    // run each of the experiments
    println!(
        "running with pipline count of {}, lanes {}, messages per lane {}, iterations {}",
        PIPELINES, LANES, MESSAGES, ITERATIONS
    );
    // experiment(futures_daisy_chain::ServerSimulator::default());
    experiment(smol_driver::ServerSimulator::default());
    experiment(crossbeam_async_driver::ServerSimulator::default());
    experiment(tokio_driver::ServerSimulator::default());
    experiment(flume_tokio_driver::ServerSimulator::default());
    experiment(flume_async_std_driver::ServerSimulator::default());
    experiment(d3_driver::ServerSimulator::default());
    experiment(crossbeam_driver::ServerSimulator::default());
}
