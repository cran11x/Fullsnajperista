# Quick Start Guide

## 1. Setup Environment Variables

Copy `.env.example` to `.env` and fill in your credentials:

```bash
cp .env.example .env
# Edit .env with your actual values
```

Required:
- `HELIUS_API_KEY` - Get from https://helius.dev
- `APIFY_DATASET_ID` - Get from https://apify.com/louisdeconinck/pump-fun-crypto-coin-scraper

Optional:
- `APIFY_API_KEY` - Only needed if dataset is private
- `PORT` - Default: 3000
- `REDIS_URL` - Default: redis://localhost:6379

## 2. Start Redis

### Option A: Docker Compose (Recommended)
```bash
docker-compose up -d redis
```

### Option B: Local Redis
```bash
# Install Redis and start it
redis-server
```

## 3. Build and Run

### Development
```bash
cargo run
```

### Production
```bash
cargo build --release
./target/release/pump-token-count-api
```

### Docker
```bash
docker-compose up -d
```

## 4. Test the API

```bash
# Test count endpoint (replace with real creator address)
curl http://localhost:3000/count/uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1

# Test health endpoint
curl http://localhost:3000/health

# Test stats endpoint
curl http://localhost:3000/stats
```

## 5. Monitor Logs

```bash
# Docker logs
docker-compose logs -f api

# Or if running locally, logs will appear in terminal
```

## Expected Behavior

1. **On startup:**
   - Connects to Redis
   - Connects to Helius WebSocket
   - Downloads Apify dataset (first time)
   - Starts listening for CREATE events

2. **WebSocket:**
   - Automatically reconnects on disconnect
   - Increments creator count in Redis on each CREATE event
   - Updates health check timestamp

3. **Refresh Task:**
   - Runs every 3 hours
   - Downloads latest Apify dataset
   - Bulk loads all creator counts to Redis

4. **API Endpoints:**
   - `/count/:address` - Returns count from Redis (< 15ms) or fallback API (< 3s)
   - `/health` - Shows WebSocket status and last update time
   - `/stats` - Shows Redis statistics

## Troubleshooting

### WebSocket not connecting
- Check `HELIUS_API_KEY` is valid
- Check Helius free tier limits
- Check logs: `docker-compose logs api`

### Redis connection failed
- Ensure Redis is running: `redis-cli ping`
- Check `REDIS_URL` in `.env`

### Apify refresh failing
- Check `APIFY_DATASET_ID` is correct
- If dataset is private, set `APIFY_API_KEY`
- Check Apify API limits

### Slow responses
- First request after startup may be slow (fallback API)
- Subsequent requests should be < 15ms (Redis cache)
- Check Redis is running and accessible

