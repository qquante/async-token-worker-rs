//! Four.Meme CLI Tool

use anyhow::Result;
use clap::{Parser, Subcommand};
use four_meme::{FourMeme, TokenConfig, VALID_LABELS};
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "four-meme")]
#[command(about = "Four.Meme Token Creator CLI", long_about = None)]
struct Cli {
    /// Wallet private key (without 0x prefix)
    #[arg(short = 'k', long, env = "PRIVATE_KEY")]
    private_key: String,

    /// BSC RPC URL
    #[arg(short = 'r', long, default_value = "https://bsc-dataseed.binance.org", env = "BSC_RPC_URL")]
    rpc_url: String,

    /// Gas price in Gwei (higher = faster confirmation)
    #[arg(short = 'g', long, default_value = "5", env = "FOUR_MEME_GAS_GWEI")]
    gas_gwei: u64,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new token
    Create {
        /// Token ticker/symbol (e.g., PEPE)
        #[arg(short = 't', long)]
        ticker: String,

        /// Token name (e.g., Pepe Token)
        #[arg(short = 'n', long)]
        name: String,

        /// Token description
        #[arg(short = 'd', long)]
        description: Option<String>,

        /// Path to logo image (PNG/JPG)
        #[arg(short = 'i', long)]
        image: PathBuf,

        /// Token label (Meme, AI, Defi, Games, Infra, De-Sci, Social, Depin, Charity, Others)
        #[arg(short = 'l', long, default_value = "Meme")]
        label: String,

        /// Presale amount in BNB (creator's initial buy)
        #[arg(short = 'p', long, default_value = "0")]
        presale: f64,

        /// Twitter URL
        #[arg(long)]
        twitter: Option<String>,

        /// Telegram URL
        #[arg(long)]
        telegram: Option<String>,

        /// Website URL
        #[arg(long)]
        website: Option<String>,
    },

    /// Buy tokens on bonding curve
    Buy {
        /// Token contract address
        #[arg(short = 'a', long)]
        address: String,

        /// Amount of BNB to spend
        #[arg(short = 'm', long)]
        amount: f64,
    },

    /// Check wallet balance
    Balance,

    /// List valid labels
    Labels,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Create client
    let client = FourMeme::new(&cli.private_key, &cli.rpc_url)?;

    match cli.command {
        Commands::Create {
            ticker,
            name,
            description,
            image,
            label,
            presale,
            twitter,
            telegram,
            website,
        } => {
            // Read image file
            let image_data = std::fs::read(&image).map_err(|e| {
                error!("Failed to read image file: {}", e);
                e
            })?;

            info!("Image loaded: {} bytes", image_data.len());

            let config = TokenConfig {
                ticker,
                name,
                description,
                image: image_data,
                pre_uploaded_image_url: None,
                label,
                presale_bnb: presale,
                twitter_url: twitter,
                telegram_url: telegram,
                website_url: website,
            };

            match client.create_token(config).await {
                Ok(result) => {
                    println!("\n✅ TOKEN CREATED SUCCESSFULLY!\n");
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    println!("📍 Address: {}", result.token_address);
                    println!("📝 TX Hash: {}", result.tx_hash);
                    println!("🔢 Block:   {}", result.block_number);
                    println!("⛽ Gas:     {}", result.gas_used);
                    println!("⏱️  Time:    {}ms", result.creation_time_ms);
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
                    println!("🔗 Links:");
                    println!("   Four.Meme:   {}", result.links.four_meme);
                    println!("   PancakeSwap: {}", result.links.pancakeswap);
                    println!("   BSCScan:     {}", result.links.bscscan);
                    println!("   Transaction: {}", result.links.tx);
                    println!();
                }
                Err(e) => {
                    error!("Failed to create token: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Buy { address, amount } => {
            let token_address: ethers::types::Address = address
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid address: {}", e))?;

            match client.buy_token(token_address, amount).await {
                Ok(result) => {
                    println!("\n✅ BUY SUCCESSFUL!\n");
                    println!("TX Hash: {}", result.tx_hash);
                    println!("Gas:     {}", result.gas_used);
                }
                Err(e) => {
                    error!("Failed to buy: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Balance => {
            let balance = client
                .wallet_address();
            
            println!("\n💰 Wallet Balance\n");
            println!("Address: {:?}", balance);
            
            // Get balance from blockchain
            let blockchain = four_meme::Blockchain::new(&cli.private_key, &cli.rpc_url, cli.gas_gwei)?;
            let balance_wei = blockchain.get_balance().await?;
            let balance_bnb = ethers::utils::format_ether(balance_wei);
            println!("Balance: {} BNB", balance_bnb);
        }

        Commands::Labels => {
            println!("\n📋 Valid Token Labels:\n");
            for label in VALID_LABELS {
                println!("  • {}", label);
            }
            println!();
        }
    }

    Ok(())
}

