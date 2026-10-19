//! Error types for Four.Meme

use thiserror::Error;

/// Main error type for Four.Meme operations
#[derive(Error, Debug)]
pub enum FourMemeError {
    /// Invalid private key format
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),

    /// Network/RPC error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// API error from Four.Meme
    #[error("API error: {0}")]
    ApiError(String),

    /// Insufficient balance for operation
    #[error("Insufficient balance: need {required} BNB, have {available} BNB")]
    InsufficientBalance { required: String, available: String },

    /// Transaction failed
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    /// Transaction reverted on-chain
    #[error("Transaction reverted")]
    TransactionReverted,

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Signature error
    #[error("Signature error: {0}")]
    SignatureError(String),

    /// Could not extract token address from receipt
    #[error("Could not extract token address from transaction receipt")]
    TokenAddressNotFound,

    /// HTTP request error
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// JSON parsing error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Hex decoding error
    #[error("Hex decode error: {0}")]
    HexError(#[from] hex::FromHexError),

    /// Ethers provider error
    #[error("Provider error: {0}")]
    ProviderError(String),

    /// Ethers wallet error
    #[error("Wallet error: {0}")]
    WalletError(String),

    /// Contract error
    #[error("Contract error: {0}")]
    ContractError(String),

    /// Timeout error
    #[error("Operation timed out: {0}")]
    Timeout(String),
}

impl From<ethers::providers::ProviderError> for FourMemeError {
    fn from(err: ethers::providers::ProviderError) -> Self {
        FourMemeError::ProviderError(err.to_string())
    }
}

impl From<ethers::signers::WalletError> for FourMemeError {
    fn from(err: ethers::signers::WalletError) -> Self {
        FourMemeError::WalletError(err.to_string())
    }
}

