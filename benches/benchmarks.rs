use std::sync::{Arc, Mutex};
use criterion::{criterion_group, criterion_main, Criterion};

use experiment_lib::*;

const PIPELINES: usize = 5;
const MESSAGES: usize = 100;

// return n avecotr of drivers to test
fn drivers() -> Vec<Arc<Mutex<dyn ExperimentDriver>>> {
    vec![
    Arc::new(Mutex::new(tokio_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
    Arc::new(Mutex::new(crossbeam_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
    Arc::new(Mutex::new(flume_tokio_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
    Arc::new(Mutex::new(flume_async_std_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
    Arc::new(Mutex::new(d3_driver::ServerSimulator::default())) as Arc<Mutex<dyn ExperimentDriver>>,
    ]
}

// gather stats for the drivers at a parameterize lane count
fn gather(lanes: usize, group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    let drivers = drivers();
    for driver in drivers {
        driver.lock().unwrap().setup(PIPELINES, lanes, MESSAGES);
        let name = driver.lock().unwrap().name().to_string();
        let mut runner = driver.lock().unwrap();
        group.bench_function(name, |b| b.iter(|| runner.run()));
        drop(runner);
        driver.lock().unwrap().teardown();
    }
}

// run the benchmarks for different lany sizes
pub fn bench(c: &mut Criterion) {
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
