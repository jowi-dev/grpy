use std::process::{ExitCode, Stdio};
use std::time::Duration;

use clap::Parser;
use grpy::auth::google::HttpTokenEndpoint;
use grpy::auth::{self, FallbackStore, FileStore, KeyringStore, TOKEN_FILE_NAME};
use grpy::cli::{AuthService, Cli, Command};
use grpy::config::{self, Config, Paths};
use grpy::location::{LocationQuery, LocationResolver, Nominatim};
use grpy::provider::FakeProvider;
use grpy::store::Store;
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

    if let Some(Command::Auth {
        service: AuthService::Google,
    }) = cli.command
    {
        return auth_google(&config, &paths);
    }

    for warning in config.warnings() {
        eprintln!("grpy: warning: {warning}");
    }
    let store_file = paths.store_file();
    let _store = match Store::open(&store_file) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("grpy: {}: {err}", store_file.display());
            return ExitCode::FAILURE;
        }
    };

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

/// `grpy auth google`: signs in through the browser and saves the refresh
/// token to the OS keyring, or a private file in the data dir without one.
fn auth_google(config: &Config, paths: &Paths) -> ExitCode {
    let Some(client) = &config.google else {
        eprintln!(
            "grpy: no Google OAuth client configured; set `google.client_id` and \
             `google.client_secret` in {} (the README explains how to create one)",
            paths.config_file.display()
        );
        return ExitCode::FAILURE;
    };
    let store = FallbackStore::new(
        KeyringStore,
        FileStore::new(paths.data_dir.join(TOKEN_FILE_NAME)),
    );

    let signed_in = auth::sign_in(client, &HttpTokenEndpoint::new(), &store, |url| {
        println!("Sign in to Google in your browser. If it doesn't open, visit:\n\n  {url}\n");
        open_in_browser(url.as_str());
        println!("Waiting for Google to redirect back to grpy...");
    });
    match signed_in {
        Ok(location) => {
            println!("Signed in to Google Calendar. Refresh token saved to {location}.");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("grpy: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Best effort: the URL is printed too, so failure here is fine.
fn open_in_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
