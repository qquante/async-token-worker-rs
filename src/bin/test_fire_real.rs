//! Real TX Fire Test
//!
//! Tests the FIRE broadcaster with a REAL transaction on BSC.
//! Sends 0 BNB to self (only pays gas ~0.000021 BNB)
//!
//! Usage:
//!   export TEST_PRIVATE_KEY=0x...
//!   cargo run --bin test-fire-real --features cli

use anyhow::{anyhow, Result};
use ethers::prelude::*;
use ethers::types::transaction::eip2718::TypedTransaction;
use four_meme::broadcaster::TxBroadcaster;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;

// CONFIGURATION

const BSC_CHAIN_ID: u64 = 56;
const GAS_PRICE_GWEI: u64 = 1;
const GAS_LIMIT: u64 = 21000;
const READ_RPC: &str = "https://bsc-dataseed.binance.org";
const RECEIPT_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcResponse {
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// RPC HELPERS

async fn get_balance(client: &Client, address: &str) -> Result<f64> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getBalance",
        "params": [address, "latest"]
    });

    let response = client
        .post(READ_RPC)
        .json(&payload)
        .send()
        .await?
        .json::<JsonRpcResponse>()
        .await?;

    if let Some(result) = response.result {
        let hex = result.as_str().unwrap_or("0x0");
        let wei = U256::from_str_radix(hex.trim_start_matches("0x"), 16)?;
        let bnb = wei.as_u128() as f64 / 1e18;
        Ok(bnb)
    } else {
        Err(anyhow!("Failed to get balance"))
    }
}

async fn get_nonce(client: &Client, address: &str) -> Result<u64> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getTransactionCount",
        "params": [address, "latest"]
    });

    let response = client
        .post(READ_RPC)
        .json(&payload)
        .send()
        .await?
        .json::<JsonRpcResponse>()
        .await?;

    if let Some(result) = response.result {
        let hex = result.as_str().unwrap_or("0x0");
        let nonce = u64::from_str_radix(hex.trim_start_matches("0x"), 16)?;
        Ok(nonce)
    } else {
        Err(anyhow!("Failed to get nonce"))
    }
}

async fn wait_for_receipt(client: &Client, tx_hash: &str, timeout_secs: u64) -> Result<bool> {
    let start = Instant::now();
    
    loop {
        if start.elapsed().as_secs() > timeout_secs {
            return Err(anyhow!("Timeout waiting for receipt"));
        }

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getTransactionReceipt",
            "params": [tx_hash]
        });

        let response = client
            .post(READ_RPC)
            .json(&payload)
            .send()
            .await?
            .json::<JsonRpcResponse>()
            .await?;

        if let Some(result) = response.result {
            if !result.is_null() {
                if let Some(status) = result.get("status").and_then(|s| s.as_str()) {
                    return Ok(status == "0x1");
                }
                return Ok(true);
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// MAIN

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    println!();
    println!("================================================================================");
    println!("  FIRE BROADCASTER - REAL TX TEST");
    println!("================================================================================");
    println!();

    // Load wallet
    let private_key = std::env::var("TEST_PRIVATE_KEY")
        .map_err(|_| anyhow!("TEST_PRIVATE_KEY not set"))?;
    
    let wallet: LocalWallet = private_key
        .parse::<LocalWallet>()?
        .with_chain_id(BSC_CHAIN_ID);
    
    let address = format!("{:?}", wallet.address());

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    // Wallet info
    let balance = get_balance(&client, &address).await?;
    let nonce = get_nonce(&client, &address).await?;
    let estimated_gas = (GAS_PRICE_GWEI as f64 * GAS_LIMIT as f64) / 1e9;

    println!("  WALLET");
    println!("  -----------------------------------------------------------------------");
    println!("  Address:        {}", address);
    println!("  Balance:        {:.8} BNB", balance);
    println!("  Nonce:          {}", nonce);
    println!("  Est. Gas Cost:  {:.8} BNB ({} gwei x {})", estimated_gas, GAS_PRICE_GWEI, GAS_LIMIT);
    println!();

    if balance < estimated_gas {
        return Err(anyhow!("Insufficient balance. Need at least {:.8} BNB", estimated_gas));
    }

    // Build transaction
    let tx = TransactionRequest::new()
        .to(wallet.address())
        .value(U256::zero())
        .gas(GAS_LIMIT)
        .gas_price(U256::from(GAS_PRICE_GWEI) * U256::from(1_000_000_000u64))
        .nonce(nonce)
        .chain_id(BSC_CHAIN_ID);

    println!("  TRANSACTION");
    println!("  -----------------------------------------------------------------------");
    println!("  To:             {} (self)", address);
    println!("  Value:          0 BNB");
    println!("  Gas Limit:      {}", GAS_LIMIT);
    println!("  Gas Price:      {} gwei", GAS_PRICE_GWEI);
    println!("  Chain ID:       {}", BSC_CHAIN_ID);
    println!();

    // Sign
    let typed_tx: TypedTransaction = tx.into();
    let signature = wallet.sign_transaction(&typed_tx).await?;
    let signed_tx = typed_tx.rlp_signed(&signature);
    let signed_tx_hex = format!("0x{}", hex::encode(&signed_tx));

    println!("  SIGNED TX");
    println!("  -----------------------------------------------------------------------");
    println!("  Size:           {} bytes", signed_tx.len());
    println!("  Hash prefix:    {}...", &signed_tx_hex[..42]);
    println!();

    // Fire
    let broadcaster = TxBroadcaster::new();
    let endpoints = broadcaster.endpoints();

    println!("  FIRE BROADCAST");
    println!("  -----------------------------------------------------------------------");
    println!("  Endpoints:      {}", endpoints.len());
    for ep in endpoints {
        let method = match ep.method {
            four_meme::broadcaster::RpcMethod::EthSendRawTransaction => "eth_sendRawTransaction",
            four_meme::broadcaster::RpcMethod::EthSendPrivateTransaction => "eth_sendPrivateTransaction",
            four_meme::broadcaster::RpcMethod::BscPrivateTx => "bsc_private_tx",
            four_meme::broadcaster::RpcMethod::WebSocket => "ws_sendRawTransaction",
        };
        let tier = if ep.is_builder { "builder" } else { "public" };
        println!("                  - {:15} | {:26} | {}", ep.name, method, tier);
    }
    println!();

    let fire_start = Instant::now();
    
    match broadcaster.fire(&signed_tx_hex).await {
        Ok(result) => {
            let fire_time = fire_start.elapsed();

            println!("  FIRE RESULT");
            println!("  -----------------------------------------------------------------------");
            println!("  Status:         SUCCESS");
            println!("  TX Hash:        {}", result.tx_hash);
            println!("  First Response: {} ({} ms)", result.first_responder, result.first_success_ms);
            println!("  Total Time:     {} ms", fire_time.as_millis());
            println!("  Success Rate:   {}/{} endpoints", result.success_count, result.success_count + result.failure_count);
            println!();

            // Individual results
            println!("  ENDPOINT RESULTS");
            println!("  -----------------------------------------------------------------------");
            println!("  {:15} | {:>8} | {:8} | {}", "ENDPOINT", "LATENCY", "STATUS", "RESULT");
            println!("  -----------------------------------------------------------------------");
            
            for r in &result.results {
                let status = if r.success { "OK" } else { "FAILED" };
                let info = if let Some(hash) = &r.tx_hash {
                    format!("{}...{}", &hash[..10], &hash[hash.len()-6..])
                } else if let Some(err) = &r.error {
                    let msg = err.replace('\n', " ");
                    if msg.len() > 40 { format!("{}...", &msg[..40]) } else { msg }
                } else {
                    "-".to_string()
                };
                println!("  {:15} | {:>6} ms | {:8} | {}", r.endpoint_name, r.latency_ms, status, info);
            }
            println!();

            // Wait for confirmation
            println!("  CONFIRMATION");
            println!("  -----------------------------------------------------------------------");
            print!("  Waiting for block inclusion... ");
            
            let confirm_start = Instant::now();
            match wait_for_receipt(&client, &result.tx_hash, RECEIPT_TIMEOUT_SECS).await {
                Ok(success) => {
                    let confirm_time = confirm_start.elapsed();
                    if success {
                        println!("CONFIRMED ({} ms)", confirm_time.as_millis());
                    } else {
                        println!("REVERTED ({} ms)", confirm_time.as_millis());
                    }
                }
                Err(e) => {
                    println!("TIMEOUT ({})", e);
                }
            }

            // Final balance
            let new_balance = get_balance(&client, &address).await?;
            let gas_used = balance - new_balance;
            
            println!();
            println!("  FINAL STATE");
            println!("  -----------------------------------------------------------------------");
            println!("  New Balance:    {:.8} BNB", new_balance);
            println!("  Gas Used:       {:.8} BNB", gas_used);
            println!("  BSCScan:        https://bscscan.com/tx/{}", result.tx_hash);
            println!();

            // Summary
            println!("================================================================================");
            println!("  BENCHMARK SUMMARY");
            println!("================================================================================");
            println!("  Fire Latency:       {} ms (first response)", result.first_success_ms);
            println!("  Total Fire Time:    {} ms (all endpoints)", fire_time.as_millis());
            println!("  Confirmation Time:  {} ms", confirm_start.elapsed().as_millis());
            println!("  Success Rate:       {:.0}%", (result.success_count as f64 / (result.success_count + result.failure_count) as f64) * 100.0);
            println!("================================================================================");
            println!();
        }
        Err(e) => {
            println!("  FIRE RESULT");
            println!("  -----------------------------------------------------------------------");
            println!("  Status:         FAILED");
            println!("  Error:          {}", e);
            println!();
            return Err(anyhow!("Fire failed: {}", e));
        }
    }

    Ok(())
}
