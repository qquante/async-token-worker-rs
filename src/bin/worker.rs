//! Four.Meme Token Creator Worker - OPTIMIZED
//!
//! High-performance Rust worker that:
//! 1. Pre-authenticates on startup (saves ~300ms per deploy)
//! 2. Listens to Redis pub/sub channels
//! 3. Processes multiple token creation requests IN PARALLEL
//! 4. Each request runs in its own tokio task (non-blocking)
//! 5. Publishes results to `token:created` channel
//!
//! ## Modes (set via WORKER_TYPE env var)
//!
//! - `WORKER_TYPE=DEPLOYER_EN` - Listens to `deployer:en` (from token-deployer)
//! - `WORKER_TYPE=DEPLOYER_ZH` - Listens to `deployer:zh` (from token-deployer)
//! - `WORKER_TYPE=BOOK_EN` - Listens to `book:en` (from cz-book-watcher)
//! - `WORKER_TYPE=BOOK_ZH` - Listens to `book:zh` (from cz-book-watcher)
//!
//! ## Usage
//!
//! ```bash
//! PRIVATE_KEY=your_key REDIS_URL=redis://localhost:6379 cargo run --bin four-meme-worker --features worker
//! ```
//!
//! ## Environment Variables
//!
//! - `PRIVATE_KEY` - Wallet private key (required)
//! - `REDIS_URL` - Redis connection URL (default: redis://localhost:6379)
//! - `BSC_RPC_URL` - BSC RPC URL (default: https://bsc-dataseed.binance.org)
//! - `FOUR_MEME_GAS_GWEI` - Gas price in gwei (default: 5)
//! - `PRESALE_MIN` - Minimum presale BNB (default: 0.19)
//! - `PRESALE_MAX` - Maximum presale BNB (default: 0.20)
//! - `LABEL` - Token label (default: Meme)
//! - `LANG` - Worker mode: EN, ZH, or VIP (default: EN)
//! - `MAX_CONCURRENT` - Max parallel token creations (default: 5)

use anyhow::Result;
use four_meme::{
    redis_client::{channels, decode_image, download_image, parse_deploy_request, RedisClient, TokenCreatedMessage, TokenLinks},
    FourMeme, TokenConfig, VALID_LABELS,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// Macro para log que vai para stderr (não é buffered no Docker)
macro_rules! log_stderr {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

// ============================================================================
// CONFIGURATION
// ============================================================================

struct Config {
    private_key: String,
    rpc_url: String,
    redis_url: String,
    gas_gwei: u64,
    presale_min: f64,
    presale_max: f64,
    label: String,
    lang: String,
    /// Maximum concurrent token creations
    max_concurrent: usize,
    /// Channels to subscribe
    channels: Vec<String>,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        // WORKER_TYPE determines which channel to listen to
        // Options: DEPLOYER_EN, DEPLOYER_ZH, BOOK_EN, BOOK_ZH
        let worker_type = std::env::var("WORKER_TYPE").unwrap_or_else(|_| "DEPLOYER_EN".to_string());
        let lang = worker_type.clone(); // Keep for display purposes
        
        // Determine channels based on worker type
        let channels = match worker_type.as_str() {
            "DEPLOYER_EN" => vec![channels::DEPLOYER_EN.to_string()],
            "DEPLOYER_ZH" => vec![channels::DEPLOYER_ZH.to_string()],
            "BOOK_EN" => vec![channels::BOOK_EN.to_string()],
            "BOOK_ZH" => vec![channels::BOOK_ZH.to_string()],
            _ => vec![channels::DEPLOYER_EN.to_string()],
        };

        Ok(Self {
            private_key: std::env::var("PRIVATE_KEY")
                .map_err(|_| anyhow::anyhow!("PRIVATE_KEY not set"))?,
            rpc_url: std::env::var("BSC_RPC_URL")
                .unwrap_or_else(|_| "https://bsc-dataseed.binance.org".to_string()),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6888".to_string()),
            gas_gwei: std::env::var("FOUR_MEME_GAS_GWEI")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            presale_min: std::env::var("PRESALE_MIN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.001),
            presale_max: std::env::var("PRESALE_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.002),
            label: std::env::var("LABEL").unwrap_or_else(|_| "Meme".to_string()),
            lang,
            max_concurrent: std::env::var("MAX_CONCURRENT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            channels,
        })
    }

    fn random_presale(&self) -> f64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as f64;
        let random = (seed / 1_000_000_000.0) * (self.presale_max - self.presale_min) + self.presale_min;
        (random * 10000.0).round() / 10000.0
    }
}

// ============================================================================
// WORKER
// ============================================================================

struct Worker {
    config: Arc<Config>,
    four_meme: Arc<FourMeme>,
    redis: RedisClient,
    /// Semaphore to limit concurrent token creations
    semaphore: Arc<Semaphore>,
}

impl Worker {
    async fn new(config: Config) -> Result<Self> {
        log_stderr!("[WORKER] Creating FourMeme client...");
        let four_meme: FourMeme = match FourMeme::new(&config.private_key, &config.rpc_url) {
            Ok(fm) => {
                log_stderr!("[WORKER] FourMeme client created successfully");
                fm
            }
            Err(e) => {
                log_stderr!("[WORKER] ERROR creating FourMeme client: {}", e);
                return Err(anyhow::anyhow!("FourMeme error: {}", e));
            }
        };
        
        // =========================================================================
        // PRE-AUTH ON STARTUP (saves ~300ms on first deploy)
        // =========================================================================
        log_stderr!("[WORKER] Pre-authenticating wallet...");
        info!("[WORKER] Pre-authenticating wallet...");
        match four_meme.pre_auth().await {
            Ok(_) => {
                log_stderr!("[WORKER] Pre-auth successful!");
                info!("[WORKER] Pre-auth successful!");
            }
            Err(e) => {
                log_stderr!("[WORKER] Pre-auth failed: {}", e);
                warn!("[WORKER] Pre-auth failed (will auth on first deploy): {}", e);
            }
        }
        
        // =========================================================================
        // PRE-FETCH NONCE ON STARTUP (saves ~100ms on first deploy)
        // =========================================================================
        log_stderr!("[WORKER] Initializing nonce from blockchain...");
        info!("[WORKER] Initializing nonce from blockchain...");
        match four_meme.init_nonce().await {
            Ok(nonce) => {
                log_stderr!("[WORKER] Nonce initialized: {}", nonce);
                info!("[WORKER] ✅ Nonce initialized: {} (0ms on deploy!)", nonce);
            }
            Err(e) => {
                log_stderr!("[WORKER] Nonce init failed: {}", e);
                warn!("[WORKER] Nonce init failed: {} (will fetch on first deploy)", e);
            }
        }

        log_stderr!("[WORKER] Creating semaphore with max_concurrent={}", config.max_concurrent);
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
        
        log_stderr!("[WORKER] Creating Redis client for {}", config.redis_url);
        let redis = RedisClient::new(&config.redis_url);

        log_stderr!("[WORKER] Worker::new() completed");
        Ok(Self {
            config: Arc::new(config),
            four_meme: Arc::new(four_meme),
            redis,
            semaphore,
        })
    }

    async fn run(&mut self) -> Result<()> {
        log_stderr!("[WORKER] Worker::run() starting...");
        self.print_banner();

        // =========================================================================
        // BACKGROUND AUTH REFRESH (renews cache every 4min50s)
        // =========================================================================
        // This ensures auth cache is ALWAYS valid (0ms auth overhead)
        let four_meme_auth = Arc::clone(&self.four_meme);
        tokio::spawn(async move {
            loop {
                // Sleep 4min50s (10s before 5min cache expiry)
                tokio::time::sleep(Duration::from_secs(290)).await;
                
                info!("[WORKER] 🔄 Background auth refresh...");
                match four_meme_auth.authenticate_fresh().await {
                    Ok(_) => {
                        info!("[WORKER] ✅ Auth cache renewed (valid for 5min)");
                    }
                    Err(e) => {
                        warn!("[WORKER] ⚠️  Auth refresh failed: {} (will retry in 4min50s)", e);
                    }
                }
            }
        });
        info!("[WORKER] Background auth refresh task started (every 4min50s)");
        
        // =========================================================================
        // BACKGROUND NONCE REFRESH (syncs every 8 seconds)
        // =========================================================================
        // Uses max(local, chain) - catches external TXs (sells, etc) quickly
        let four_meme_nonce = Arc::clone(&self.four_meme);
        tokio::spawn(async move {
            loop {
                // Sleep 8 seconds
                tokio::time::sleep(Duration::from_secs(8)).await;
                
                // Silent refresh - only log if nonce changed
                match four_meme_nonce.refresh_nonce().await {
                    Ok(_nonce) => {
                        // Nonce refresh is silent - blockchain.rs logs if changed
                    }
                    Err(e) => {
                        warn!("[WORKER] ⚠️  Nonce refresh failed: {}", e);
                    }
                }
            }
        });
        info!("[WORKER] Background nonce refresh task started (every 8s)");

        // RECONNECTION LOOP - keeps trying if Redis connection drops
        loop {
            log_stderr!("[WORKER] Preparing to subscribe to channels...");
            let channels: Vec<&str> = self.config.channels.iter().map(|s| s.as_str()).collect();
            log_stderr!("[WORKER] Channels to subscribe: {:?}", channels);
            
            log_stderr!("[WORKER] Calling redis.subscribe()...");
            let mut rx = match self.redis.subscribe(channels).await {
                Ok(r) => {
                    log_stderr!("[WORKER] Redis subscribe successful!");
                    r
                }
                Err(e) => {
                    log_stderr!("[WORKER] ERROR in redis.subscribe(): {}", e);
                    error!("[WORKER] Redis subscribe failed: {}", e);
                    log_stderr!("[WORKER] Retrying in 5 seconds...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            log_stderr!("[WORKER] Entering message loop...");
            info!("[WORKER] Started, waiting for messages...\n");

            // Process messages
            while let Some((channel, payload)) = rx.recv().await {
                log_stderr!("[WORKER] Received message on channel: {}", channel);
                info!("[WORKER] Received message on {}", channel);
                
                // Clone what we need for the spawned task
                let config = Arc::clone(&self.config);
                let four_meme = Arc::clone(&self.four_meme);
                let semaphore = Arc::clone(&self.semaphore);
                let mut redis_pub = RedisClient::new(&self.config.redis_url);
                
                // Spawn task for PARALLEL processing
                tokio::spawn(async move {
                    // Acquire semaphore permit (limits concurrency)
                    let _permit = match semaphore.acquire().await {
                        Ok(p) => p,
                        Err(_) => {
                            error!("[WORKER] Semaphore closed");
                            return;
                        }
                    };

                    let start = Instant::now();
                    
                    // Process the request
                    match process_request(&config, &four_meme, &channel, &payload, &mut redis_pub).await {
                        Ok(token_address) => {
                            log_stderr!("[WORKER] Token created: {}", token_address);
                            info!(
                                "[{}] Token created in {:?}: {}",
                                config.lang,
                                start.elapsed(),
                                token_address
                            );
                        }
                        Err(e) => {
                            log_stderr!("[WORKER] Token creation failed: {}", e);
                            error!("[{}] Failed: {}", config.lang, e);
                        }
                    }
                });
            }

            // If we get here, the Redis connection was lost
            log_stderr!("[WORKER] Redis connection lost! Reconnecting in 5 seconds...");
            warn!("[WORKER] Redis connection lost, reconnecting...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    fn print_banner(&self) {
        let flag = match self.config.lang.as_str() {
            "ZH" => "🇨🇳",
            "VIP" => "⭐",
            _ => "🇺🇸",
        };
        
        println!(r#"
╔═══════════════════════════════════════════════════════════════════════════════╗
║  {} FOUR.MEME RUST WORKER - {} (OPTIMIZED)                                     
╠═══════════════════════════════════════════════════════════════════════════════╣
║  TIME: {}
║  WALLET: {}...
╠═══════════════════════════════════════════════════════════════════════════════╣
║  RPC: {}
║  REDIS: {}
║  GAS: {} gwei (~2-3s confirmation)
║  PRESALE: {} - {} BNB
║  LABEL: {}
║  MAX CONCURRENT: {}
╠═══════════════════════════════════════════════════════════════════════════════╣
║  OPTIMIZATIONS:
║    ✅ Pre-auth on startup (saves ~300ms)
║    ✅ Auth cache (5 min TTL, background refresh)
║    ✅ Nonce cache (0ms lookup, background refresh)
║    ✅ No balance check
║    ✅ High gas price for fast confirmation
║    ✅ Parallel processing with semaphore
╠═══════════════════════════════════════════════════════════════════════════════╣
║  CHANNELS:
{}
╚═══════════════════════════════════════════════════════════════════════════════╝
"#,
            flag,
            self.config.lang,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            &self.config.private_key[..8],
            self.config.rpc_url,
            self.config.redis_url,
            self.config.gas_gwei,
            self.config.presale_min,
            self.config.presale_max,
            self.config.label,
            self.config.max_concurrent,
            self.config.channels.iter()
                .map(|c| format!("║    - {}", c))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

// ============================================================================
// REQUEST PROCESSING
// ============================================================================

async fn process_request(
    config: &Config,
    four_meme: &FourMeme,
    _channel: &str,
    payload: &str,
    redis: &mut RedisClient,
) -> Result<String> {
    // Parse request
    let request = parse_deploy_request(payload)?;
    
    info!(
        "[{}] Processing: ${} - {}",
        config.lang, request.ticker, request.name
    );

    // Get image - Priority: four_meme_image_url > image_url > image_base64
    let (image, pre_uploaded_url) = if let Some(fm_url) = &request.four_meme_image_url {
        // FASTEST PATH: Pre-uploaded to Four.Meme, skip upload entirely
        log_stderr!("[WORKER] Using pre-uploaded Four.Meme URL (skip upload)");
        (Vec::new(), Some(fm_url.clone()))
    } else if let Some(url) = &request.image_url {
        // Download from external URL (Twitter, etc)
        log_stderr!("[WORKER] Downloading image from URL: {}", url);
        (download_image(url).await?, None)
    } else if let Some(base64) = &request.image_base64 {
        // Decode from base64
        log_stderr!("[WORKER] Decoding image from base64");
        (decode_image(base64)?, None)
    } else {
        return Err(anyhow::anyhow!("No image provided (need fourMemeImageUrl, imageUrl or imageBase64)"));
    };

    // Validate label
    let label = if VALID_LABELS.contains(&config.label.as_str()) {
        config.label.clone()
    } else {
        "Meme".to_string()
    };

    // Create token config
    let token_config = TokenConfig {
        ticker: request.ticker.clone(),
        name: request.name.clone(),
        description: request.description.clone(),
        image,
        pre_uploaded_image_url: pre_uploaded_url,
        label,
        presale_bnb: config.random_presale(),
        twitter_url: request.tweet_url.clone(),
        telegram_url: None,
        website_url: None,
    };

    // Create token!
    let result = four_meme.create_token(token_config).await?;

    // Publish result to Redis
    let created_msg = TokenCreatedMessage {
        lang: config.lang.clone(),
        ticker: request.ticker,
        name: request.name,
        token_address: result.token_address.clone(),
        tx_hash: result.tx_hash.clone(),
        links: TokenLinks {
            bscscan: result.links.bscscan,
            four_meme: result.links.four_meme,
            pancakeswap: result.links.pancakeswap,
            tx: result.links.tx,
        },
        snipe: None,
        tweet_url: request.tweet_url,
    };

    if let Err(e) = redis.connect().await {
        warn!("[WORKER] Failed to connect Redis for publish: {}", e);
    } else if let Err(e) = redis.publish(channels::TOKEN_CREATED, &created_msg).await {
        warn!("[WORKER] Failed to publish token:created: {}", e);
    }

    Ok(result.token_address)
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    log_stderr!("[WORKER] ====== STARTING RUST TOKEN CREATOR ======");
    log_stderr!("[WORKER] Initializing tracing...");
    
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    log_stderr!("[WORKER] Tracing initialized");
    
    // Load config
    log_stderr!("[WORKER] Loading config from env...");
    let config = match Config::from_env() {
        Ok(c) => {
            log_stderr!("[WORKER] Config loaded successfully");
            log_stderr!("[WORKER]   PRIVATE_KEY: {}...", &c.private_key[..8.min(c.private_key.len())]);
            log_stderr!("[WORKER]   REDIS_URL: {}", c.redis_url);
            log_stderr!("[WORKER]   RPC_URL: {}", c.rpc_url);
            log_stderr!("[WORKER]   LANG: {}", c.lang);
            log_stderr!("[WORKER]   CHANNELS: {:?}", c.channels);
            c
        }
        Err(e) => {
            log_stderr!("[WORKER] ERROR loading config: {}", e);
            return Err(e);
        }
    };

    // Create and run worker (with pre-auth)
    log_stderr!("[WORKER] Creating worker...");
    let mut worker = match Worker::new(config).await {
        Ok(w) => {
            log_stderr!("[WORKER] Worker created successfully");
            w
        }
        Err(e) => {
            log_stderr!("[WORKER] ERROR creating worker: {}", e);
            return Err(e);
        }
    };
    
    log_stderr!("[WORKER] Starting worker run loop...");
    
    // Handle shutdown
    tokio::select! {
        result = worker.run() => {
            if let Err(e) = result {
                log_stderr!("[WORKER] ERROR in worker.run(): {}", e);
                error!("[WORKER] Error: {}", e);
            } else {
                log_stderr!("[WORKER] worker.run() completed without error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            log_stderr!("[WORKER] Received Ctrl+C");
            info!("[WORKER] Shutting down...");
        }
    }

    log_stderr!("[WORKER] Main function ending");
    Ok(())
}
