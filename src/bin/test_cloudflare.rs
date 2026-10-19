//! Test Cloudflare bypass with different strategies
//!
//! Run with:
//!   cargo run --bin test-cloudflare --features cli
//!
//! With proxy:
//!   HTTPS_PROXY=http://user:pass@ip:port cargo run --bin test-cloudflare --features cli
//!
//! This test compares:
//! 1. Regular reqwest (no headers) - should get Cloudflare challenge
//! 2. Reqwest with browser headers - might work
//! 3. Reqwest with browser headers + proxy - should work

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, USER_AGENT};
use std::time::Duration;

const API_URL: &str = "https://four.meme/meme-api/v1/private/user/nonce/generate";
const TEST_ADDRESS: &str = "0x742d35Cc6634C0532925a3b844Bc9e7595f5bE91";

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    println!("===========================================");
    println!("   CLOUDFLARE BYPASS TEST");
    println!("===========================================\n");

    let proxy_url = std::env::var("HTTPS_PROXY").ok();
    if let Some(ref p) = proxy_url {
        println!("Proxy configured: {}\n", p.replace(|c: char| c.is_ascii_alphanumeric() && c != '@' && c != ':' && c != '.', "*"));
    } else {
        println!("No proxy configured (set HTTPS_PROXY env var)\n");
    }

    // Test 1: Regular reqwest (expected: Cloudflare block)
    println!("TEST 1: Regular reqwest (no headers)");
    println!("-------------------------------------------");
    test_basic().await;
    println!();

    // Test 2: Reqwest with browser headers
    println!("TEST 2: Reqwest with browser headers");
    println!("-------------------------------------------");
    test_with_headers(None).await;
    println!();

    // Test 3: Reqwest with browser headers + proxy
    if let Some(ref proxy) = proxy_url {
        println!("TEST 3: Reqwest with browser headers + PROXY");
        println!("-------------------------------------------");
        test_with_headers(Some(proxy)).await;
        println!();
    }

    println!("===========================================");
    println!("   TEST COMPLETE");
    println!("===========================================");
}

fn build_browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
    ));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("Origin", HeaderValue::from_static("https://four.meme"));
    headers.insert("Referer", HeaderValue::from_static("https://four.meme/"));
    headers.insert("sec-ch-ua", HeaderValue::from_static("\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""));
    headers.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
    headers.insert("sec-ch-ua-platform", HeaderValue::from_static("\"Windows\""));
    headers.insert("sec-fetch-dest", HeaderValue::from_static("empty"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    headers
}

async fn test_basic() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to create client");

    let payload = serde_json::json!({
        "accountAddress": TEST_ADDRESS,
        "verifyType": "LOGIN",
        "networkCode": "BSC"
    });

    make_request(&client, &payload).await;
}

async fn test_with_headers(proxy_url: Option<&String>) {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .default_headers(build_browser_headers())
        .cookie_store(true);

    if let Some(proxy) = proxy_url {
        match reqwest::Proxy::all(proxy) {
            Ok(p) => {
                builder = builder.proxy(p);
                println!("Using proxy: {}", proxy.split('@').last().unwrap_or(proxy));
            }
            Err(e) => {
                println!("Failed to configure proxy: {}", e);
                return;
            }
        }
    }

    let client = builder.build().expect("Failed to create client");

    let payload = serde_json::json!({
        "accountAddress": TEST_ADDRESS,
        "verifyType": "LOGIN",
        "networkCode": "BSC"
    });

    make_request(&client, &payload).await;
}

async fn make_request(client: &reqwest::Client, payload: &serde_json::Value) {
    match client.post(API_URL).json(payload).send().await {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            println!("Status: {}", status);
            println!("Body (first 400 chars):\n{}", &text[..text.len().min(400)]);

            if text.contains("Just a moment") || text.contains("<!DOCTYPE html>") {
                println!("\n❌ BLOCKED: Cloudflare challenge detected!");
            } else if text.contains("\"code\":0") || text.contains("\"code\":\"0\"") {
                println!("\n✅ SUCCESS: Got valid JSON response!");

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(nonce) = json.get("data").and_then(|d| d.as_str()) {
                        println!("📋 Nonce: {}", nonce);
                    }
                }
            } else {
                println!("\n⚠️  UNKNOWN: Not Cloudflare, not expected JSON");
                println!("Full body: {}", text);
            }
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}
