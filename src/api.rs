//! Four.Meme API client
//!
//! Uses cf_clearance cookie from Redis (set by cloudflare-bypass worker)
//! to bypass Cloudflare protection.
//!
//! Architecture:
//! - Main client (no proxy): for auth (nonce, login, create_token) - uses CF cookie
//! - Upload client (with proxy): ONLY for image upload - avoids IP rate limit

use crate::error::FourMemeError;
use crate::types::*;
use crate::API_BASE;
use ethers::prelude::*;
use reqwest::{header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT}, multipart, Client, Proxy};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cloudflare credentials cache
#[derive(Clone, Default)]
struct CloudflareCredentials {
    cookie: Option<String>,
    user_agent: Option<String>,
    fetched_at: u64,
}

/// Four.Meme API client
pub struct FourMemeApi {
    client: Client,                           // Main client - NO proxy (for auth)
    upload_client: OnceLock<Client>,          // Lazy upload client - WITH proxy (sync init)
    redis_url: Option<String>,
    cf_cache: Arc<RwLock<CloudflareCredentials>>,
}

impl FourMemeApi {
    /// Build main HTTP client (NO proxy - for auth calls that need CF cookie)
    /// IMPORTANT: .no_proxy() disables automatic HTTPS_PROXY env var detection
    fn build_client() -> Client {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .cookie_store(true)
            .no_proxy()  // Disable automatic proxy from env - uses server IP directly
            .build()
            .expect("Failed to create HTTP client")
    }
    
    /// Build upload client WITH proxy (for image uploads to avoid rate limits)
    fn build_upload_client() -> Client {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(60))  // Longer timeout for uploads
            .cookie_store(true);
        
        // Check for HTTPS_PROXY env var
        if let Ok(proxy_url) = std::env::var("HTTPS_PROXY") {
            if !proxy_url.is_empty() {
                match Proxy::all(&proxy_url) {
                    Ok(proxy) => {
                        builder = builder.proxy(proxy);
                        // Log proxy (hide credentials)
                        let safe_url = proxy_url.split('@').last().unwrap_or(&proxy_url);
                        info!("[FOUR.MEME-API] Upload client using proxy: {}", safe_url);
                    }
                    Err(e) => {
                        warn!("[FOUR.MEME-API] Failed to configure upload proxy: {}", e);
                    }
                }
            }
        }
        
        builder.build().expect("Failed to create upload HTTP client")
    }
    
    /// Get or create the upload client (lazy initialization)
    fn get_upload_client(&self) -> &Client {
        self.upload_client.get_or_init(|| Self::build_upload_client())
    }
    
    /// Create a new API client
    pub fn new() -> Self {
        let client = Self::build_client();

        Self { 
            client,
            upload_client: OnceLock::new(),
            redis_url: None,
            cf_cache: Arc::new(RwLock::new(CloudflareCredentials::default())),
        }
    }
    
    /// Create a new API client with Redis URL for Cloudflare cookie
    pub fn with_redis(redis_url: &str) -> Self {
        let client = Self::build_client();

        Self { 
            client,
            upload_client: OnceLock::new(),
            redis_url: Some(redis_url.to_string()),
            cf_cache: Arc::new(RwLock::new(CloudflareCredentials::default())),
        }
    }
    
    /// Get Cloudflare credentials from Redis
    async fn get_cf_credentials(&self) -> CloudflareCredentials {
        // Check cache first (refresh every 60 seconds)
        {
            let cache = self.cf_cache.read().await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            if cache.cookie.is_some() && now - cache.fetched_at < 60 {
                return cache.clone();
            }
        }
        
        // Fetch from Redis
        if let Some(ref redis_url) = self.redis_url {
            match redis::Client::open(redis_url.as_str()) {
                Ok(client) => {
                    if let Ok(mut conn) = client.get_connection() {
                        let cookie: Option<String> = redis::cmd("GET")
                            .arg("cloudflare:cookie")
                            .query(&mut conn)
                            .ok();
                        
                        let user_agent: Option<String> = redis::cmd("GET")
                            .arg("cloudflare:user_agent")
                            .query(&mut conn)
                            .ok();
                        
                        if cookie.is_some() {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            
                            let creds = CloudflareCredentials {
                                cookie,
                                user_agent,
                                fetched_at: now,
                            };
                            
                            // Update cache
                            {
                                let mut cache = self.cf_cache.write().await;
                                *cache = creds.clone();
                            }
                            
                            debug!("Got Cloudflare cookie from Redis");
                            return creds;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to connect to Redis for CF cookie: {}", e);
                }
            }
        }
        
        CloudflareCredentials::default()
    }
    
    /// Build headers with Cloudflare cookie
    async fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        
        let cf = self.get_cf_credentials().await;
        
        if let Some(ref cookie) = cf.cookie {
            if let Ok(val) = HeaderValue::from_str(&format!("cf_clearance={}", cookie)) {
                headers.insert(COOKIE, val);
                debug!("[CF-BYPASS] Using cf_clearance cookie ({}...)", &cookie[..cookie.len().min(20)]);
            }
        } else {
            warn!("[CF-BYPASS] No cookie found in Redis!");
        }
        
        if let Some(ref ua) = cf.user_agent {
            if let Ok(val) = HeaderValue::from_str(ua) {
                headers.insert(USER_AGENT, val);
                debug!("[CF-BYPASS] Using User-Agent from Redis");
            }
        } else {
            headers.insert(
                USER_AGENT, 
                HeaderValue::from_static("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            );
            warn!("[CF-BYPASS] Using fallback User-Agent (not from Redis)");
        }
        
        headers
    }

    /// Get nonce for authentication
    ///
    /// # Arguments
    /// * `address` - Wallet address
    ///
    /// # Returns
    /// * `Result<String>` - Nonce string
    pub async fn get_nonce(&self, address: Address) -> Result<String, FourMemeError> {
        // Format address as checksum address (0x + 40 hex chars)
        // ethers Address Debug format is already checksum: "0x..."
        let address_str = format!("{:?}", address);
        debug!("Getting nonce for {}", address_str);

        let url = format!("{}/v1/private/user/nonce/generate", API_BASE);

        let payload = json!({
            "accountAddress": address_str,
            "verifyType": "LOGIN",
            "networkCode": "BSC"
        });

        // Get headers with Cloudflare cookie
        let headers = self.build_headers().await;

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        debug!("Nonce API status: {}, body: {}", status, &text[..text.len().min(500)]);

        if text.is_empty() {
            return Err(FourMemeError::ApiError("Empty response from nonce API".to_string()));
        }

        let response: NonceResponse = serde_json::from_str(&text).map_err(|e| {
            FourMemeError::ApiError(format!("Failed to parse nonce response: {}. Body: {}", e, &text[..text.len().min(200)]))
        })?;

        // Check response code (can be 0 or "0")
        let code = match &response.code {
            serde_json::Value::Number(n) => n.as_i64().unwrap_or(-1),
            serde_json::Value::String(s) => s.parse().unwrap_or(-1),
            _ => -1,
        };

        if code != 0 {
            return Err(FourMemeError::ApiError(format!(
                "Nonce error: {:?}",
                response.msg
            )));
        }

        info!("Nonce received");
        Ok(response.data)
    }

    /// Login to Four.Meme
    ///
    /// # Arguments
    /// * `address` - Wallet address
    /// * `signature` - Signed message signature
    ///
    /// # Returns
    /// * `Result<String>` - Access token
    pub async fn login(&self, address: Address, signature: &str) -> Result<String, FourMemeError> {
        // Format address as checksum address (same as JS wallet.address)
        let address_str = format!("{:?}", address);
        debug!("Logging in for {}", address_str);

        let url = format!("{}/v1/private/user/login/dex", API_BASE);

        let payload = json!({
            "region": "WEB",
            "langType": "EN",
            "loginIp": "",
            "inviteCode": "",
            "verifyInfo": {
                "address": address_str,
                "networkCode": "BSC",
                "signature": signature,
                "verifyType": "LOGIN"
            },
            "walletName": "MetaMask"
        });

        // Get headers with Cloudflare cookie
        let headers = self.build_headers().await;

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        debug!("Login API status: {}, body: {}", status, &text[..text.len().min(500)]);

        if text.is_empty() {
            return Err(FourMemeError::ApiError("Empty response from login API".to_string()));
        }

        let response: LoginResponse = serde_json::from_str(&text).map_err(|e| {
            FourMemeError::ApiError(format!("Failed to parse login response: {}. Body: {}", e, &text[..text.len().min(200)]))
        })?;

        let code = match &response.code {
            serde_json::Value::Number(n) => n.as_i64().unwrap_or(-1),
            serde_json::Value::String(s) => s.parse().unwrap_or(-1),
            _ => -1,
        };

        if code != 0 {
            return Err(FourMemeError::ApiError(format!(
                "Login error: {:?}",
                response.msg
            )));
        }

        info!("Login successful");
        Ok(response.data)
    }

    /// Upload image to Four.Meme
    /// 
    /// Uses separate client WITH proxy to avoid IP-based rate limits.
    /// This is isolated from auth calls which need the CF cookie without proxy.
    ///
    /// # Arguments
    /// * `access_token` - Access token from login
    /// * `image_data` - Image bytes
    ///
    /// # Returns
    /// * `Result<String>` - Image URL
    pub async fn upload_image(
        &self,
        access_token: &str,
        image_data: &[u8],
    ) -> Result<String, FourMemeError> {
        debug!("Uploading image ({} bytes) via proxy client", image_data.len());

        let url = format!("{}/v1/private/token/upload", API_BASE);

        // Create multipart form
        let part = multipart::Part::bytes(image_data.to_vec())
            .file_name("token.png")
            .mime_str("image/png")
            .map_err(|e| FourMemeError::ApiError(e.to_string()))?;

        let form = multipart::Form::new().part("file", part);

        // Get headers with Cloudflare cookie
        let mut headers = self.build_headers().await;
        if let Ok(val) = HeaderValue::from_str(access_token) {
            headers.insert("meme-web-access", val);
        }

        // Use upload_client (with proxy) instead of main client
        let response = self
            .get_upload_client()
            .post(&url)
            .headers(headers)
            .multipart(form)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        debug!("Upload API status: {}, body: {}", status, &text[..text.len().min(500)]);

        if text.is_empty() {
            return Err(FourMemeError::ApiError("Empty response from upload API".to_string()));
        }

        let response: UploadResponse = serde_json::from_str(&text).map_err(|e| {
            FourMemeError::ApiError(format!("Failed to parse upload response: {}. Body: {}", e, &text[..text.len().min(200)]))
        })?;

        let code = match &response.code {
            serde_json::Value::Number(n) => n.as_i64().unwrap_or(-1),
            serde_json::Value::String(s) => s.parse().unwrap_or(-1),
            _ => -1,
        };

        if code != 0 {
            return Err(FourMemeError::ApiError(format!(
                "Upload error: {:?}",
                response.msg
            )));
        }

        info!("Image uploaded successfully");
        Ok(response.data)
    }

    /// Create token via API (get createArg and signature)
    ///
    /// # Arguments
    /// * `access_token` - Access token from login
    /// * `config` - Token configuration
    /// * `image_url` - Uploaded image URL
    ///
    /// # Returns
    /// * `Result<CreateTokenData>` - Token ID, createArg, and signature
    pub async fn create_token(
        &self,
        access_token: &str,
        config: &TokenConfig,
        image_url: &str,
    ) -> Result<CreateTokenData, FourMemeError> {
        debug!("Creating token via API: {} - {}", config.ticker, config.name);

        let url = format!("{}/v1/private/token/create", API_BASE);

        let now = chrono::Utc::now().timestamp_millis() as u64;

        let request = CreateTokenRequest {
            name: config.name.clone(),
            short_name: config.ticker.clone(),
            desc: config.description.clone().unwrap_or_else(|| config.name.clone()),
            img_url: image_url.to_string(),
            launch_time: now,
            label: config.label.clone(),
            pre_sale: config.presale_bnb.to_string(),
            only_mpc: false,
            lp_trading_fee: 0.0025,
            total_supply: 1_000_000_000,
            raised_amount: 24,
            sale_rate: 0.8,
            reserve_rate: 0,
            fun_group: false,
            click_fun: false,
            symbol: "BNB".to_string(),
            raised_token: RaisedTokenConfig::default(),
            web_url: config.website_url.clone(),
            twitter_url: config.twitter_url.clone(),
            telegram_url: config.telegram_url.clone(),
        };

        // Get headers with Cloudflare cookie
        let mut headers = self.build_headers().await;
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        if let Ok(val) = HeaderValue::from_str(access_token) {
            headers.insert("meme-web-access", val);
        }

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        debug!("API response status: {}", status);
        debug!("API response body: {}", text);

        if !status.is_success() {
            return Err(FourMemeError::ApiError(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        let parsed: CreateTokenApiResponse = serde_json::from_str(&text)?;

        let code = match &parsed.code {
            serde_json::Value::Number(n) => n.as_i64().unwrap_or(-1),
            serde_json::Value::String(s) => s.parse().unwrap_or(-1),
            _ => -1,
        };

        if code != 0 {
            return Err(FourMemeError::ApiError(format!(
                "Create error: {:?}",
                parsed.msg
            )));
        }

        info!("Token created via API: tokenId={}", parsed.data.token_id);
        Ok(parsed.data)
    }
}

impl Default for FourMemeApi {
    fn default() -> Self {
        Self::new()
    }
}

