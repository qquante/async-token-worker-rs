//! Type definitions for Four.Meme

use serde::{Deserialize, Serialize};

// TOKEN CONFIGURATION

/// Configuration for creating a new token
#[derive(Debug, Clone)]
pub struct TokenConfig {
    /// Token ticker/symbol (e.g., "PEPE")
    pub ticker: String,

    /// Token name (e.g., "Pepe Token")
    pub name: String,

    /// Token description (optional, defaults to name)
    pub description: Option<String>,

    /// Token logo image as bytes (PNG/JPG)
    /// Only used if pre_uploaded_image_url is None
    pub image: Vec<u8>,

    /// Pre-uploaded image URL from Four.Meme (OPTIMIZATION)
    /// If set, skips upload step entirely
    pub pre_uploaded_image_url: Option<String>,

    /// Token label category
    pub label: String,

    /// Presale amount in BNB (creator's initial buy)
    pub presale_bnb: f64,

    /// Twitter URL (optional)
    pub twitter_url: Option<String>,

    /// Telegram URL (optional)
    pub telegram_url: Option<String>,

    /// Website URL (optional)
    pub website_url: Option<String>,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            ticker: String::new(),
            name: String::new(),
            description: None,
            image: Vec::new(),
            pre_uploaded_image_url: None,
            label: "Meme".to_string(),
            presale_bnb: 0.0,
            twitter_url: None,
            telegram_url: None,
            website_url: None,
        }
    }
}

// RESULTS

/// Result of token creation
#[derive(Debug, Clone, Serialize)]
pub struct TokenResult {
    /// Whether creation was successful
    pub success: bool,

    /// Created token contract address
    pub token_address: String,

    /// Transaction hash
    pub tx_hash: String,

    /// Block number where token was created
    pub block_number: u64,

    /// Gas used for transaction
    pub gas_used: String,

    /// Total time in milliseconds
    pub creation_time_ms: u64,

    /// Detailed benchmarks (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmarks: Option<Benchmarks>,

    /// Useful links
    pub links: TokenLinks,
}

/// Detailed benchmarks for each step
#[derive(Debug, Clone, Default, Serialize)]
pub struct Benchmarks {
    /// Auth time in ms (0 if cached)
    pub auth_ms: u64,
    /// Image upload time in ms
    pub upload_ms: u64,
    /// API call time in ms
    pub api_ms: u64,
    /// TX send + wait time in ms
    pub tx_ms: u64,
    /// Total time in ms
    pub total_ms: u64,
}

/// Useful links for the created token
#[derive(Debug, Clone, Serialize)]
pub struct TokenLinks {
    /// BSCScan token page
    pub bscscan: String,

    /// Four.Meme token page
    pub four_meme: String,

    /// PancakeSwap swap page
    pub pancakeswap: String,

    /// Transaction on BSCScan
    pub tx: String,
}

/// Result of token purchase
#[derive(Debug, Clone, Serialize)]
pub struct BuyResult {
    /// Whether buy was successful
    pub success: bool,

    /// Transaction hash
    pub tx_hash: String,

    /// Gas used
    pub gas_used: String,
}

/// Internal transaction result
#[derive(Debug, Clone)]
pub struct TxResult {
    pub token_address: String,
    pub tx_hash: String,
    pub block_number: u64,
    pub gas_used: String,
}

// API TYPES

/// Response from nonce generation
#[derive(Debug, Deserialize)]
pub struct NonceResponse {
    pub code: serde_json::Value, // Can be 0 or "0"
    pub data: String,
    pub msg: Option<String>,
}

/// Response from login
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub code: serde_json::Value,
    pub data: String, // Access token
    pub msg: Option<String>,
}

/// Response from image upload
#[derive(Debug, Deserialize)]
pub struct UploadResponse {
    pub code: serde_json::Value,
    pub data: String, // Image URL
    pub msg: Option<String>,
}

/// Response from token creation API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenApiResponse {
    pub code: serde_json::Value,
    pub data: CreateTokenData,
    pub msg: Option<String>,
}

/// Data from token creation API
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenData {
    pub token_id: u64,
    pub create_arg: String,
    pub signature: String,
}

/// Raised token configuration (required by API)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RaisedTokenConfig {
    pub symbol: String,
    pub native_symbol: String,
    pub symbol_address: String,
    pub deploy_cost: String,
    pub buy_fee: String,
    pub sell_fee: String,
    pub min_trade_fee: String,
    pub b0_amount: String,
    pub total_b_amount: String,
    pub total_amount: String,
    pub logo_url: String,
    pub trade_level: Vec<String>,
    pub status: String,
    pub buy_token_link: String,
    pub reserved_number: u32,
    pub sale_rate: String,
    pub network_code: String,
    pub platform: String,
}

impl Default for RaisedTokenConfig {
    fn default() -> Self {
        Self {
            symbol: "BNB".to_string(),
            native_symbol: "BNB".to_string(),
            symbol_address: "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c".to_string(),
            deploy_cost: "0".to_string(),
            buy_fee: "0.01".to_string(),
            sell_fee: "0.01".to_string(),
            min_trade_fee: "0".to_string(),
            b0_amount: "8".to_string(),
            total_b_amount: "24".to_string(),
            total_amount: "1000000000".to_string(),
            logo_url: "https://static.four.meme/market/68b871b6-96f7-408c-b8d0-388d804b34275092658264263839640.png".to_string(),
            trade_level: vec!["0.1".to_string(), "0.5".to_string(), "1".to_string()],
            status: "PUBLISH".to_string(),
            buy_token_link: "https://pancakeswap.finance/swap".to_string(),
            reserved_number: 10,
            sale_rate: "0.8".to_string(),
            network_code: "BSC".to_string(),
            platform: "MEME".to_string(),
        }
    }
}

/// Token creation request payload
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenRequest {
    pub name: String,
    pub short_name: String,
    pub desc: String,
    pub img_url: String,
    pub launch_time: u64,
    pub label: String,
    pub pre_sale: String,
    pub only_mpc: bool,
    pub lp_trading_fee: f64,
    pub total_supply: u64,
    pub raised_amount: u32,
    pub sale_rate: f64,
    pub reserve_rate: u32,
    pub fun_group: bool,
    pub click_fun: bool,
    pub symbol: String,
    pub raised_token: RaisedTokenConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_url: Option<String>,
}

