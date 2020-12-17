use criterion::{criterion_group, criterion_main, Criterion};
use num_cpus;

use experiment_lib::*;

// return a vector of (max_lanes, drivers) to test
fn drivers() -> Vec<(usize, Box<dyn ExperimentDriver>)> {
    vec![
        (
            5000,
            Box::new(async_channel_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>,
        ),
        (
            5000,
            Box::new(flume_async_executor_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>,
        ),
        (
            5000,
            Box::new(flume_tokio_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>,
        ),
        (
            5000,
            Box::new(forwarder_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>,
        ),
        (
            5000,
            Box::new(smol_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>,
        ),
        (
            5000,
            Box::new(tokio_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>,
        ),
    ]
}

// gather stats for the drivers at a parameterize lane count
fn gather(capacity: usize, lanes: usize, group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    let drivers = drivers();
    for (max_lanes, mut driver) in drivers {
        if lanes <= max_lanes {
            let config = ExperimentConfig {
                capacity,
                lanes,
                ..ExperimentConfig::default()
            };
            driver.setup(config);
            let name = driver.name().to_string();
            group.bench_function(name, |b| b.iter(|| driver.run()));
            drop(driver);
        }
    }
}

// run the benchmarks for different lany sizes
pub fn bench(c: &mut Criterion) {
    println!("running with {} CPUs", num_cpus::get());

    let channel_capacity = vec![0, 50, 250, 5000];
    for capacity in channel_capacity {
        let lane_counts = vec![250, 500, 1000, 4000];
        for lanes in lane_counts {
            let group_name = format!("capacity={}, lanes={}", capacity, lanes);
            let mut group = c.benchmark_group(group_name);
            gather(capacity, lanes, &mut group);
            group.finish();
        }
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
