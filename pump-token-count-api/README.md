# Pump Token Count API

Ultra-fast Rust API server that provides sub-15ms token count lookups for pump.fun creators using Redis caching, with real-time WebSocket updates and periodic dataset refresh.

## Features

- **< 15ms response time** for Redis cache hits (99.999% of requests)
- **Real-time updates** via Helius WebSocket listener
- **Webhook support** for Helius webhook integration (see [WEBHOOK_SETUP.md](./WEBHOOK_SETUP.md))
- **Daily refresh** from Apify dataset (every 3 hours)
- **Fallback API** when Redis miss (Enhanced Transactions API → DAS API)
- **Health monitoring** with WebSocket connection status
- **TTL on Redis keys** (30 days) to prevent memory bloat

## Quick Start

### Prerequisites

- Rust 1.75+
- Redis server
- Helius API key
- Apify dataset ID

### Environment Variables

Copy `.env.example` to `.env` and fill in:

```bash
HELIUS_API_KEY=your_helius_api_key
REDIS_URL=redis://localhost:6379
APIFY_DATASET_ID=your_apify_dataset_id
APIFY_API_KEY=your_apify_api_key  # Optional
PORT=3000
```

### Run with Docker Compose

```bash
docker-compose up -d
```

### Run Locally

```bash
cargo run --release
```

## API Endpoints

### GET /count/:address

Returns the number of pump.fun tokens created by the given creator address.

**Example:**
```bash
curl http://localhost:3000/count/uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1
# Returns: 28
```

**Response:** Plain text number (e.g., `28`)

### GET /health

Returns WebSocket connection status and last update timestamp.

**Example:**
```bash
curl http://localhost:3000/health
```

**Response:**
```json
{
  "ws_connected": true,
  "last_update_seconds_ago": 23
}
```

### GET /stats

Returns Redis statistics (key count, memory usage).

**Example:**
```bash
curl http://localhost:3000/stats
```

**Response:**
```json
{
  "redis_keys": 12345,
  "memory_usage": "2.5M"
}
```

### POST /webhook

Receives Helius webhook payload and processes CREATE token transactions. See [WEBHOOK_SETUP.md](./WEBHOOK_SETUP.md) for detailed setup instructions.

**Example:**
```bash
curl -X POST http://localhost:3000/webhook \
  -H "Content-Type: application/json" \
  -d @webhook_payload.json
```

**Response:**
```json
{
  "success": true,
  "processed": 1
}
```

### GET /recent?limit=N

Returns N most recent CREATE tokens (default: 100, max: 1000).

**Example:**
```bash
curl http://localhost:3000/recent?limit=10
```

**Response:**
```json
{
  "success": true,
  "count": 10,
  "tokens": [
    {
      "mint": "mint-address",
      "creator": "creator-address",
      "signature": "tx-signature",
      "timestamp": 1234567890,
      "slot": 123456
    }
  ]
}
```

## Architecture

### Components

1. **Axum Web Server** - Handles HTTP requests
2. **Redis Cache** - Stores creator token counts with 30-day TTL
3. **WebSocket Listener** - Real-time CREATE event tracking from Helius
4. **Webhook Handler** - Receives Helius webhook payloads for CREATE transactions
5. **Refresh Task** - Downloads Apify dataset every 3 hours and bulk loads to Redis
6. **Fallback API** - Enhanced Transactions API → DAS API when Redis miss

### Critical Fixes Implemented

1. **Creator Parsing** - Correctly extracts creator from transaction (not always `account_keys[0]`)
2. **Refresh Interval** - Every 3 hours (not 6) to catch new spammers faster
3. **Fallback Order** - Enhanced Transactions API first (more reliable), then DAS
4. **TTL on Keys** - 30-day expiration prevents memory bloat
5. **Health Check** - Monitors WebSocket connection status

## Performance

- **Redis hits**: < 15ms (99.999% of requests)
- **Fallback API**: < 3 seconds (rare, only on cache miss)
- **Bulk refresh**: < 8 minutes for 2M+ tokens

## Development

```bash
# Check code
cargo check

# Run tests
cargo test

# Build release
cargo build --release
```

## License

MIT

