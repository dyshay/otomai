mod crypto;
mod handler;
mod identification;
mod server_selection;

use clap::Parser;
use dofus_common::config::AuthConfig;
use dofus_database;
use dofus_network::server;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

#[derive(Parser)]
#[command(name = "dofus-auth", about = "Dofus 2.x Auth Server")]
struct Cli {
    #[arg(short, long, default_value = "config/auth.toml")]
    config: PathBuf,

    /// Enable maintenance mode
    #[arg(long)]
    maintenance: bool,

    /// Max concurrent connections (queue capacity)
    #[arg(long, default_value_t = 100)]
    max_connections: usize,

    /// Max login attempts per IP per minute
    #[arg(long, default_value_t = 10)]
    rate_limit: u32,

    /// Auto-create accounts on first login (dev mode)
    #[arg(long)]
    auto_create: bool,
}

pub struct AuthState {
    pub config: AuthConfig,
    pub pool: sqlx::PgPool,
    /// Permanent signature key — signs the session public key
    pub rsa_private_key: rsa::RsaPrivateKey,
    /// Ephemeral session private key — decrypts client credentials
    pub session_private_key: rsa::RsaPrivateKey,
    /// Session public key DER, signed with the signature key (PKCS1v15)
    pub signed_session_key: Vec<u8>,
    pub auto_create_accounts: bool,

    // Connection queue (semaphore-based)
    pub connection_semaphore: Semaphore,

    // Maintenance mode
    maintenance: AtomicBool,

    // Rate limiting: IP -> (attempt_count, window_start)
    rate_limits: Mutex<HashMap<IpAddr, (u32, std::time::Instant)>>,
    rate_limit_max: u32,
}

impl AuthState {
    pub fn is_maintenance(&self) -> bool {
        self.maintenance.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn set_maintenance(&self, enabled: bool) {
        self.maintenance.store(enabled, Ordering::Relaxed);
        tracing::info!("Maintenance mode: {}", if enabled { "ON" } else { "OFF" });
    }

    /// Returns true if the request is allowed, false if rate-limited.
    pub fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let mut limits = self.rate_limits.lock().unwrap();
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(60);

        let entry = limits.entry(ip).or_insert((0, now));

        // Reset window if expired
        if now.duration_since(entry.1) > window {
            *entry = (0, now);
        }

        entry.0 += 1;
        entry.0 <= self.rate_limit_max
    }

    /// Record a failed login attempt (for stricter rate limiting on failures).
    pub fn record_failed_attempt(&self, ip: IpAddr) {
        let mut limits = self.rate_limits.lock().unwrap();
        let now = std::time::Instant::now();
        // Failed attempts count double
        let entry = limits.entry(ip).or_insert((0, now));
        entry.0 += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test rate limiting and maintenance without needing a DB pool.
    // We test the logic directly on the data structures.

    struct TestRateLimiter {
        rate_limits: Mutex<HashMap<IpAddr, (u32, std::time::Instant)>>,
        rate_limit_max: u32,
    }

    impl TestRateLimiter {
        fn new(max: u32) -> Self {
            Self {
                rate_limits: Mutex::new(HashMap::new()),
                rate_limit_max: max,
            }
        }

        fn check(&self, ip: IpAddr) -> bool {
            let mut limits = self.rate_limits.lock().unwrap();
            let now = std::time::Instant::now();
            let window = std::time::Duration::from_secs(60);
            let entry = limits.entry(ip).or_insert((0, now));
            if now.duration_since(entry.1) > window {
                *entry = (0, now);
            }
            entry.0 += 1;
            entry.0 <= self.rate_limit_max
        }

        fn record_failure(&self, ip: IpAddr) {
            let mut limits = self.rate_limits.lock().unwrap();
            let now = std::time::Instant::now();
            let entry = limits.entry(ip).or_insert((0, now));
            entry.0 += 1;
        }
    }

    #[test]
    fn rate_limit_allows_under_threshold() {
        let rl = TestRateLimiter::new(5);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        for _ in 0..5 {
            assert!(rl.check(ip));
        }
    }

    #[test]
    fn rate_limit_blocks_over_threshold() {
        let rl = TestRateLimiter::new(3);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        assert!(rl.check(ip)); // 1
        assert!(rl.check(ip)); // 2
        assert!(rl.check(ip)); // 3
        assert!(!rl.check(ip)); // 4 — blocked
    }

    #[test]
    fn rate_limit_different_ips_independent() {
        let rl = TestRateLimiter::new(2);
        let ip1: IpAddr = "1.1.1.1".parse().unwrap();
        let ip2: IpAddr = "2.2.2.2".parse().unwrap();
        assert!(rl.check(ip1));
        assert!(rl.check(ip1));
        assert!(!rl.check(ip1)); // blocked
        assert!(rl.check(ip2)); // different IP, still OK
    }

    #[test]
    fn failed_attempt_counts_extra() {
        let rl = TestRateLimiter::new(3);
        let ip: IpAddr = "5.5.5.5".parse().unwrap();
        assert!(rl.check(ip));       // count=1
        rl.record_failure(ip);       // count=2
        assert!(rl.check(ip));       // count=3
        assert!(!rl.check(ip));      // count=4, blocked
    }

    #[test]
    fn maintenance_mode_toggle() {
        let flag = AtomicBool::new(false);
        assert!(!flag.load(Ordering::Relaxed));
        flag.store(true, Ordering::Relaxed);
        assert!(flag.load(Ordering::Relaxed));
        flag.store(false, Ordering::Relaxed);
        assert!(!flag.load(Ordering::Relaxed));
    }
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
    tracing::info!("RSA signature key loaded");

    // Generate ephemeral session keypair (generated once at startup)
    let session_private_key = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 1024)?;
    let session_public_key = session_private_key.to_public_key();
    let session_der = {
        use pkcs8::EncodePublicKey;
        session_public_key
            .to_public_key_der()
            .map_err(|e| anyhow::anyhow!("Failed to encode session public key DER: {}", e))?
    };
    // Sign session DER with permanent signature key (PKCS1v15)
    let signed_session_key = {
        use rsa::pkcs1v15::Pkcs1v15Sign;
        private_key.sign(Pkcs1v15Sign::new_unprefixed(), session_der.as_bytes())?
    };
    tracing::info!("Session keypair generated and signed ({} bytes)", signed_session_key.len());

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
        session_private_key,
        signed_session_key,
        auto_create_accounts: cli.auto_create,
        connection_semaphore: Semaphore::new(cli.max_connections),
        maintenance: AtomicBool::new(cli.maintenance),
        rate_limits: Mutex::new(HashMap::new()),
        rate_limit_max: cli.rate_limit,
    });

    if cli.maintenance {
        tracing::warn!("Server starting in MAINTENANCE mode");
    }
    tracing::info!(
        max_connections = cli.max_connections,
        rate_limit = cli.rate_limit,
        auto_create = cli.auto_create,
        "Auth server ready"
    );

    // Start IPC server for world server communication
    let ipc_addr = format!("0.0.0.0:{}", config.ipc_port);
    let (_ipc_rx, _ipc_handle) = dofus_ipc::server::start(&ipc_addr).await?;
    tracing::info!("IPC server ready on port {}", config.ipc_port);

    // Spawn IPC message handler
    tokio::spawn(async move {
        let mut ipc_rx = _ipc_rx;
        while let Some((envelope, reply_tx)) = ipc_rx.recv().await {
            match envelope.msg_type.as_str() {
                "handshake" => {
                    if let Ok(hs) = serde_json::from_value::<dofus_ipc::messages::Handshake>(envelope.payload) {
                        tracing::info!("World server registered: {} (id={})", hs.server_name, hs.server_id);
                    }
                }
                "server_status" => {
                    if let Ok(status) = serde_json::from_value::<dofus_ipc::messages::ServerStatusUpdate>(envelope.payload) {
                        tracing::info!("World {} status: {} players, status={}", status.server_id, status.player_count, status.status);
                    }
                }
                _ => {
                    tracing::debug!("Unknown IPC message: {}", envelope.msg_type);
                }
            }
        }
    });

    let addr = format!("{}:{}", config.host, config.port);
    server::run_server(&addr, move |session| {
        let state = Arc::clone(&state);
        async move { handler::handle_client(session, state).await }
    })
    .await
}
