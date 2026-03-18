mod character_selection;
mod game_context;
mod handler;
mod ticket;

use clap::Parser;
use dofus_common::config::WorldConfig;
use dofus_database;
use dofus_network::server;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "dofus-world", about = "Dofus 2.x World Server")]
struct Cli {
    #[arg(short, long, default_value = "config/world.toml")]
    config: PathBuf,
}

pub struct WorldState {
    pub config: WorldConfig,
    pub pool: sqlx::PgPool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,dofus_world=debug,dofus_network=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = WorldConfig::load(&cli.config)?;

    tracing::info!("Starting world server on {}:{}", config.host, config.port);

    let pool = dofus_database::create_pool(&config.database_url).await?;
    dofus_database::run_migrations(&pool).await?;
    tracing::info!("Database ready");

    let state = Arc::new(WorldState {
        config: config.clone(),
        pool,
    });

    let addr = format!("{}:{}", config.host, config.port);
    server::run_server(&addr, move |session| {
        let state = Arc::clone(&state);
        async move { handler::handle_client(session, state).await }
    })
    .await
}
