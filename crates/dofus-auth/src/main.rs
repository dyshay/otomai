mod handler;

use clap::Parser;
use dofus_common::config::AuthConfig;
use dofus_database;
use dofus_network::server;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "dofus-auth", about = "Dofus 2.x Auth Server")]
struct Cli {
    #[arg(short, long, default_value = "config/auth.toml")]
    config: PathBuf,
}

struct AuthState {
    config: AuthConfig,
    pool: sqlx::SqlitePool,
    rsa_private_key: rsa::RsaPrivateKey,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,dofus_auth=debug,dofus_network=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = AuthConfig::load(&cli.config)?;

    tracing::info!("Starting auth server on {}:{}", config.host, config.port);

    // Load RSA private key
    let pem = std::fs::read_to_string(&config.rsa_private_key_path)?;
    let private_key = {
        use pkcs1::DecodeRsaPrivateKey;
        rsa::RsaPrivateKey::from_pkcs1_pem(&pem).or_else(|_| {
            use pkcs8::DecodePrivateKey;
            rsa::RsaPrivateKey::from_pkcs8_pem(&pem)
        })?
    };
    tracing::info!("RSA private key loaded");

    // Setup database
    let pool = dofus_database::create_pool(&config.database_url).await?;
    dofus_database::run_migrations(&pool).await?;

    // Ensure at least one server exists
    dofus_database::repository::insert_server(&pool, 1, "Jiva", "127.0.0.1", 5556).await?;
    tracing::info!("Database ready");

    let state = Arc::new(AuthState {
        config: config.clone(),
        pool,
        rsa_private_key: private_key,
    });

    let addr = format!("{}:{}", config.host, config.port);
    server::run_server(&addr, move |session| {
        let state = Arc::clone(&state);
        async move { handler::handle_client(session, state).await }
    })
    .await
}
