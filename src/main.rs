use std::process::ExitCode;

use grpy::config::{self, Paths};

#[tokio::main]
async fn main() -> ExitCode {
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

    println!("grpy");
    ExitCode::SUCCESS
}
