use std::process::ExitCode;

use clap::Parser;
use grpy::cli::Cli;
use grpy::location::{LocationResolver, Nominatim};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // The config file's home location plugs in here once it exists (#3).
    let home = None;

    match LocationResolver::new(Nominatim::new()).resolve(cli.location().as_ref(), home) {
        Ok(resolved) => {
            println!("Using location: {resolved}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("grpy: {err}");
            ExitCode::FAILURE
        }
    }
}
