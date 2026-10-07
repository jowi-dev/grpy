use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use grpy::cli::Cli;
use grpy::config::{self, Paths};
use grpy::location::{LocationQuery, LocationResolver, Nominatim};
use grpy::provider::FakeProvider;
use grpy::tui;

/// Simulated provider delay, so the loading state is visible.
const DEMO_LATENCY: Duration = Duration::from_millis(800);

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

    let home = LocationQuery::from(&config.home.place);

    let resolved = match LocationResolver::new(Nominatim::new())
        .resolve(cli.location().as_ref(), Some(&home))
    {
        Ok(resolved) => resolved,
        Err(err) => {
            eprintln!("grpy: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("Using location: {resolved}");

    // No real providers exist yet, so browse made-up venues nearby.
    let provider = FakeProvider::demo(&resolved.location).with_latency(DEMO_LATENCY);
    match tui::run(provider, resolved.location, config.home.radius_km()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("grpy: {err}");
            ExitCode::FAILURE
        }
    }
}
