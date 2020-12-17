use experiment_lib::*;

// generic experiment
fn experiment(mut driver: Box<dyn ExperimentDriver>, config: ExperimentConfig, pause: bool) {
    driver.setup(config);
    for _ in 1 ..= config.iterations {
        let start = std::time::Instant::now();
        driver.run();
        let elapsed = start.elapsed();
        println!("{} completed in {:#?}", driver.name(), elapsed);
    }
    drop(driver);
    if pause {
        println!("pausing for 5 seconds, cpu should go to near zero");
        std::thread::sleep(std::time::Duration::from_secs(5))
    }
}

fn runner(mut drivers: ExperimentDrivers) {
    println!("running with configuration {:#?}", drivers.config);

    for driver in drivers.drivers.drain(..) {
        experiment(driver, drivers.config, true);
    }
}

fn main() {
    // get the configuration for the experiments and run them. Report any parse errors.
    match get_config() {
        Ok(config) => runner(config),
        Err(err) => println!("{}", err),
    }
}
