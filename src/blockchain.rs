//! Blockchain interaction module for BSC - OPTIMIZED
//!
//! Features:
//! - Configurable gas price for faster confirmations
//! - No unnecessary balance checks
//! - Optimized transaction handling
//! - Multi-RPC broadcast for maximum speed

use crate::broadcaster::TxBroadcaster;
use crate::error::FourMemeError;
use crate::types::TxResult;
use crate::TOKEN_MANAGER_ADDRESS;
use ethers::prelude::*;
use ethers::types::transaction::eip2718::TypedTransaction;
use std::sync::Arc;
use tracing::{debug, info, warn};

// TokenManager ABI (minimal - only what we need)
abigen!(
    TokenManager,
    r#"[
        function createToken(bytes calldata createArg, bytes calldata sign) external payable
        function buyToken(address token, uint256 minOut) external payable
        event TokenCreate(address indexed creator, address indexed token, uint256 requestId, string name, string symbol, uint256 totalSupply, uint256 launchTime, uint256 launchFee)
    ]"#
);

/// Blockchain client for BSC operations - OPTIMIZED WITH FIRE MODE
/// 
/// ALWAYS broadcasts to multiple RPCs simultaneously:
/// - 48Club (Privacy RPC) - ~45% of BSC blocks
/// - bloXroute (BDN) - ~15% of BSC blocks
/// - BlockRazor - ~37% of BSC blocks
/// - Ankr, Chainstack - Backup
pub struct Blockchain {
    provider: Arc<Provider<Http>>,
    wallet: LocalWallet,
    signer: Arc<SignerMiddleware<Arc<Provider<Http>>, LocalWallet>>,
    chain_id: u64,
    /// Gas price in gwei
    gas_gwei: u64,
    /// Multi-RPC broadcaster - ALWAYS ACTIVE
    broadcaster: TxBroadcaster,
}

impl Blockchain {
    /// Create a new blockchain client
    ///
    /// # Arguments
    /// * `private_key` - Wallet private key (with or without 0x prefix)
    /// * `rpc_url` - BSC RPC URL
    /// * `gas_gwei` - Gas price in gwei (5 = ~2-3s confirmation)
    ///
    /// # Returns
    /// * `Result<Blockchain>` - Client instance or error
    pub fn new(private_key: &str, rpc_url: &str, gas_gwei: u64) -> Result<Self, FourMemeError> {
        // Clean private key (remove 0x prefix if present)
        let clean_key = private_key.trim_start_matches("0x");

        // Parse private key
        let wallet: LocalWallet = clean_key
            .parse()
            .map_err(|e| FourMemeError::InvalidPrivateKey(format!("{}", e)))?;

        // BSC chain ID
        let chain_id = 56u64;
        let wallet = wallet.with_chain_id(chain_id);

        // Create provider
        let provider = Provider::<Http>::try_from(rpc_url)
            .map_err(|e| FourMemeError::NetworkError(e.to_string()))?;
        let provider = Arc::new(provider);

        // Create signer middleware
        let signer = SignerMiddleware::new(provider.clone(), wallet.clone());
        let signer = Arc::new(signer);

        // Create broadcaster with ALL endpoints (ALWAYS ACTIVE)
        let broadcaster = TxBroadcaster::new();
        
        info!(
            "[BLOCKCHAIN] 🔥 Initialized with FIRE MODE: wallet={:?}, gas={}gwei, endpoints={}",
            wallet.address(),
            gas_gwei,
            broadcaster.endpoints().len()
        );
        
        // Log all endpoints
        for endpoint in broadcaster.endpoints() {
            info!(
                "[BLOCKCHAIN]   📡 {} -> {} (builder: {})",
                endpoint.name, endpoint.url, endpoint.is_builder
            );
        }

        Ok(Self {
            provider,
            wallet,
            signer,
            chain_id,
            gas_gwei,
            broadcaster,
        })
    }

    /// Get reference to the broadcaster
    pub fn broadcaster(&self) -> &TxBroadcaster {
        &self.broadcaster
    }

    /// Get number of configured endpoints
    pub fn endpoint_count(&self) -> usize {
        self.broadcaster.endpoints().len()
    }

    /// Get wallet address
    pub fn wallet_address(&self) -> Address {
        self.wallet.address()
    }

    /// Get wallet BNB balance
    pub async fn get_balance(&self) -> Result<U256, FourMemeError> {
        let balance = self.provider.get_balance(self.wallet.address(), None).await?;
        Ok(balance)
    }

    /// Sign a message with the wallet
    pub async fn sign_message(&self, message: &str) -> Result<String, FourMemeError> {
        debug!("[BLOCKCHAIN] Signing message");

        let signature = self
            .wallet
            .sign_message(message)
            .await
            .map_err(|e| FourMemeError::SignatureError(e.to_string()))?;

        let sig_hex = format!("0x{}", hex::encode(signature.to_vec()));

        Ok(sig_hex)
    }

    /// Send createToken transaction to TokenManager contract - OPTIMIZED
    ///
    /// If broadcaster is enabled, fires to multiple RPCs simultaneously.
    /// Otherwise, sends to single RPC.
    pub async fn send_create_token(
        &self,
        create_arg: &str,
        signature: &str,
        value: U256,
    ) -> Result<TxResult, FourMemeError> {
        info!("[BLOCKCHAIN] Sending createToken ({}gwei)...", self.gas_gwei);
        info!("[BLOCKCHAIN]   Value: {} BNB", ethers::utils::format_ether(value));

        // Parse contract address
        let contract_address: Address = TOKEN_MANAGER_ADDRESS
            .parse()
            .map_err(|e| FourMemeError::ContractError(format!("Invalid contract address: {}", e)))?;

        // Create contract instance
        let contract = TokenManager::new(contract_address, self.signer.clone());

        // Decode hex bytes
        let create_arg_bytes = hex::decode(create_arg.trim_start_matches("0x"))
            .map_err(|e| FourMemeError::HexError(e))?;
        let signature_bytes = hex::decode(signature.trim_start_matches("0x"))
            .map_err(|e| FourMemeError::HexError(e))?;

        // Calculate gas price
        let gas_price = U256::from(self.gas_gwei) * U256::from(1_000_000_000u64); // gwei to wei

        // Build transaction with HIGH GAS for speed (clone bytes as they're used again for signing)
        let tx = contract
            .create_token(create_arg_bytes.clone().into(), signature_bytes.clone().into())
            .value(value)
            .gas_price(gas_price);

        // Estimate gas
        let gas_estimate = tx
            .estimate_gas()
            .await
            .map_err(|e| FourMemeError::ContractError(format!("Gas estimation failed: {}", e)))?;

        debug!("[BLOCKCHAIN]   Gas estimate: {}", gas_estimate);

        // Send transaction with gas buffer
        let gas_limit = gas_estimate * 120 / 100; // 20% buffer

        // =====================================================
        // 🔥 FIRE MODE - ALWAYS ACTIVE
        // =====================================================
        info!(
            "[BLOCKCHAIN] 🔥 FIRE MODE: Broadcasting to {} RPCs simultaneously!",
            self.broadcaster.endpoints().len()
        );
        
        // Build and sign raw transaction
        let signed_tx = self.build_signed_create_token_tx(
            &create_arg_bytes,
            &signature_bytes,
            value,
            gas_price,
            gas_limit,
        ).await?;
        
        // 🔥 FIRE to ALL endpoints at once!
        let fire_result = self.broadcaster.fire(&signed_tx).await?;
        
        info!("[BLOCKCHAIN] 🔥 TX FIRED SUCCESSFULLY!");
        info!("[BLOCKCHAIN]   TX Hash: {}", fire_result.tx_hash);
        info!("[BLOCKCHAIN]   First responder: {} ({}ms)", 
            fire_result.first_responder, fire_result.first_success_ms);
        info!("[BLOCKCHAIN]   Success: {}/{} endpoints", 
            fire_result.success_count, 
            fire_result.success_count + fire_result.failure_count);
        
        // Wait for confirmation on primary provider
        let tx_hash: H256 = fire_result.tx_hash.parse()
            .map_err(|e| FourMemeError::TransactionFailed(format!("Invalid tx hash: {}", e)))?;
        
        info!("[BLOCKCHAIN]   Waiting for block confirmation...");
        let receipt = self.provider
            .get_transaction_receipt(tx_hash)
            .await?;
        
        // Poll for receipt if not immediately available
        let receipt = if receipt.is_none() {
            self.wait_for_receipt(tx_hash).await?
        } else {
            receipt.unwrap()
        };

        // Check status
        if receipt.status != Some(U64::from(1)) {
            return Err(FourMemeError::TransactionReverted);
        }

        // Extract token address from logs
        let token_address = self
            .extract_token_address(&receipt)
            .ok_or(FourMemeError::TokenAddressNotFound)?;

        info!("[BLOCKCHAIN] ✅ TOKEN DEPLOYED SUCCESSFULLY!");
        info!("[BLOCKCHAIN]   Token: {}", token_address);
        info!("[BLOCKCHAIN]   Block: {}", receipt.block_number.unwrap_or_default());
        info!("[BLOCKCHAIN]   Gas used: {}", receipt.gas_used.unwrap_or_default());

        Ok(TxResult {
            token_address,
            tx_hash: format!("{:?}", tx_hash),
            block_number: receipt.block_number.unwrap_or_default().as_u64(),
            gas_used: receipt.gas_used.unwrap_or_default().to_string(),
        })
    }

    /// Build and sign a raw createToken transaction
    async fn build_signed_create_token_tx(
        &self,
        create_arg: &[u8],
        signature: &[u8],
        value: U256,
        gas_price: U256,
        gas_limit: U256,
    ) -> Result<String, FourMemeError> {
        // Parse contract address
        let contract_address: Address = TOKEN_MANAGER_ADDRESS
            .parse()
            .map_err(|e| FourMemeError::ContractError(format!("Invalid contract address: {}", e)))?;

        // Get current nonce
        let nonce = self.provider
            .get_transaction_count(self.wallet.address(), None)
            .await?;

        // Encode function call data
        // createToken(bytes calldata createArg, bytes calldata sign)
        let contract = TokenManager::new(contract_address, self.signer.clone());
        let call = contract.create_token(create_arg.to_vec().into(), signature.to_vec().into());
        let tx_data = call.tx.data().cloned().unwrap_or_default();

        // Build legacy transaction
        let tx = TransactionRequest::new()
            .to(contract_address)
            .value(value)
            .gas(gas_limit)
            .gas_price(gas_price)
            .nonce(nonce)
            .data(tx_data)
            .chain_id(self.chain_id);

        // Convert to TypedTransaction
        let typed_tx: TypedTransaction = tx.into();

        // Sign transaction
        let signature = self.wallet
            .sign_transaction(&typed_tx)
            .await
            .map_err(|e| FourMemeError::SignatureError(e.to_string()))?;

        // Encode signed transaction
        let signed_tx = typed_tx.rlp_signed(&signature);
        let hex_tx = format!("0x{}", hex::encode(signed_tx));

        debug!("[BLOCKCHAIN] Built signed TX: {} bytes", hex_tx.len() / 2);

        Ok(hex_tx)
    }

    /// Wait for transaction receipt with polling
    async fn wait_for_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt, FourMemeError> {
        let max_attempts = 60; // 60 * 500ms = 30 seconds max
        let poll_interval = std::time::Duration::from_millis(500);

        for attempt in 0..max_attempts {
            if let Some(receipt) = self.provider.get_transaction_receipt(tx_hash).await? {
                return Ok(receipt);
            }
            
            if attempt % 10 == 0 {
                debug!("[BLOCKCHAIN] Waiting for receipt... (attempt {})", attempt);
            }
            
            tokio::time::sleep(poll_interval).await;
        }

        Err(FourMemeError::Timeout("Transaction receipt not found after 30s".to_string()))
    }

    /// Extract token address from transaction receipt logs
    /// 
    /// Matches the JavaScript implementation exactly:
    /// ```js
    /// if (log.data?.length >= 130) {
    ///     return '0x' + log.data.slice(66, 130).slice(24);
    /// }
    /// ```
    fn extract_token_address(&self, receipt: &TransactionReceipt) -> Option<String> {
        let contract_address: Address = TOKEN_MANAGER_ADDRESS.parse().ok()?;

        for log in &receipt.logs {
            // Check if log is from TokenManager contract
            if log.address != contract_address {
                continue;
            }

            // In Rust ethers, log.data is Bytes (raw bytes)
            // JS expects hex string length >= 130 (including 0x = 64+ bytes)
            if log.data.len() >= 65 {
                // JS: slice(66, 130).slice(24) on hex string
                // = bytes 45-65 in raw form (20 bytes = address)
                let token_bytes = &log.data[45..65];
                let token_address = format!("0x{}", hex::encode(token_bytes));
                info!("[BLOCKCHAIN] Extracted token from log data: {}", token_address);
                return Some(token_address);
            }
        }

        // Alternative: check indexed topics (TokenCreate event)
        for log in &receipt.logs {
            if log.address != contract_address {
                continue;
            }
            
            // Token address is the 2nd indexed parameter (topics[2])
            if log.topics.len() >= 3 {
                let token_topic = log.topics[2];
                let token_address = Address::from(token_topic);
                let token_str = format!("{:?}", token_address);
                info!("[BLOCKCHAIN] Extracted token from topic: {}", token_str);
                return Some(token_str);
            }
        }

        warn!("[BLOCKCHAIN] Could not extract token address from receipt");
        None
    }

    /// Send buyToken transaction - OPTIMIZED
    pub async fn send_buy_token(
        &self,
        token_address: Address,
        amount_bnb: f64,
    ) -> Result<TxResult, FourMemeError> {
        info!("[BLOCKCHAIN] Buying {:?} ({}gwei)", token_address, self.gas_gwei);

        let contract_address: Address = TOKEN_MANAGER_ADDRESS
            .parse()
            .map_err(|e| FourMemeError::ContractError(format!("Invalid contract address: {}", e)))?;

        let value = ethers::utils::parse_ether(amount_bnb.to_string())
            .map_err(|e| FourMemeError::ValidationError(format!("Invalid amount: {}", e)))?;

        let gas_price = U256::from(self.gas_gwei) * U256::from(1_000_000_000u64);

        // Create contract instance
        let contract = TokenManager::new(contract_address, self.signer.clone());

        // Build and send transaction
        let tx_call = contract
            .buy_token(token_address, U256::zero())
            .value(value)
            .gas_price(gas_price);
        let pending_tx = tx_call
            .send()
            .await
            .map_err(|e| FourMemeError::TransactionFailed(format!("Send failed: {}", e)))?;

        let tx_hash = pending_tx.tx_hash();
        info!("[BLOCKCHAIN]   TX: {:?}", tx_hash);

        // Wait for confirmation
        let receipt = pending_tx
            .await
            .map_err(|e| FourMemeError::TransactionFailed(format!("Wait failed: {}", e)))?
            .ok_or(FourMemeError::TransactionFailed(
                "No receipt received".to_string(),
            ))?;

        if receipt.status != Some(U64::from(1)) {
            return Err(FourMemeError::TransactionReverted);
        }

        Ok(TxResult {
            token_address: format!("{:?}", token_address),
            tx_hash: format!("{:?}", tx_hash),
            block_number: receipt.block_number.unwrap_or_default().as_u64(),
            gas_used: receipt.gas_used.unwrap_or_default().to_string(),
        })
    }
}
