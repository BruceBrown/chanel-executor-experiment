# A Series of Experiments Comparing Different Async and Sync Channel Executors

[![Build Status](https://github.com/BruceBrown/channel-executor-experiment/workflows/Rust/badge.svg)](
https://github.com/brucebrown/channel-executor-experiment/actions)
[![Test Status](https://github.com/BruceBrown/channel-executor-experiment/workflows/Tests/badge.svg)](
https://github.com/brucebrown/channel-executor-experiment/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](
https://github.com/BruceBrown/channel-executor-experiment#license)
[![Cargo](https://img.shields.io/crates/v/channel-executor-experiment.svg)](
https://crates.io/crates/channel-executor-experiment)
[![Documentation](https://docs.rs/channel-executor-experiment/badge.svg)](
https://docs.rs/channel-executor-experiment)
[![Rust 1.47+](https://img.shields.io/badge/rust-1.47+-color.svg)](
https://www.rust-lang.org)

## Goals

The primary goal is to provide a performance comparison between various as async channel implementations alongside
various async executors.

## The Experiment
A server simulation will be used for bench marks. I consists of a pipeline of channels,
where the first forwards to the second and in turn it forwards to the third, etc. A number of pipelines are created,
forming lanes and are driven in parallel. Each tail end of the pipeline sends to a concentrator, which sends to
a notifier channel when all lanes have completed running. The elapsed time from when the lanes sere sent to until
the notification is received is measured to get a relative performance indication.

Experiments will be run that varry the message queue length and number of lanes. A baseline experiment will establish the performance
of a minimal experiment. Then all other experiments will be compared in order to draw some conclusions.

## The Forwarder
A Forwarder struct will be used for simulating pipeline steps. It receives a variant and interprets it as either configuration or data.
The configuation messages provides a means of injecting a forwarding sender or a counted notifier. Whereas the data messages provide a
sequenced stream of messages.

## Fairness
Other than the baseline implementation, which is a bare-bones receive and forward, all implementations will use the same template
for setting up the pipeline, lanes, and notification. Allowances will be made for awaiting on channel send and receive as well as for
creating and scheduling tasks.

## Drivers
Each experiment implements a driver, responsible to setup, teardown and running an iteration of the experiment. The following drivers
are available for testing:
* smol, a driver using smol::channel and smol::executor.
* async_channel, based upon smol, this uses the latest version of the async_channel, async_executor, etc.
* flume_async, using a flume channel and async_executor.
* flume_tokio, using a flume channel and tokio executor.
* forwarder, using smol channel and smol executor, this provides a relative comparison of the overhead incurred by the common Forwarder implementation.
* tokio, using tokio channel and tokio executor.

## Compatibility

channel-executor-experiment supports stable Rust releases going back at least six months,
and every time the minimum supported Rust version is increased, a new minor
version is released. Currently, the minimum supported Rust version is 1.47.

## Contributing

channel-executor-experiment welcomes contribution from everyone in the form of suggestions, bug reports,
pull requests, and feedback. 💛

If you need ideas for contribution, there are several ways to get started:
* Found a bug or have a feature request?
[Submit an issue](https://github.com/brucebrown/channel-executor-experiment/issues/new)!
* Issues and PRs labeled with
[feedback wanted](https://github.com/brucebrown/channel-executor-experiment/issues?utf8=%E2%9C%93&q=is%3Aopen+sort%3Aupdated-desc+label%3A%22feedback+wanted%22+)
* Issues labeled with
  [good first issue](https://github.com/brucebrown/channel-executor-experiment/issues?q=is%3Aissue+is%3Aopen+sort%3Aupdated-desc+label%3A%22good+first+issue%22)
  are relatively easy starter issues.

## Learning Resources

If you'd like to learn more read our [wiki](https://github.com/brucebrown/channel-executor/wiki)

## Conduct

The channel-executor-experiment project adheres to the
[Rust Code of Conduct](https://github.com/rust-lang/rust/blob/master/CODE_OF_CONDUCT.md).
This describes the minimum behavior expected from all contributors.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.


## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
