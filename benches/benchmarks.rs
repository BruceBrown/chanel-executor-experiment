use criterion::{criterion_group, criterion_main, Criterion};
use num_cpus;
use std::sync::{Arc, Mutex};

use experiment_lib::*;

const PIPELINES: usize = 5;
const MESSAGES: usize = 100;

// return a vector of (max_lanes, drivers) to test
fn drivers() -> Vec<(usize, Arc<Mutex<dyn ExperimentDriver>>)> {
    vec![
        (
            5000,
            Arc::new(Mutex::new(smol_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
        (
            5000,
            Arc::new(Mutex::new(crossbeam_async_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
        (
            5000,
            Arc::new(Mutex::new(tokio_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
        (
            1000,
            Arc::new(Mutex::new(crossbeam_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
        (
            5000,
            Arc::new(Mutex::new(flume_tokio_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
        (
            5000,
            Arc::new(Mutex::new(flume_async_std_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
        (
            5000,
            Arc::new(Mutex::new(d3_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
        ),
    ]
}

// gather stats for the drivers at a parameterize lane count
fn gather(lanes: usize, group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    let drivers = drivers();
    for (max_lanes, driver) in drivers {
        if lanes <= max_lanes {
            driver.lock().unwrap().setup(PIPELINES, lanes, MESSAGES);
            let name = driver.lock().unwrap().name().to_string();
            let mut runner = driver.lock().unwrap();
            group.bench_function(name, |b| b.iter(|| runner.run()));
            drop(runner);
            driver.lock().unwrap().teardown();
        }
    }
}

// run the benchmarks for different lany sizes
pub fn bench(c: &mut Criterion) {
    println!("running with {} CPUs", num_cpus::get());

    let lane_counts = vec![250, 500, 1000, 2000, 4000];
    for lanes in lane_counts {
        let group_name = format!("pipeline={}, lanes={}, messages={}", PIPELINES, lanes, MESSAGES);
        let mut group = c.benchmark_group(group_name);
        gather(lanes, &mut group);
        group.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
