//! Transaction Broadcaster - Fire paralelo para múltiplos RPCs
//!
//! Envia transações assinadas para múltiplos endpoints simultaneamente
//! para maximizar a chance de inclusão rápida no próximo bloco.
//!
//! Endpoints:
//! - LOCAL POW (WebSocket) - Prioridade máxima!
//! - 48Club (Privacy RPC) - ~45% dos blocos BSC
//! - bloXroute (BDN) - ~15% dos blocos BSC  
//! - BlockRazor - ~37% dos blocos BSC (maior builder!)
//! - Ankr, Chainstack - Backup

use crate::error::FourMemeError;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};

// CONFIGURATION

/// RPC method to use for sending transactions
#[derive(Debug, Clone, PartialEq)]
pub enum RpcMethod {
    /// Standard eth_sendRawTransaction (works everywhere)
    EthSendRawTransaction,
    /// BlockRazor private transaction method
    EthSendPrivateTransaction,
    /// bloXroute BSC private transaction method (special payload format)
    BscPrivateTx,
    /// WebSocket eth_sendRawTransaction (for local POW proxy)
    WebSocket,
}

impl Default for RpcMethod {
    fn default() -> Self {
        RpcMethod::EthSendRawTransaction
    }
}

/// RPC Endpoint configuration
#[derive(Debug, Clone)]
pub struct RpcEndpoint {
    /// Endpoint name (for logging)
    pub name: String,
    /// RPC URL
    pub url: String,
    /// Authorization header (optional)
    pub auth_header: Option<String>,
    /// Priority (lower = higher priority)
    pub priority: u8,
    /// Is this a builder/privacy RPC?
    pub is_builder: bool,
    /// RPC method to use
    pub method: RpcMethod,
}

// ENDPOINT CONFIGURATION

/// bloXroute Authorization Header


/// BlockRazor Authorization Token


/// Ankr API Key


/// Chainstack API Key


/// Public defaults plus explicitly configured provider endpoints.
fn configured_default_endpoints<F>(mut read: F) -> Vec<RpcEndpoint>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut endpoints = vec![
        // TIER 0: LOCAL POW PROXY - MAXIMUM PRIORITY!
        
        // Local POW Proxy (WebSocket) - Sends directly to miners
        // Uses host.docker.internal to access host from inside Docker
        RpcEndpoint {
            name: "LocalPOW".to_string(),
            url: "ws://host.docker.internal:7788".to_string(),
            auth_header: None,
            priority: 0, // HIGHEST PRIORITY!
            is_builder: true,
            method: RpcMethod::WebSocket,
        },
        
        // TIER 1: BUILDERS (Privacy RPCs) - HIGH PRIORITY
        // Each uses the OPTIMAL method for that builder!
        
        // 48Club - Maior builder da BSC (~45% dos blocos)
        // Método: eth_sendRawTransaction (Privacy RPC já envia direto pro builder)
        RpcEndpoint {
            name: "48Club".to_string(),
            url: "https://rpc.48.club".to_string(),
            auth_header: None,
            priority: 1,
            is_builder: true,
            method: RpcMethod::EthSendRawTransaction,
        },
        
        // BlockRazor - Maior builder da BSC (~37% dos blocos!)
        // Método: eth_sendPrivateTransaction
        // URL: https://rpc.blockrazor.builders
        // Docs: https://blockrazor.gitbook.io/blockrazor/bsc/block-builder
        
        // bloXroute - BDN (~15% dos blocos)
        // Método: bsc_private_tx (método específico do bloXroute para BSC)
        // PAYLOAD ESPECIAL: {"params": {"transaction": "...", "mev_builders": ["all"]}}
        // URL: https://docs.bloxroute.com/bsc-and-eth/apis/frontrunning-protection/bsc-private-transactions
        
        // TIER 2: FAST RPCs (BACKUP - mempool público)
        
        // Ankr - Fast RPC (backup, mempool público)
        
        // Chainstack - Fast RPC (backup, mempool público)
    ];
    for (name, variable, base, method, priority, is_builder, header_auth) in [
        ("BlockRazor", "BLOCKRAZOR_AUTH", "https://rpc.blockrazor.builders", RpcMethod::EthSendPrivateTransaction, 1, true, true),
        ("bloXroute", "BLOXROUTE_AUTH", "https://api.blxrbdn.com", RpcMethod::BscPrivateTx, 1, true, true),
        ("Ankr", "ANKR_API_KEY", "https://rpc.ankr.com/bsc/", RpcMethod::EthSendRawTransaction, 10, false, false),
        ("Chainstack", "CHAINSTACK_API_KEY", "https://bsc-mainnet.core.chainstack.com/", RpcMethod::EthSendRawTransaction, 10, false, false),
    ] {
        let Some(value) = read(variable).filter(|value| !value.trim().is_empty()) else {
            continue;
        };
        let url = if header_auth { base.to_string() } else { format!("{}{}", base, value) };
        endpoints.push(RpcEndpoint {
            name: name.to_string(),
            url,
            auth_header: if header_auth { Some(value) } else { None },
            priority,
            is_builder,
            method,
        });
    }
    endpoints
}

pub fn default_endpoints() -> Vec<RpcEndpoint> {
    configured_default_endpoints(|name| std::env::var(name).ok())
}

// BROADCASTER

/// Result of a broadcast attempt to a single endpoint
#[derive(Debug, Clone)]
pub struct BroadcastResult {
    pub endpoint_name: String,
    pub success: bool,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
    pub latency_ms: u64,
}

/// Result of broadcasting to all endpoints
#[derive(Debug, Clone)]
pub struct FireResult {
    /// Transaction hash (from first successful response)
    pub tx_hash: String,
    /// Results from each endpoint
    pub results: Vec<BroadcastResult>,
    /// Total time to get first success
    pub first_success_ms: u64,
    /// Which endpoint responded first
    pub first_responder: String,
    /// Number of successful broadcasts
    pub success_count: usize,
    /// Number of failed broadcasts
    pub failure_count: usize,
}

/// JSON-RPC response
/// Note: result can be either a String (tx hash) or an Object (bloXroute returns {"txHash": "..."})
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

/// bloXroute response format for bsc_private_tx
#[derive(Debug, Deserialize)]
struct BloxrouteResult {
    #[serde(rename = "txHash")]
    tx_hash: String,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// Transaction Broadcaster
pub struct TxBroadcaster {
    client: Client,
    endpoints: Vec<RpcEndpoint>,
}

impl TxBroadcaster {
    /// Create a new broadcaster with default endpoints
    pub fn new() -> Self {
        Self::with_endpoints(default_endpoints())
    }

    /// Create a broadcaster with custom endpoints
    pub fn with_endpoints(endpoints: Vec<RpcEndpoint>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, endpoints }
    }

    /// Add bloXroute endpoint with auth header (uses bsc_private_tx)
    pub fn with_bloxroute(mut self, auth_header: &str) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: "bloXroute".to_string(),
            url: "https://api.blxrbdn.com".to_string(),
            auth_header: Some(auth_header.to_string()),
            priority: 1,
            is_builder: true,
            method: RpcMethod::BscPrivateTx,
        });
        self
    }

    /// Add Ankr endpoint (uses standard eth_sendRawTransaction)
    pub fn with_ankr(mut self, api_key: &str) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: "Ankr".to_string(),
            url: format!("https://rpc.ankr.com/bsc/{}", api_key),
            auth_header: None,
            priority: 10,
            is_builder: false,
            method: RpcMethod::EthSendRawTransaction,
        });
        self
    }

    /// Add Chainstack endpoint (uses standard eth_sendRawTransaction)
    pub fn with_chainstack(mut self, api_key: &str) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: "Chainstack".to_string(),
            url: format!("https://bsc-mainnet.core.chainstack.com/{}", api_key),
            auth_header: None,
            priority: 10,
            is_builder: false,
            method: RpcMethod::EthSendRawTransaction,
        });
        self
    }

    /// Add custom endpoint
    pub fn with_endpoint(mut self, endpoint: RpcEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// Get list of configured endpoints
    pub fn endpoints(&self) -> &[RpcEndpoint] {
        &self.endpoints
    }

    /// Fire transaction to ALL endpoints simultaneously
    ///
    /// Returns as soon as first endpoint responds with success,
    /// but continues broadcasting to others in background.
    ///
    /// # Arguments
    /// * `signed_tx` - Signed transaction bytes (with 0x prefix)
    ///
    /// # Returns
    /// * `Result<FireResult>` - Result with tx hash and endpoint stats
    pub async fn fire(&self, signed_tx: &str) -> Result<FireResult, FourMemeError> {
        let start_time = Instant::now();
        
        info!(
            "[BROADCASTER] 🔥 FIRE! Sending to {} endpoints...",
            self.endpoints.len()
        );

        // Sort by priority (builders first)
        let mut sorted_endpoints = self.endpoints.clone();
        sorted_endpoints.sort_by_key(|e| e.priority);

        // Create tasks for all endpoints
        let mut handles = Vec::new();
        
        for endpoint in &sorted_endpoints {
            let client = self.client.clone();
            let endpoint = endpoint.clone();
            let tx = signed_tx.to_string();
            
            let handle = tokio::spawn(async move {
                let result = send_to_endpoint(&client, &endpoint, &tx).await;
                (endpoint.name.clone(), result)
            });
            
            handles.push(handle);
        }

        // Collect results
        let mut results = Vec::new();
        let mut first_success: Option<(String, String, u64)> = None; // (endpoint_name, tx_hash, latency)

        for handle in handles {
            match handle.await {
                Ok((name, result)) => {
                    let latency = start_time.elapsed().as_millis() as u64;
                    
                    match result {
                        Ok(tx_hash) => {
                            info!(
                                "[BROADCASTER] ✅ {} responded in {}ms: {}",
                                name, latency, tx_hash
                            );
                            
                            if first_success.is_none() {
                                first_success = Some((name.clone(), tx_hash.clone(), latency));
                            }
                            
                            results.push(BroadcastResult {
                                endpoint_name: name,
                                success: true,
                                tx_hash: Some(tx_hash),
                                error: None,
                                latency_ms: latency,
                            });
                        }
                        Err(e) => {
                            warn!("[BROADCASTER] ❌ {} failed in {}ms: {}", name, latency, e);
                            results.push(BroadcastResult {
                                endpoint_name: name,
                                success: false,
                                tx_hash: None,
                                error: Some(e.to_string()),
                                latency_ms: latency,
                            });
                        }
                    }
                }
                Err(e) => {
                    error!("[BROADCASTER] Task panic: {}", e);
                }
            }
        }

        // Check if we got at least one success
        let (first_responder, tx_hash, first_success_ms) = first_success
            .ok_or_else(|| FourMemeError::TransactionFailed(
                "All endpoints failed to broadcast transaction".to_string()
            ))?;

        let success_count = results.iter().filter(|r| r.success).count();
        let failure_count = results.iter().filter(|r| !r.success).count();

        info!(
            "[BROADCASTER] ==================== FIRE RESULT ===================="
        );
        info!(
            "[BROADCASTER]   First responder: {} ({}ms)",
            first_responder, first_success_ms
        );
        info!(
            "[BROADCASTER]   Success: {}/{} endpoints",
            success_count,
            results.len()
        );
        info!("[BROADCASTER]   TX Hash: {}", tx_hash);
        info!(
            "[BROADCASTER] ======================================================="
        );

        Ok(FireResult {
            tx_hash,
            results,
            first_success_ms,
            first_responder,
            success_count,
            failure_count,
        })
    }

    /// Fire transaction and return immediately after first success
    /// (doesn't wait for other endpoints)
    pub async fn fire_fast(&self, signed_tx: &str) -> Result<String, FourMemeError> {
        let start_time = Instant::now();
        
        info!(
            "[BROADCASTER] ⚡ FIRE FAST! Racing {} endpoints...",
            self.endpoints.len()
        );

        // Use tokio::select! to get first success
        let mut sorted_endpoints = self.endpoints.clone();
        sorted_endpoints.sort_by_key(|e| e.priority);

        // Create futures for all endpoints
        let futures: Vec<_> = sorted_endpoints
            .iter()
            .map(|endpoint| {
                let client = self.client.clone();
                let endpoint = endpoint.clone();
                let tx = signed_tx.to_string();
                
                async move {
                    let result = send_to_endpoint(&client, &endpoint, &tx).await;
                    (endpoint.name.clone(), result)
                }
            })
            .collect();

        // Race all futures and return first success
        let results = futures::future::join_all(futures).await;
        
        for (name, result) in results {
            if let Ok(tx_hash) = result {
                let latency = start_time.elapsed().as_millis();
                info!(
                    "[BROADCASTER] ⚡ First success from {} in {}ms: {}",
                    name, latency, tx_hash
                );
                return Ok(tx_hash);
            }
        }

        Err(FourMemeError::TransactionFailed(
            "All endpoints failed".to_string()
        ))
    }
}

impl Default for TxBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

// INTERNAL FUNCTIONS

/// Send transaction to a single endpoint using the CORRECT method for that builder
async fn send_to_endpoint(
    client: &Client,
    endpoint: &RpcEndpoint,
    signed_tx: &str,
) -> Result<String, FourMemeError> {
    let start = Instant::now();
    
    debug!(
        "[BROADCASTER] Sending to {} via {:?}...", 
        endpoint.name, 
        endpoint.method
    );

    // Handle WebSocket separately
    if endpoint.method == RpcMethod::WebSocket {
        return send_via_websocket(endpoint, signed_tx).await;
    }

    // Build JSON-RPC request based on endpoint method
    let payload = match endpoint.method {
        RpcMethod::EthSendRawTransaction => {
            // Standard method - works for 48Club, Ankr, Chainstack
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_sendRawTransaction",
                "params": [signed_tx]
            })
        }
        RpcMethod::EthSendPrivateTransaction => {
            // BlockRazor private method - goes directly to builder
            // Docs: https://blockrazor.gitbook.io/blockrazor/bsc/block-builder/send-privatetransaction
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_sendPrivateTransaction",
                "params": [signed_tx]
            })
        }
        RpcMethod::BscPrivateTx => {
            // bloXroute BSC private method - sends to all MEV builders
            // Docs: https://docs.bloxroute.com/bsc-and-eth/apis/frontrunning-protection/bsc-private-transactions
            // IMPORTANT: transaction should NOT have 0x prefix for bloXroute!
            let tx_without_prefix = signed_tx.trim_start_matches("0x");
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "bsc_private_tx",
                "params": {
                    "transaction": tx_without_prefix,
                    "mev_builders": ["all"]
                }
            })
        }
        RpcMethod::WebSocket => {
            // Already handled above
            unreachable!()
        }
    };

    // Build request
    let mut request = client.post(&endpoint.url).json(&payload);

    // Add auth header if present
    if let Some(auth) = &endpoint.auth_header {
        request = request.header("Authorization", auth);
    }

    // Send request
    let response = request.send().await?;
    
    let status = response.status();
    let text = response.text().await?;

    debug!(
        "[BROADCASTER] {} response ({}ms): status={}, body={}",
        endpoint.name,
        start.elapsed().as_millis(),
        status,
        &text[..text.len().min(200)]
    );

    if !status.is_success() {
        return Err(FourMemeError::NetworkError(format!(
            "HTTP {}: {}",
            status, text
        )));
    }

    // Parse JSON-RPC response
    let rpc_response: JsonRpcResponse = serde_json::from_str(&text)?;

    if let Some(error) = rpc_response.error {
        return Err(FourMemeError::TransactionFailed(format!(
            "RPC error {}: {}",
            error.code, error.message
        )));
    }

    // Extract tx hash from result
    // Handle different response formats:
    // - String: "0x..." (standard eth_sendRawTransaction, BlockRazor)
    // - Object: {"txHash": "..."} (bloXroute bsc_private_tx)
    let result = rpc_response
        .result
        .ok_or_else(|| FourMemeError::TransactionFailed("No tx hash in response".to_string()))?;
    
    // Try to extract tx hash from different formats
    let tx_hash = if let Some(hash) = result.as_str() {
        // Standard string response (most endpoints)
        hash.to_string()
    } else if let Some(obj) = result.as_object() {
        // Object response (bloXroute: {"txHash": "..."})
        if let Some(hash) = obj.get("txHash").and_then(|v| v.as_str()) {
            // Add 0x prefix if missing
            if hash.starts_with("0x") {
                hash.to_string()
            } else {
                format!("0x{}", hash)
            }
        } else {
            return Err(FourMemeError::TransactionFailed(
                format!("Unknown response format: {:?}", obj)
            ));
        }
    } else {
        return Err(FourMemeError::TransactionFailed(
            format!("Unexpected result type: {:?}", result)
        ));
    };
    
    Ok(tx_hash)
}

/// Send transaction via WebSocket (for local POW proxy)
async fn send_via_websocket(
    endpoint: &RpcEndpoint,
    signed_tx: &str,
) -> Result<String, FourMemeError> {
    let start = Instant::now();
    
    info!("[BROADCASTER] 🔌 Connecting to WebSocket: {}", endpoint.url);
    
    // Connect to WebSocket
    let (ws_stream, _) = connect_async(&endpoint.url)
        .await
        .map_err(|e| FourMemeError::NetworkError(format!("WebSocket connect failed: {}", e)))?;
    
    let (mut write, mut read) = ws_stream.split();
    
    // Build JSON-RPC request
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_sendRawTransaction",
        "params": [signed_tx]
    });
    
    let msg = Message::Text(payload.to_string());
    
    // Send request
    write.send(msg).await
        .map_err(|e| FourMemeError::NetworkError(format!("WebSocket send failed: {}", e)))?;
    
    info!("[BROADCASTER] 🔌 Sent TX via WebSocket, waiting for response...");
    
    // Wait for response with timeout
    let response = tokio::time::timeout(
        Duration::from_secs(30),
        read.next()
    ).await
        .map_err(|_| FourMemeError::Timeout("WebSocket response timeout".to_string()))?
        .ok_or_else(|| FourMemeError::NetworkError("WebSocket closed unexpectedly".to_string()))?
        .map_err(|e| FourMemeError::NetworkError(format!("WebSocket read failed: {}", e)))?;
    
    let text = match response {
        Message::Text(t) => t,
        _ => return Err(FourMemeError::NetworkError("Unexpected WebSocket message type".to_string())),
    };
    
    info!(
        "[BROADCASTER] 🔌 {} response ({}ms): {}",
        endpoint.name,
        start.elapsed().as_millis(),
        &text[..text.len().min(200)]
    );
    
    // Parse JSON-RPC response
    let rpc_response: JsonRpcResponse = serde_json::from_str(&text)?;
    
    if let Some(error) = rpc_response.error {
        return Err(FourMemeError::TransactionFailed(format!(
            "RPC error {}: {}",
            error.code, error.message
        )));
    }
    
    // Extract tx hash
    let result = rpc_response
        .result
        .ok_or_else(|| FourMemeError::TransactionFailed("No tx hash in response".to_string()))?;
    
    let tx_hash = result.as_str()
        .ok_or_else(|| FourMemeError::TransactionFailed("Invalid tx hash format".to_string()))?
        .to_string();
    
    info!("[BROADCASTER] 🔌 WebSocket TX sent successfully: {}", tx_hash);
    
    Ok(tx_hash)
}

// BUILDER PATTERN FOR EASY CONFIGURATION

/// Builder for TxBroadcaster
pub struct BroadcasterBuilder {
    endpoints: Vec<RpcEndpoint>,
}

impl BroadcasterBuilder {
    pub fn new() -> Self {
        Self {
            endpoints: default_endpoints(),
        }
    }

    /// Start with no endpoints
    pub fn empty() -> Self {
        Self {
            endpoints: Vec::new(),
        }
    }

    /// Add bloXroute with auth header (uses bsc_private_tx method)
    pub fn bloxroute(mut self, auth_header: &str) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: "bloXroute".to_string(),
            url: "https://api.blxrbdn.com".to_string(),
            auth_header: Some(auth_header.to_string()),
            priority: 1,
            is_builder: true,
            method: RpcMethod::BscPrivateTx,
        });
        self
    }

    /// Add Ankr (uses standard eth_sendRawTransaction)
    pub fn ankr(mut self, api_key: &str) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: "Ankr".to_string(),
            url: format!("https://rpc.ankr.com/bsc/{}", api_key),
            auth_header: None,
            priority: 10,
            is_builder: false,
            method: RpcMethod::EthSendRawTransaction,
        });
        self
    }

    /// Add Chainstack (uses standard eth_sendRawTransaction)
    pub fn chainstack(mut self, api_key: &str) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: "Chainstack".to_string(),
            url: format!("https://bsc-mainnet.core.chainstack.com/{}", api_key),
            auth_header: None,
            priority: 10,
            is_builder: false,
            method: RpcMethod::EthSendRawTransaction,
        });
        self
    }

    /// Add 48Club (Privacy RPC, uses eth_sendRawTransaction)
    pub fn club48(mut self) -> Self {
        if !self.endpoints.iter().any(|e| e.name == "48Club") {
            self.endpoints.push(RpcEndpoint {
                name: "48Club".to_string(),
                url: "https://rpc.48.club".to_string(),
                auth_header: None,
                priority: 1,
                is_builder: true,
                method: RpcMethod::EthSendRawTransaction,
            });
        }
        self
    }

    /// Add BlockRazor (uses eth_sendPrivateTransaction)
    /// URL: https://rpc.blockrazor.builders
    /// Docs: https://blockrazor.gitbook.io/blockrazor/bsc/block-builder
    pub fn blockrazor(mut self, auth_token: &str) -> Self {
        if !self.endpoints.iter().any(|e| e.name == "BlockRazor") {
            self.endpoints.push(RpcEndpoint {
                name: "BlockRazor".to_string(),
                url: "https://rpc.blockrazor.builders".to_string(),
                auth_header: Some(auth_token.to_string()),
                priority: 1,
                is_builder: true,
                method: RpcMethod::EthSendPrivateTransaction,
            });
        }
        self
    }

    /// Add custom endpoint with specified method
    pub fn custom(
        mut self, 
        name: &str, 
        url: &str, 
        auth: Option<&str>, 
        is_builder: bool,
        method: RpcMethod,
    ) -> Self {
        self.endpoints.push(RpcEndpoint {
            name: name.to_string(),
            url: url.to_string(),
            auth_header: auth.map(|s| s.to_string()),
            priority: if is_builder { 5 } else { 15 },
            is_builder,
            method,
        });
        self
    }

    /// Build the broadcaster
    pub fn build(self) -> TxBroadcaster {
        TxBroadcaster::with_endpoints(self.endpoints)
    }
}

impl Default for BroadcasterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// CONVENIENCE FUNCTION

/// Create a fully configured broadcaster from environment variables
///
/// Reads:
/// - BLOXROUTE_AUTH: bloXroute authorization header
/// - ANKR_API_KEY: Ankr API key
/// - CHAINSTACK_API_KEY: Chainstack API key
pub fn broadcaster_from_env() -> TxBroadcaster {
    BroadcasterBuilder::new().build()
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_endpoints() {
        let endpoints = configured_default_endpoints(|_| None);
        assert_eq!(endpoints.len(), 2);
        assert!(endpoints.iter().all(|e| e.auth_header.is_none()));
        assert!(endpoints.iter().any(|e| e.name == "48Club"));
        assert!(endpoints.iter().all(|e| !["BlockRazor", "bloXroute", "Ankr", "Chainstack"].contains(&e.name.as_str())));
    }

    #[test]
    fn configured_credentials_are_optional_and_explicit() {
        let blank = configured_default_endpoints(|_| Some("  ".to_string()));
        assert_eq!(blank.len(), 2);
        let configured = configured_default_endpoints(|name| {
            (name == "BLOCKRAZOR_AUTH").then(|| "fixture_auth".to_string())
        });
        assert_eq!(configured.len(), 3);
        let endpoint = configured.iter().find(|e| e.name == "BlockRazor").unwrap();
        assert_eq!(endpoint.auth_header.as_deref(), Some("fixture_auth"));
    }

    #[test]
    fn test_builder_pattern() {
        let broadcaster = BroadcasterBuilder::new()
            .bloxroute("test_auth")
            .ankr("test_key")
            .build();

        assert!(broadcaster.endpoints().len() >= 4);
        assert!(broadcaster.endpoints().iter().any(|e| e.name == "bloXroute"));
        assert!(broadcaster.endpoints().iter().any(|e| e.name == "Ankr"));
    }
}


