use std::process::ExitCode;

use clap::Parser;
use grpy::cli::Cli;
use grpy::config::{self, Paths};
use grpy::location::{LocationResolver, Nominatim};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let env = |name: &str| std::env::var(name).ok();

    let Some(paths) = Paths::from_env(env) else {
        eprintln!("grpy: can't find a config directory; set HOME or XDG_CONFIG_HOME");
        return ExitCode::FAILURE;
    };
    let config = match config::load_or_init(&paths.config_file, env) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("grpy: {err}");
            return ExitCode::FAILURE;
        }
    };
    for warning in config.warnings() {
        eprintln!("grpy: warning: {warning}");
    }

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
