mod character_selection;
mod game_context;
mod handler;
mod inventory;
pub mod map_cache;
mod movement;
mod spells;
mod stats;
mod ticket;
pub mod world;

use clap::Parser;
use dofus_common::config::WorldConfig;
use dofus_database;
use dofus_network::server;
use std::path::{Path, PathBuf};
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
    pub world: world::World,
    pub maps: map_cache::MapCache,
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

    // Load map data from D2P archives
    let maps = if let Some(ref maps_dir) = config.maps_dir {
        tracing::info!("Loading maps from {}", maps_dir);
        map_cache::MapCache::load_from_dir(Path::new(maps_dir))?
    } else {
        tracing::warn!("No maps_dir configured — map transitions will be disabled");
        map_cache::MapCache::empty()
    };
    tracing::info!("Map cache: {} raw maps loaded", maps.raw_count());

    let state = Arc::new(WorldState {
        config: config.clone(),
        pool,
        world: world::World::new(),
        maps,
    });

    let addr = format!("{}:{}", config.host, config.port);
    server::run_server(&addr, move |session| {
        let state = Arc::clone(&state);
        async move { handler::handle_client(session, state).await }
    })
    .await
}
