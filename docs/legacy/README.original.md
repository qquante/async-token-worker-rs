> Historical project notes. Paths, operational status, badges and test claims below are retained as archival reference and have not all been revalidated. Use the root README and docs/REVIEW.md for this review snapshot.

# 🦀 Four.Meme Rust Worker

High-performance token creator for Four.Meme on BSC, written in Rust.

## ✨ Features

- **Parallel Processing**: Multiple tokens created simultaneously via `tokio::spawn`
- **Redis Pub/Sub**: Listens to deployment channels, publishes results
- **Semaphore Limiter**: Controls max concurrent creations (default: 5)
- **100% Compatible**: Same API flow as JavaScript version
- **Docker Ready**: Multi-stage build for minimal image size

## 📦 Channels

| Channel | Description |
|---------|-------------|
| `token:deploy:en` | English token deployment requests |
| `token:deploy:zh` | Chinese token deployment requests |
| `cz:tweets:new` | CZ tweets from capture worker |
| `cz:test:tweets:new` | Test channel for CZ tweets |
| `token:created` | Published when token is created |

## 🚀 Usage

### Docker (Recommended)

```bash
# Build
docker build -t four-meme-worker .

# Run EN worker
docker run -e PRIVATE_KEY=your_key \
           -e REDIS_URL=redis://host:6379 \
           -e LANG=EN \
           four-meme-worker

# Run ZH worker  
docker run -e PRIVATE_KEY=your_key \
           -e REDIS_URL=redis://host:6379 \
           -e LANG=ZH \
           four-meme-worker
```

### Docker Compose

```bash
# Start Rust workers (profile: rust)
docker-compose --profile rust up -d rust-token-creator-en rust-token-creator-zh
```

### Local (requires Rust toolchain)

```bash
# Linux/Mac
cargo run --bin four-meme-worker --features worker

# With environment
PRIVATE_KEY=xxx REDIS_URL=redis://localhost:6379 cargo run --bin four-meme-worker --features worker
```

## ⚙️ Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PRIVATE_KEY` | *required* | Wallet private key |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection URL |
| `BSC_RPC_URL` | `https://bsc-dataseed.binance.org` | BSC RPC endpoint |
| `PRESALE_MIN` | `0.19` | Minimum presale BNB |
| `PRESALE_MAX` | `0.20` | Maximum presale BNB |
| `LABEL` | `Meme` | Token label |
| `LANG` | `EN` | Worker language (EN/ZH) |
| `MAX_CONCURRENT` | `5` | Max parallel token creations |
| `RUST_LOG` | `info` | Log level |

## 📊 Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    REDIS PUB/SUB                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   token:deploy:en ──┐                                          │
│   token:deploy:zh ──┼──► RUST WORKER ──► token:created         │
│   cz:tweets:new   ──┤      │                                   │
│   cz:test:tweets  ──┘      │                                   │
│                            ▼                                    │
│                   ┌────────────────┐                           │
│                   │  PARALLEL      │                           │
│                   │  PROCESSING    │                           │
│                   │                │                           │
│                   │ ┌────┐ ┌────┐  │                           │
│                   │ │ T1 │ │ T2 │  │  (tokio::spawn)           │
│                   │ └────┘ └────┘  │                           │
│                   │ ┌────┐ ┌────┐  │                           │
│                   │ │ T3 │ │ T4 │  │                           │
│                   │ └────┘ └────┘  │                           │
│                   │    ▼           │                           │
│                   │ SEMAPHORE(5)   │  (limits concurrency)     │
│                   └────────────────┘                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 📨 Message Format

### Input (token:deploy:en/zh)

```json
{
  "ticker": "PEPE",
  "name": "Pepe Token",
  "description": "A meme token",
  "imageBase64": "base64_encoded_image",
  "tweetUrl": "https://twitter.com/...",
  "source": "cz-book-watcher",
  "arrivalTime": 1702000000000
}
```

### Output (token:created)

```json
{
  "lang": "EN",
  "ticker": "PEPE",
  "name": "Pepe Token",
  "tokenAddress": "0x...",
  "txHash": "0x...",
  "links": {
    "bscscan": "https://bscscan.com/token/0x...",
    "fourMeme": "https://four.meme/token/0x...",
    "pancakeswap": "https://pancakeswap.finance/swap?outputCurrency=0x...",
    "tx": "https://bscscan.com/tx/0x..."
  }
}
```

## 🧪 Testing

```bash
# Publish test message to Redis
redis-cli PUBLISH token:deploy:en '{"ticker":"TEST","name":"Test Token","imageBase64":"..."}'

# Watch results
redis-cli SUBSCRIBE token:created
```

## 🏗️ Architecture

```
rust-four-meme/
├── Cargo.toml              # Dependencies
├── Dockerfile              # Multi-stage build
├── src/
│   ├── lib.rs              # Main library
│   ├── api.rs              # Four.Meme API client
│   ├── blockchain.rs       # BSC/Ethers interaction
│   ├── error.rs            # Error types
│   ├── redis_client.rs     # Redis pub/sub
│   ├── types.rs            # Data structures
│   └── bin/
│       ├── cli.rs          # CLI tool
│       └── worker.rs       # Redis worker
```

## ⚡ Performance

| Metric | Node.js | Rust |
|--------|---------|------|
| Startup | ~500ms | ~50ms |
| Memory | ~100MB | ~10MB |
| Parallel | Limited | Native |
| CPU | Single-thread | Multi-core |

## 🔒 Security

- Private keys never logged
- Wallet addresses checksummed
- TLS for all HTTP requests
- No external storage of keys

## 📝 License

MIT

