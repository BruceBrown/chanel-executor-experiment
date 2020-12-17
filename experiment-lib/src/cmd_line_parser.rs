use super::*;
use std::env;

#[derive(Debug, Clone)]
pub enum ParseError {
    MissingValue(String),
    UnknownKeyword(String),
    ParseIntError(std::num::ParseIntError),
    VerbosityError(String),
    Help,
}

impl std::convert::From<std::num::ParseIntError> for ParseError {
    fn from(error: std::num::ParseIntError) -> Self { ParseError::ParseIntError(error) }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::MissingValue(keyword) => write!(f, "missing value for {}", keyword),
            ParseError::UnknownKeyword(keyword) => write!(f, "unknown keyword {}", keyword),
            ParseError::ParseIntError(err) => write!(f, "{}", err),
            ParseError::VerbosityError(keyword) => write!(f, "unknown verbosity keywork {}", keyword),
            ParseError::Help => write!(
                f,
                "cargo run -- keywords drivers\n\
                keywords are:\n\
                 --capacity, -c   : message queue capacity, 0 is unbounded\n\
                 --pipelines, -p  : count of forwarding elements in a lane\n\
                 --lanes, -l      : count of lanes\n\
                 --messages, -m   : count of messages per lane\n\
                 --threads, -t    : count of threads async executor will use\n\
                 --verbosity, -v  : verbosity level for logging: none, all, notify\n\
                 --iterations, -i : count of iterations to run\n\
                drivers are: smol, fast_channel, async_channel, flume_tokio, flume_async_executor"
            ),
        }
    }
}

impl std::str::FromStr for Verbosity {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Verbosity::None),
            "all" => Ok(Verbosity::All),
            "notify" => Ok(Verbosity::Notify),
            _ => Err(ParseError::VerbosityError(s.to_string())),
        }
    }
}

pub fn get_config() -> Result<ExperimentDrivers, ParseError> {
    let mut drivers = ExperimentDrivers::default();
    let mut config = drivers.config;
    let args: Vec<String> = env::args().collect();
    let mut idx = 1;
    while idx < args.len() {
        if args[idx] == "--capacity" || args[idx] == "-c" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.capacity = args[idx + 1].parse::<usize>()?;
            idx += 2;
        } else if args[idx] == "--pipelines" || args[idx] == "-p" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.pipelines = args[idx + 1].parse::<usize>()?;
            idx += 2;
        } else if args[idx] == "--lanes" || args[idx] == "-l" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.lanes = args[idx + 1].parse::<usize>()?;
            idx += 2;
        } else if args[idx] == "--messages" || args[idx] == "-m" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.messages = args[idx + 1].parse::<usize>()?;
            idx += 2;
        } else if args[idx] == "--iterations" || args[idx] == "-i" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.iterations = args[idx + 1].parse::<usize>()?;
            idx += 2;
        } else if args[idx] == "--threads" || args[idx] == "-t" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.threads = args[idx + 1].parse::<usize>()?;
            idx += 2;
        } else if args[idx] == "--verbosity" || args[idx] == "-v" {
            if idx + 1 >= args.len() {
                return Err(ParseError::MissingValue(args[idx].clone()));
            }
            config.verbosity = args[idx + 1].parse::<Verbosity>()?;
            idx += 2;
        } else if args[idx] == "--help" || args[idx] == "-h" {
            return Err(ParseError::Help);
        } else if args[idx] == "smol" {
            drivers
                .drivers
                .push(Box::new(smol_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>);
            idx += 1;
        } else if args[idx] == "async_channel" {
            drivers
                .drivers
                .push(Box::new(async_channel_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>);
            idx += 1;
        } else if args[idx] == "forwarder" {
            drivers
                .drivers
                .push(Box::new(forwarder_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>);
            idx += 1;
        } else if args[idx] == "flume_tokio" {
            drivers
                .drivers
                .push(Box::new(flume_tokio_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>);
            idx += 1;
        } else if args[idx] == "flume_async_executor" {
            drivers
                .drivers
                .push(Box::new(flume_async_executor_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>);
            idx += 1;
        } else if args[idx] == "tokio" {
            drivers
                .drivers
                .push(Box::new(tokio_driver::ServerSimulator::default()) as Box<dyn ExperimentDriver>);
            idx += 1;
        } else {
            return Err(ParseError::UnknownKeyword(args[idx].clone()));
        }
    }
    drivers.config = config;
    Ok(drivers)
}
