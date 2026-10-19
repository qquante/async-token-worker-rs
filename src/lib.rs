//! # Four.Meme Token Creator - OPTIMIZED
//!
//! High-performance Rust implementation for creating tokens on Four.Meme (BSC).
//!
//! ## Optimizations
//! - Auth cache (saves ~300-500ms per deploy)
//! - No balance check (unnecessary overhead)
//! - Configurable gas price for faster confirmations
//! - Pre-auth support
//!
//! ## Usage
//!
//! ```rust,no_run
//! use four_meme::{FourMeme, TokenConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut client = FourMeme::new(
//!         "your_private_key",
//!         "https://bsc-dataseed.binance.org",
//!     )?;
//!
//!     // Pre-authenticate on startup (optional but recommended)
//!     client.pre_auth().await?;
//!
//!     let config = TokenConfig {
//!         ticker: "TEST".to_string(),
//!         name: "Test Token".to_string(),
//!         description: Some("A test token".to_string()),
//!         image: std::fs::read("logo.png")?,
//!         label: "Meme".to_string(),
//!         presale_bnb: 0.1,
//!         twitter_url: None,
//!         telegram_url: None,
//!         website_url: None,
//!     };
//!
//!     let result = client.create_token(config).await?;
//!     println!("Token created: {}", result.token_address);
//!
//!     Ok(())
//! }
//! ```

pub mod api;
pub mod blockchain;
pub mod broadcaster;
pub mod error;
pub mod redis_client;
pub mod types;

pub use api::FourMemeApi;
pub use blockchain::Blockchain;
pub use broadcaster::{FireResult, RpcEndpoint, RpcMethod, TxBroadcaster, BLOXROUTE_AUTH, ANKR_API_KEY, CHAINSTACK_API_KEY};
pub use error::FourMemeError;
pub use redis_client::RedisClient;
pub use types::*;

use ethers::prelude::*;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::info;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Four.Meme API base URL
pub const API_BASE: &str = "https://four.meme/meme-api";

/// TokenManager contract address on BSC
pub const TOKEN_MANAGER_ADDRESS: &str = "0x5c952063c7fc8610FFDB798152D69F0B9550762b";

/// Token creation fee in BNB
pub const CREATION_FEE: f64 = 0.01;

/// Buy fee percentage (1%)
pub const BUY_FEE_PERCENT: u64 = 1;

/// Default gas price in gwei (5 gwei = ~2-3s confirmation on BSC)
pub const DEFAULT_GAS_GWEI: u64 = 5;

/// Valid token labels
pub const VALID_LABELS: &[&str] = &[
    "Meme", "AI", "Defi", "Games", "Infra", "De-Sci", "Social", "Depin", "Charity", "Others",
];

// ============================================================================
// AUTH CACHE (saves ~300-500ms per deploy)
// ============================================================================

/// Cached auth entry
#[derive(Clone)]
struct AuthCacheEntry {
    access_token: String,
    expires_at: Instant,
}

// Global auth cache
lazy_static::lazy_static! {
    static ref AUTH_CACHE: RwLock<HashMap<String, AuthCacheEntry>> = RwLock::new(HashMap::new());
}

const AUTH_CACHE_TTL: Duration = Duration::from_secs(5 * 60); // 5 minutes

fn get_cached_auth(address: &str) -> Option<String> {
    let cache = AUTH_CACHE.read().ok()?;
    let entry = cache.get(&address.to_lowercase())?;
    
    if Instant::now() < entry.expires_at {
        info!("[FOUR.MEME] ⚡ Using cached auth");
        Some(entry.access_token.clone())
    } else {
        None
    }
}

fn set_cached_auth(address: &str, access_token: &str) {
    if let Ok(mut cache) = AUTH_CACHE.write() {
        cache.insert(
            address.to_lowercase(),
            AuthCacheEntry {
                access_token: access_token.to_string(),
                expires_at: Instant::now() + AUTH_CACHE_TTL,
            },
        );
    }
}

// ============================================================================
// MAIN CLIENT
// ============================================================================

/// Main Four.Meme client - OPTIMIZED
pub struct FourMeme {
    api: FourMemeApi,
    blockchain: Blockchain,
    wallet_address: Address,
    /// Gas price in gwei (default: 5)
    gas_gwei: u64,
}

impl FourMeme {
    /// Create a new Four.Meme client
    ///
    /// # Arguments
    /// * `private_key` - Wallet private key (with or without 0x prefix)
    /// * `rpc_url` - BSC RPC URL
    ///
    /// # Returns
    /// * `Result<FourMeme>` - Client instance or error
    pub fn new(private_key: &str, rpc_url: &str) -> Result<Self, FourMemeError> {
        let gas_gwei = std::env::var("FOUR_MEME_GAS_GWEI")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_GAS_GWEI);

        let blockchain = Blockchain::new(private_key, rpc_url, gas_gwei)?;
        let wallet_address = blockchain.wallet_address();
        
        // Create API with Redis URL for Cloudflare cookie (if available)
        let api = match std::env::var("REDIS_URL") {
            Ok(redis_url) => {
                info!("[FOUR.MEME] Using Redis for Cloudflare cookie: {}", 
                    redis_url.split('@').last().unwrap_or(&redis_url));
                FourMemeApi::with_redis(&redis_url)
            }
            Err(_) => {
                info!("[FOUR.MEME] No REDIS_URL - Cloudflare bypass disabled");
                FourMemeApi::new()
            }
        };

        info!(
            "[FOUR.MEME] Client initialized: wallet={:?}, gas={}gwei",
            wallet_address, gas_gwei
        );

        Ok(Self {
            api,
            blockchain,
            wallet_address,
            gas_gwei,
        })
    }

    /// Create with custom gas price
    pub fn with_gas_gwei(mut self, gwei: u64) -> Self {
        self.gas_gwei = gwei;
        self
    }

    /// Get the wallet address
    pub fn wallet_address(&self) -> Address {
        self.wallet_address
    }

    /// Pre-authenticate wallet (call on startup to cache auth)
    /// This saves ~300-500ms on the first token creation
    pub async fn pre_auth(&self) -> Result<String, FourMemeError> {
        info!("[FOUR.MEME] Pre-authenticating wallet...");
        let start = Instant::now();
        
        let access_token = self.authenticate_fresh().await?;
        
        info!("[FOUR.MEME] Pre-auth complete in {:?}", start.elapsed());
        Ok(access_token)
    }
    
    /// Initialize nonce from blockchain (call on startup)
    /// This ensures 0ms nonce lookup on first deploy
    pub async fn init_nonce(&self) -> Result<U256, FourMemeError> {
        info!("[FOUR.MEME] Initializing nonce from blockchain...");
        self.blockchain.init_nonce().await
    }
    
    /// Refresh nonce using max(local, chain)
    /// Safe for background task - won't break pending TXs
    pub async fn refresh_nonce(&self) -> Result<U256, FourMemeError> {
        self.blockchain.refresh_nonce().await
    }

    /// Create a new token on Four.Meme - OPTIMIZED
    ///
    /// Flow:
    /// 1. Get auth (cached or fresh) - ~0ms if cached, ~300ms if fresh
    /// 2. Upload image - ~100-300ms
    /// 3. Create token API - ~50-100ms
    /// 4. Send TX with high gas - ~100-300ms
    /// 5. Wait confirmation - ~2-5s with high gas
    ///
    /// # Arguments
    /// * `config` - Token configuration
    ///
    /// # Returns
    /// * `Result<TokenResult>` - Creation result with token address and tx hash
    pub async fn create_token(&self, config: TokenConfig) -> Result<TokenResult, FourMemeError> {
        let start_time = Instant::now();
        let mut benchmarks = Benchmarks::default();

        // Validate config
        self.validate_config(&config)?;

        info!(
            "[FOUR.MEME] Creating: ${} - {} (presale: {} BNB, gas: {}gwei)",
            config.ticker, config.name, config.presale_bnb, self.gas_gwei
        );

        // NO BALANCE CHECK - If insufficient, TX will fail anyway
        // This saves ~100-150ms per deploy

        // Step 1: Authenticate (CACHED)
        let auth_start = Instant::now();
        let access_token = self.authenticate().await?;
        benchmarks.auth_ms = auth_start.elapsed().as_millis() as u64;

        // Step 2: Get image URL (use pre-uploaded or upload now)
        let upload_start = Instant::now();
        let image_url = if let Some(pre_url) = &config.pre_uploaded_image_url {
            // Use pre-uploaded URL (FASTEST PATH - skip upload)
            info!("[FOUR.MEME] Using pre-uploaded image URL (0ms)");
            benchmarks.upload_ms = 0;
            pre_url.clone()
        } else {
            // Upload image now
            let url = self.api.upload_image(&access_token, &config.image).await?;
            benchmarks.upload_ms = upload_start.elapsed().as_millis() as u64;
            info!("[FOUR.MEME] Image uploaded in {}ms", benchmarks.upload_ms);
            url
        };

        // Step 3: Create via API
        let api_start = Instant::now();
        let api_response = self.api.create_token(&access_token, &config, &image_url).await?;
        benchmarks.api_ms = api_start.elapsed().as_millis() as u64;
        info!("[FOUR.MEME] API response in {}ms", benchmarks.api_ms);

        // Step 4: Send on-chain transaction (HIGH GAS for speed)
        let tx_start = Instant::now();
        let required = self.calculate_required_bnb(config.presale_bnb);
        let tx_result = self
            .blockchain
            .send_create_token(&api_response.create_arg, &api_response.signature, required)
            .await?;
        benchmarks.tx_ms = tx_start.elapsed().as_millis() as u64;

        let total_time = start_time.elapsed();
        benchmarks.total_ms = total_time.as_millis() as u64;

        // Print benchmarks
        info!("[FOUR.MEME] ==================== BENCHMARK ====================");
        info!("[FOUR.MEME]   Auth:     {}ms (cached: {})", benchmarks.auth_ms, benchmarks.auth_ms < 50);
        info!("[FOUR.MEME]   Upload:   {}ms", benchmarks.upload_ms);
        info!("[FOUR.MEME]   API:      {}ms", benchmarks.api_ms);
        info!("[FOUR.MEME]   TX+Wait:  {}ms", benchmarks.tx_ms);
        info!("[FOUR.MEME]   ─────────────────────────────");
        info!("[FOUR.MEME]   TOTAL:    {}ms", benchmarks.total_ms);
        info!("[FOUR.MEME] =====================================================");
        info!("[FOUR.MEME] Token: {}", tx_result.token_address);

        // Build links before moving values
        let links = TokenLinks {
            bscscan: format!("https://bscscan.com/token/{}", tx_result.token_address),
            four_meme: format!("https://four.meme/token/{}", tx_result.token_address),
            pancakeswap: format!(
                "https://pancakeswap.finance/swap?outputCurrency={}",
                tx_result.token_address
            ),
            tx: format!("https://bscscan.com/tx/{}", tx_result.tx_hash),
        };

        Ok(TokenResult {
            success: true,
            token_address: tx_result.token_address,
            tx_hash: tx_result.tx_hash,
            block_number: tx_result.block_number,
            gas_used: tx_result.gas_used,
            creation_time_ms: total_time.as_millis() as u64,
            benchmarks: Some(benchmarks),
            links,
        })
    }

    /// Authenticate with Four.Meme API (uses cache)
    async fn authenticate(&self) -> Result<String, FourMemeError> {
        let address_str = format!("{:?}", self.wallet_address);
        
        // Check cache first
        if let Some(token) = get_cached_auth(&address_str) {
            return Ok(token);
        }

        // Fresh auth
        self.authenticate_fresh().await
    }

    /// Authenticate without cache
    pub async fn authenticate_fresh(&self) -> Result<String, FourMemeError> {
        let address_str = format!("{:?}", self.wallet_address);
        
        info!("[FOUR.MEME] Auth: Getting nonce...");
        let nonce = self.api.get_nonce(self.wallet_address).await?;

        info!("[FOUR.MEME] Auth: Signing...");
        let message = format!("You are sign in Meme {}", nonce);
        let signature = self.blockchain.sign_message(&message).await?;

        info!("[FOUR.MEME] Auth: Login...");
        let access_token = self.api.login(self.wallet_address, &signature).await?;

        // Cache for 5 minutes
        set_cached_auth(&address_str, &access_token);
        info!("[FOUR.MEME] Auth: Cached for 5 minutes");

        Ok(access_token)
    }

    /// Validate token configuration
    fn validate_config(&self, config: &TokenConfig) -> Result<(), FourMemeError> {
        if config.ticker.is_empty() {
            return Err(FourMemeError::ValidationError("Ticker is required".to_string()));
        }

        if config.name.is_empty() {
            return Err(FourMemeError::ValidationError("Name is required".to_string()));
        }

        if config.image.is_empty() && config.pre_uploaded_image_url.is_none() {
            return Err(FourMemeError::ValidationError(
                "Image is required (provide either image bytes or pre_uploaded_image_url)".to_string()
            ));
        }

        if !VALID_LABELS.contains(&config.label.as_str()) {
            return Err(FourMemeError::ValidationError(format!(
                "Invalid label '{}'. Valid labels: {:?}",
                config.label, VALID_LABELS
            )));
        }

        if config.presale_bnb < 0.0 {
            return Err(FourMemeError::ValidationError(
                "Presale amount cannot be negative".to_string(),
            ));
        }

        Ok(())
    }

    /// Calculate required BNB (creation fee + presale + buy fee)
    fn calculate_required_bnb(&self, presale_bnb: f64) -> U256 {
        let creation_fee = ethers::utils::parse_ether(CREATION_FEE.to_string()).unwrap();
        let presale = ethers::utils::parse_ether(presale_bnb.to_string()).unwrap();
        let buy_fee = presale / U256::from(100 / BUY_FEE_PERCENT);

        creation_fee + presale + buy_fee
    }
}

// ============================================================================
// BUY TOKENS
// ============================================================================

impl FourMeme {
    /// Buy tokens on Four.Meme bonding curve
    pub async fn buy_token(
        &self,
        token_address: Address,
        amount_bnb: f64,
    ) -> Result<BuyResult, FourMemeError> {
        info!("[FOUR.MEME] Buying {} BNB of token {:?}", amount_bnb, token_address);

        // Authenticate (cached)
        let _access_token = self.authenticate().await?;

        // Send buy transaction
        let tx_result = self.blockchain.send_buy_token(token_address, amount_bnb).await?;

        info!("[FOUR.MEME] Buy completed: {}", tx_result.tx_hash);

        Ok(BuyResult {
            success: true,
            tx_hash: tx_result.tx_hash,
            gas_used: tx_result.gas_used,
        })
    }
}
