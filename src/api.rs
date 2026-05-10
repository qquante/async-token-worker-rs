//! Four.Meme API client

use crate::error::FourMemeError;
use crate::types::*;
use crate::API_BASE;
use ethers::prelude::*;
use reqwest::{multipart, Client};
use serde_json::json;
use std::time::Duration;
use tracing::{debug, info};

/// Four.Meme API client
pub struct FourMemeApi {
    client: Client,
}

impl FourMemeApi {
    /// Create a new API client
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
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

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await?
            .json::<NonceResponse>()
            .await?;

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

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await?
            .json::<LoginResponse>()
            .await?;

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
        debug!("Uploading image ({} bytes)", image_data.len());

        let url = format!("{}/v1/private/token/upload", API_BASE);

        // Create multipart form
        let part = multipart::Part::bytes(image_data.to_vec())
            .file_name("token.png")
            .mime_str("image/png")
            .map_err(|e| FourMemeError::ApiError(e.to_string()))?;

        let form = multipart::Form::new().part("file", part);

        let response = self
            .client
            .post(&url)
            .header("meme-web-access", access_token)
            .multipart(form)
            .send()
            .await?
            .json::<UploadResponse>()
            .await?;

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

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("meme-web-access", access_token)
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

