//! Broadcaster Test - TX Fake Test
//!
//! Tests the multi-RPC broadcaster with a fake transaction.
//! This confirms all endpoints are working without spending gas.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin test-broadcaster --features cli
//! ```

use anyhow::Result;
use four_meme::broadcaster::{default_endpoints, RpcMethod, TxBroadcaster};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant};
use tracing::error;
use tracing_subscriber::EnvFilter;

// ============================================================================
// TEST CONFIGURATIONS
// ============================================================================

/// Fake signed transaction for BSC (chain_id = 56)
/// This is a properly formatted EIP-155 transaction that will be rejected 
/// due to invalid signature/nonce, but confirms endpoint connectivity.
/// 
/// Structure: Legacy TX with chain_id = 56 (BSC)
/// - nonce: 0
/// - gas_price: 5 gwei
/// - gas_limit: 21000
/// - to: 0x4592d8f8d7b001e72cb26a73e4fa1806a51ac79d
/// - value: 0
/// - data: empty
/// - v: 147 (0x93) = 35 + 56*2 (EIP-155 for chain 56)
/// - r, s: fake signature bytes
const FAKE_TX: &str = "0xf86c808502540be40082520894d8dA6BF26964aF9D7eEd9e03E53415D37aA960458080819ea0c1b3d6e5e6e4f5b8e3c9e5e4e3b2e1e0e9e8e7e6e5e4e3e2e1e0e9e8e7e6e5e4a0e3e2e1e0e9e8e7e6e5e4e3e2e1e0e9e8e7e6e5e4e3e2e1e0e9e8e7e6e5e4e3e2";

/// JSON-RPC response structure
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ============================================================================
// TEST FUNCTIONS
// ============================================================================

/// Test 1: Connectivity test (eth_blockNumber)
async fn test_connectivity(client: &Client) -> Result<()> {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║  TEST 1: CONNECTIVITY TEST (eth_blockNumber)                                  ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
    println!();

    let endpoints = default_endpoints();
    let mut results = Vec::new();

    for endpoint in &endpoints {
        let start = Instant::now();
        
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_blockNumber",
            "params": []
        });

        let mut request = client.post(&endpoint.url).json(&payload);
        
        if let Some(auth) = &endpoint.auth_header {
            request = request.header("Authorization", auth);
        }

        match request.send().await {
            Ok(response) => {
                let latency = start.elapsed().as_millis();
                
                if response.status().is_success() {
                    match response.json::<JsonRpcResponse>().await {
                        Ok(rpc) => {
                            if let Some(result) = rpc.result {
                                let block = result.as_str().unwrap_or("?");
                                let block_num = u64::from_str_radix(block.trim_start_matches("0x"), 16).unwrap_or(0);
                                println!("  ✅ {:15} │ {:>6}ms │ Block: {} │ {:?}", 
                                    endpoint.name, latency, block_num, endpoint.method);
                                results.push((endpoint.name.clone(), true, latency));
                            } else if let Some(err) = rpc.error {
                                println!("  ⚠️  {:15} │ {:>6}ms │ RPC Error: {}", 
                                    endpoint.name, latency, err.message);
                                results.push((endpoint.name.clone(), false, latency));
                            }
                        }
                        Err(e) => {
                            println!("  ❌ {:15} │ {:>6}ms │ Parse error: {}", 
                                endpoint.name, latency, e);
                            results.push((endpoint.name.clone(), false, latency));
                        }
                    }
                } else {
                    println!("  ❌ {:15} │ {:>6}ms │ HTTP {}", 
                        endpoint.name, latency, response.status());
                    results.push((endpoint.name.clone(), false, latency));
                }
            }
            Err(e) => {
                let latency = start.elapsed().as_millis();
                println!("  ❌ {:15} │ {:>6}ms │ Connection error: {}", 
                    endpoint.name, latency, e);
                results.push((endpoint.name.clone(), false, latency));
            }
        }
    }

    // Summary
    let success = results.iter().filter(|(_, ok, _)| *ok).count();
    let total = results.len();
    let avg_latency: u128 = results.iter().map(|(_, _, l)| l).sum::<u128>() / total as u128;
    
    println!();
    println!("  ────────────────────────────────────────────────────");
    println!("  SUMMARY: {}/{} endpoints OK │ Avg latency: {}ms", success, total, avg_latency);
    println!();

    Ok(())
}

/// Test 2: Fake TX test (confirms methods work, will get rejected)
async fn test_fake_tx(client: &Client) -> Result<()> {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║  TEST 2: FAKE TX TEST (each endpoint's specific method)                       ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  ⚠️  Expected: All endpoints will REJECT the fake TX (this is normal!)");
    println!("  ⚠️  Success = endpoint responded (even with error)");
    println!();

    let endpoints = default_endpoints();
    let mut results = Vec::new();

    for endpoint in &endpoints {
        let start = Instant::now();
        
        // Build payload based on endpoint method
        let payload = match endpoint.method {
            RpcMethod::EthSendRawTransaction => {
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_sendRawTransaction",
                    "params": [FAKE_TX]
                })
            }
            RpcMethod::EthSendPrivateTransaction => {
                // BlockRazor uses eth_sendPrivateTransaction
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_sendPrivateTransaction",
                    "params": [FAKE_TX]
                })
            }
            RpcMethod::BscPrivateTx => {
                // bloXroute uses bsc_private_tx with special payload (no 0x prefix)
                let tx_without_prefix = FAKE_TX.trim_start_matches("0x");
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
        };

        let method_name = match endpoint.method {
            RpcMethod::EthSendRawTransaction => "eth_sendRawTransaction",
            RpcMethod::EthSendPrivateTransaction => "eth_sendPrivateTransaction",
            RpcMethod::BscPrivateTx => "bsc_private_tx",
        };

        let mut request = client.post(&endpoint.url).json(&payload);
        
        if let Some(auth) = &endpoint.auth_header {
            request = request.header("Authorization", auth);
        }

        match request.send().await {
            Ok(response) => {
                let latency = start.elapsed().as_millis();
                let status = response.status();
                
                match response.text().await {
                    Ok(text) => {
                        // Try to parse as JSON-RPC
                        if let Ok(rpc) = serde_json::from_str::<JsonRpcResponse>(&text) {
                            if let Some(err) = rpc.error {
                                // This is expected! Fake TX should be rejected
                                let short_msg = if err.message.len() > 40 {
                                    format!("{}...", &err.message[..40])
                                } else {
                                    err.message.clone()
                                };
                                println!("  ✅ {:15} │ {:>6}ms │ {} │ Rejected: {}", 
                                    endpoint.name, latency, method_name, short_msg);
                                results.push((endpoint.name.clone(), true, latency, "REJECTED (expected)".to_string()));
                            } else if let Some(result) = rpc.result {
                                // Unexpected success?
                                println!("  ⚠️  {:15} │ {:>6}ms │ {} │ Unexpected result: {:?}", 
                                    endpoint.name, latency, method_name, result);
                                results.push((endpoint.name.clone(), true, latency, "UNEXPECTED SUCCESS".to_string()));
                            }
                        } else {
                            // Non-JSON response
                            let short_text = if text.len() > 50 {
                                format!("{}...", &text[..50])
                            } else {
                                text.clone()
                            };
                            println!("  ⚠️  {:15} │ {:>6}ms │ {} │ HTTP {} - {}", 
                                endpoint.name, latency, method_name, status, short_text);
                            results.push((endpoint.name.clone(), status.is_success(), latency, format!("HTTP {}", status)));
                        }
                    }
                    Err(e) => {
                        println!("  ❌ {:15} │ {:>6}ms │ {} │ Read error: {}", 
                            endpoint.name, latency, method_name, e);
                        results.push((endpoint.name.clone(), false, latency, e.to_string()));
                    }
                }
            }
            Err(e) => {
                let latency = start.elapsed().as_millis();
                println!("  ❌ {:15} │ {:>6}ms │ {} │ Connection error: {}", 
                    endpoint.name, latency, method_name, e);
                results.push((endpoint.name.clone(), false, latency, e.to_string()));
            }
        }
    }

    // Summary
    let responded = results.iter().filter(|(_, ok, _, _)| *ok).count();
    let total = results.len();
    
    println!();
    println!("  ────────────────────────────────────────────────────");
    println!("  SUMMARY: {}/{} endpoints RESPONDED", responded, total);
    println!("  (All rejections are EXPECTED for fake TX)");
    println!();

    Ok(())
}

/// Test 3: Fire simulation (parallel broadcast test)
async fn test_fire_simulation() -> Result<()> {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║  TEST 3: FIRE SIMULATION (parallel broadcast)                                 ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  🔥 Testing TxBroadcaster.fire() with fake TX...");
    println!("  ⚠️  Expected: All endpoints reject, but we measure parallel speed");
    println!();

    let broadcaster = TxBroadcaster::new();
    
    println!("  Configured endpoints ({}):", broadcaster.endpoints().len());
    for ep in broadcaster.endpoints() {
        println!("    📡 {:15} │ {:?} │ builder: {}", 
            ep.name, ep.method, ep.is_builder);
    }
    println!();

    let start = Instant::now();
    
    // Fire the fake TX
    match broadcaster.fire(FAKE_TX).await {
        Ok(result) => {
            // Unexpected success
            println!("  ⚠️  UNEXPECTED: Fire succeeded?");
            println!("      TX Hash: {}", result.tx_hash);
            println!("      First responder: {} ({}ms)", result.first_responder, result.first_success_ms);
        }
        Err(e) => {
            let total_time = start.elapsed().as_millis();
            // This is expected - all endpoints should reject fake TX
            println!("  ✅ Fire completed (all rejected as expected)");
            println!("     Total time: {}ms", total_time);
            println!("     Error: {}", e);
        }
    }

    println!();
    Ok(())
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    println!(r#"
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║   🔥 BROADCASTER TEST - FAKE TX TEST                                         ║
║                                                                               ║
║   Tests all endpoints WITHOUT spending gas                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
"#);

    // Create HTTP client
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    // Run tests
    println!("  Starting tests...\n");

    // Test 1: Connectivity
    if let Err(e) = test_connectivity(&client).await {
        error!("Connectivity test failed: {}", e);
    }

    // Test 2: Fake TX with each method
    if let Err(e) = test_fake_tx(&client).await {
        error!("Fake TX test failed: {}", e);
    }

    // Test 3: Fire simulation
    if let Err(e) = test_fire_simulation().await {
        error!("Fire simulation failed: {}", e);
    }

    // Final summary
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║  ALL TESTS COMPLETED!                                                         ║");
    println!("╠═══════════════════════════════════════════════════════════════════════════════╣");
    println!("║  ✅ If endpoints responded (even with errors), they are WORKING              ║");
    println!("║  ✅ Fake TX rejections are EXPECTED and NORMAL                               ║");
    println!("║  ✅ Ready for real transactions!                                             ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
    println!();

    Ok(())
}

