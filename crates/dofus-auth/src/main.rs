mod handler;

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
    pub rsa_private_key: rsa::RsaPrivateKey,
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

    let addr = format!("{}:{}", config.host, config.port);
    server::run_server(&addr, move |session| {
        let state = Arc::clone(&state);
        async move { handler::handle_client(session, state).await }
    })
    .await
}
