use moar::config::MoarConfig;
use moar::gateway::start_gateway;
use moar::policy::PolicyEngine;
use moar::storage::lmdb::LmdbStore;
use moar::wot::WotManager;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "moar")]
#[command(about = "Mother Of All Relays", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the relay(s)
    Start {
        /// Path to configuration file
        #[arg(short, long, default_value = "moar.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { config: config_path } => {
            let config_content = std::fs::read_to_string(&config_path)?;
            let config: MoarConfig = toml::from_str(&config_content)?;

            // Create WoT manager and start background builders
            let wot_manager = WotManager::new(
                config.discovery_relays.clone(),
                config.wots.clone(),
            );
            wot_manager.start_all().await;

            let mut processed_relays = std::collections::HashMap::new();

            for (key, relay_conf) in config.relays.clone() {
                let store: Arc<dyn moar::storage::NostrStore> =
                    Arc::new(LmdbStore::new(&relay_conf.db_path)?);
                let write_wot = match &relay_conf.policy.write.wot {
                    Some(id) => wot_manager.get_set(id).await,
                    None => None,
                };
                let read_wot = match &relay_conf.policy.read.wot {
                    Some(id) => wot_manager.get_set(id).await,
                    None => None,
                };
                let policy = Arc::new(PolicyEngine::new(relay_conf.policy.clone(), write_wot, read_wot));
                processed_relays.insert(key, (relay_conf, store, policy));
            }

            start_gateway(
                config.port,
                config.domain.clone(),
                processed_relays,
                config,
                config_path,
                wot_manager,
            )
            .await?;
        }
    }

    Ok(())
}
