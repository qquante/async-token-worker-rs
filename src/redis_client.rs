//! Redis Pub/Sub client for Four.Meme worker
//!
//! Channels:
//! - `deployer:en` - Token deployer English channel
//! - `deployer:zh` - Token deployer Chinese channel
//! - `book:en` - Book watcher English channel
//! - `book:zh` - Book watcher Chinese channel
//! - `vip:tweets:new` - VIP tweets from capture worker
//! - `token:created` - Published when token is created

use redis::{aio::ConnectionManager, AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::error::FourMemeError;

// ============================================================================
// CHANNELS
// ============================================================================

pub mod channels {
    // =========================================================================
    // TOKEN DEPLOYER channels (from token-deployer JS worker)
    // =========================================================================
    /// Token Deployer - English channel
    pub const DEPLOYER_EN: &str = "deployer:en";
    /// Token Deployer - Chinese channel
    pub const DEPLOYER_ZH: &str = "deployer:zh";

    // =========================================================================
    // BOOK WATCHER channels (from cz-book-watcher JS worker)
    // =========================================================================
    /// Book Watcher - English channel
    pub const BOOK_EN: &str = "book:en";
    /// Book Watcher - Chinese channel
    pub const BOOK_ZH: &str = "book:zh";

    // =========================================================================
    // OTHER channels
    // =========================================================================
    /// VIP tweets from capture worker
    pub const VIP_TWEETS_NEW: &str = "vip:tweets:new";
    /// Token created notification
    pub const TOKEN_CREATED: &str = "token:created";
    /// Image generation request
    pub const IMAGE_GENERATE: &str = "image:generate";
    /// Image ready response
    pub const IMAGE_READY: &str = "image:ready";
}

// ============================================================================
// MESSAGE TYPES
// ============================================================================

/// Token deployment request (from token-deployer or cz-book-watcher)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenDeployRequest {
    pub ticker: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Base64 encoded image (OPTION 1 - will be uploaded to Four.Meme)
    #[serde(default)]
    pub image_base64: Option<String>,
    /// External image URL - Twitter, etc (OPTION 2 - will be downloaded then uploaded)
    #[serde(default)]
    pub image_url: Option<String>,
    /// Pre-uploaded Four.Meme URL (OPTION 3 - FASTEST, skip upload)
    #[serde(default)]
    pub four_meme_image_url: Option<String>,
    #[serde(default)]
    pub tweet_url: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub arrival_time: Option<u64>,
}

/// Token created notification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCreatedMessage {
    pub lang: String,
    pub ticker: String,
    pub name: String,
    pub token_address: String,
    pub tx_hash: String,
    pub links: TokenLinks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snipe: Option<SnipeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenLinks {
    pub bscscan: String,
    pub four_meme: String,
    pub pancakeswap: String,
    pub tx: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnipeResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    pub amount: String,
}

// ============================================================================
// REDIS CLIENT
// ============================================================================

/// Redis client with pub/sub support
pub struct RedisClient {
    url: String,
    connection: Option<ConnectionManager>,
}

impl RedisClient {
    /// Create new Redis client
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            connection: None,
        }
    }

    /// Connect to Redis
    pub async fn connect(&mut self) -> Result<(), FourMemeError> {
        info!("Connecting to Redis: {}", self.url);
        
        let client = Client::open(self.url.as_str())
            .map_err(|e| FourMemeError::NetworkError(format!("Redis client error: {}", e)))?;
        
        let connection = ConnectionManager::new(client)
            .await
            .map_err(|e| FourMemeError::NetworkError(format!("Redis connection error: {}", e)))?;
        
        self.connection = Some(connection);
        info!("Redis connected");
        Ok(())
    }

    /// Get connection (auto-connect if needed)
    async fn get_connection(&mut self) -> Result<&mut ConnectionManager, FourMemeError> {
        if self.connection.is_none() {
            self.connect().await?;
        }
        self.connection.as_mut().ok_or_else(|| {
            FourMemeError::NetworkError("Redis not connected".to_string())
        })
    }

    /// Publish message to channel
    pub async fn publish<T: Serialize>(&mut self, channel: &str, data: &T) -> Result<(), FourMemeError> {
        let conn = self.get_connection().await?;
        let message = serde_json::to_string(data)
            .map_err(|e| FourMemeError::JsonError(e))?;
        
        conn.publish::<_, _, ()>(channel, &message)
            .await
            .map_err(|e| FourMemeError::NetworkError(format!("Redis publish error: {}", e)))?;
        
        debug!("Published to {}: {} bytes", channel, message.len());
        Ok(())
    }

    /// Subscribe to channel and return receiver
    pub async fn subscribe(&self, channels: Vec<&str>) -> Result<mpsc::Receiver<(String, String)>, FourMemeError> {
        eprintln!("[REDIS] subscribe() - Opening client to {}", self.url);
        
        let client = match Client::open(self.url.as_str()) {
            Ok(c) => {
                eprintln!("[REDIS] Client::open() successful");
                c
            }
            Err(e) => {
                eprintln!("[REDIS] ERROR in Client::open(): {}", e);
                return Err(FourMemeError::NetworkError(format!("Redis client error: {}", e)));
            }
        };
        
        eprintln!("[REDIS] Getting async connection...");
        let conn = match client.get_async_connection().await {
            Ok(c) => {
                eprintln!("[REDIS] get_async_connection() successful");
                c
            }
            Err(e) => {
                eprintln!("[REDIS] ERROR in get_async_connection(): {}", e);
                return Err(FourMemeError::NetworkError(format!("Redis connection error: {}", e)));
            }
        };
        
        eprintln!("[REDIS] Converting to pubsub...");
        let mut pubsub = conn.into_pubsub();
        
        for channel in &channels {
            eprintln!("[REDIS] Subscribing to channel: {}", channel);
            match pubsub.subscribe(*channel).await {
                Ok(_) => {
                    eprintln!("[REDIS] Subscribed to {} successfully", channel);
                    info!("Subscribed to channel: {}", channel);
                }
                Err(e) => {
                    eprintln!("[REDIS] ERROR subscribing to {}: {}", channel, e);
                    return Err(FourMemeError::NetworkError(format!("Redis subscribe error: {}", e)));
                }
            }
        }

        eprintln!("[REDIS] Creating mpsc channel...");
        let (tx, rx) = mpsc::channel::<(String, String)>(100);

        eprintln!("[REDIS] Spawning message receiver task...");
        // Spawn task to receive messages
        tokio::spawn(async move {
            use futures::StreamExt;
            
            eprintln!("[REDIS] Message receiver task started");
            let mut stream = pubsub.on_message();
            
            eprintln!("[REDIS] Entering message receive loop...");
            while let Some(msg) = stream.next().await {
                let channel: String = msg.get_channel_name().to_string();
                eprintln!("[REDIS] Received message on channel: {}", channel);
                let payload: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("[REDIS] ERROR getting payload: {}", e);
                        error!("Failed to get message payload: {}", e);
                        continue;
                    }
                };
                
                if tx.send((channel, payload)).await.is_err() {
                    eprintln!("[REDIS] Receiver dropped, stopping subscription");
                    warn!("Receiver dropped, stopping subscription");
                    break;
                }
            }
            eprintln!("[REDIS] Message receiver loop ended - connection may have dropped");
        });

        eprintln!("[REDIS] subscribe() returning receiver");
        Ok(rx)
    }
}

impl Clone for RedisClient {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            connection: None, // Each clone needs its own connection
        }
    }
}

// ============================================================================
// HELPERS
// ============================================================================

/// Parse deployment request from JSON string
pub fn parse_deploy_request(json: &str) -> Result<TokenDeployRequest, FourMemeError> {
    serde_json::from_str(json).map_err(|e| FourMemeError::JsonError(e))
}

/// Decode base64 image to bytes
pub fn decode_image(base64_str: &str) -> Result<Vec<u8>, FourMemeError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    
    STANDARD.decode(base64_str)
        .map_err(|e| FourMemeError::ValidationError(format!("Invalid base64 image: {}", e)))
}

/// Download image from external URL (Twitter, etc)
/// 
/// # Arguments
/// * `url` - Image URL to download
/// 
/// # Returns
/// * `Result<Vec<u8>>` - Image bytes
pub async fn download_image(url: &str) -> Result<Vec<u8>, FourMemeError> {
    use std::time::Duration;
    
    info!("[DOWNLOAD] Downloading image from: {}", url);
    let start = std::time::Instant::now();
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| FourMemeError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;
    
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await
        .map_err(|e| FourMemeError::NetworkError(format!("Failed to download image: {}", e)))?;
    
    if !response.status().is_success() {
        return Err(FourMemeError::NetworkError(format!(
            "Image download failed with status: {}",
            response.status()
        )));
    }
    
    let bytes = response
        .bytes()
        .await
        .map_err(|e| FourMemeError::NetworkError(format!("Failed to read image bytes: {}", e)))?;
    
    info!("[DOWNLOAD] Downloaded {} bytes in {:?}", bytes.len(), start.elapsed());
    
    Ok(bytes.to_vec())
}

