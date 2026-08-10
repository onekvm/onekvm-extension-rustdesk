mod config;
mod framing;
mod identity;
mod input;
mod onekvm;
mod protocol;
mod rendezvous;
mod server;

use std::{env, path::PathBuf, process::ExitCode};

use config::Config;

const DEFAULT_CONFIG_PATH: &str = "/run/onekvm/extensions/rustdesk/config.json";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("onekvm-protocol-rustdesk: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut health = false;
    let mut lifecycle = None;
    let mut config_path = env::var_os("ONEKVM_EXTENSION_CONFIG")
        .map(PathBuf::from)
        .map(|path| path.join("config.json"))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--health" => health = true,
            "--config" => {
                config_path = arguments.next().ok_or("--config requires a path")?.into();
            }
            "--lifecycle" => {
                lifecycle = Some(arguments.next().ok_or("--lifecycle requires an action")?);
            }
            _ => return Err(format!("unexpected argument {argument:?}").into()),
        }
    }

    if let Some(action) = lifecycle {
        if action != "enable" && action != "disable" {
            return Err(format!("unsupported lifecycle action {action:?}").into());
        }
        return Ok(());
    }

    if health {
        return Ok(());
    }

    let mut config = Config::load(&config_path)?;
    config.apply_server_mode();
    config.validate()?;
    if !config.service_enabled {
        return Ok(());
    }

    let onekvm_identity = onekvm::Identity::load()?;
    let rustdesk_identity = identity::RustDeskIdentity::load()?;
    rustdesk_identity.publish_settings_output()?;
    server::run(config, onekvm_identity, rustdesk_identity).await?;
    Ok(())
}
