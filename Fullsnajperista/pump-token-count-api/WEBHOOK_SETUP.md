# Helius Webhook Setup Guide

Ovaj vodič objašnjava kako da podesite Helius webhook za praćenje CREATE token transakcija na pump.fun.

## Šta je Helius Webhook?

Helius webhook omogućava da Helius automatski šalje podatke o transakcijama direktno na vaš server kada se dogode određene transakcije. Ovo je efikasnije od WebSocket-a jer ne zahteva održavanje stalne konekcije.

## Kako da Podesite Webhook

### 1. Priprema Servera

Server već ima webhook endpoint na:
```
POST /webhook
```

Endpoint prima Helius webhook payload i automatski:
- Detektuje CREATE token transakcije
- Ekstraktuje mint, creator, signature
- Zapisuje u Redis

### 2. Kreiranje Helius Webhook-a

#### Opcija A: Preko Helius Dashboard-a

1. Idite na [Helius Dashboard](https://dashboard.helius.dev/)
2. Prijavite se sa vašim API key-em
3. Idite na **Webhooks** sekciju
4. Kliknite **Create Webhook**
5. Popunite formu:
   - **Webhook URL**: `https://your-domain.com/webhook` (ili `http://localhost:3000/webhook` za lokalno testiranje)
   - **Transaction Types**: Izaberite **CREATE**
   - **Source**: Izaberite **PUMP_FUN**
   - **Webhook Type**: **Enhanced** (preporučeno) ili **Raw**
   - **Account Addresses**: Ostavite prazno (ili dodajte specifične adrese ako želite)
   - **Commitment Level**: **confirmed** ili **finalized**

#### Opcija B: Preko Helius API-ja

```bash
curl -X POST https://api.helius.xyz/v0/webhooks?api-key=YOUR_API_KEY \
  -H "Content-Type: application/json" \
  -d '{
    "webhookURL": "https://your-domain.com/webhook",
    "transactionTypes": ["CREATE"],
    "accountAddresses": [],
    "webhookType": "enhanced",
    "commitment": "confirmed"
  }'
```

### 3. Testiranje Webhook-a

#### Lokalno Testiranje (ngrok)

Za lokalno testiranje, koristite ngrok da eksponujete vaš lokalni server:

```bash
# Terminal 1: Pokrenite server
cd pump-token-count-api
cargo run

# Terminal 2: Pokrenite ngrok
ngrok http 3000
```

Kopirajte ngrok URL (npr. `https://abc123.ngrok.io`) i koristite ga kao webhook URL u Helius dashboard-u.

#### Test sa curl

```bash
curl -X POST http://localhost:3000/webhook \
  -H "Content-Type: application/json" \
  -d '{
    "webhook_id": "test-123",
    "timestamp": 1234567890,
    "data": [
      {
        "signature": "test-signature",
        "logs": ["Program log: Instruction: Create"],
        "accountKeys": [
          {"pubkey": "CREATOR_ADDRESS", "signer": true},
          {"pubkey": "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", "signer": false}
        ],
        "tokenTransfers": [
          {"mint": "MINT_ADDRESS"}
        ]
      }
    ]
  }'
```

### 4. Provera da Webhook Radi

#### Proverite Redis

```bash
# Proverite da li je creator count povećan
redis-cli GET pump:creates:CREATOR_ADDRESS

# Proverite da li je token sačuvan
redis-cli GET pump:tokens:MINT_ADDRESS

# Proverite recent tokens
redis-cli ZREVRANGE pump:recent_tokens 0 9
```

#### Proverite API Endpoint

```bash
# Get recent tokens
curl http://localhost:3000/recent?limit=10
```

## Koje Transakcije da Capture?

Za praćenje CREATE tokena na pump.fun, podesite webhook sa:

- **Transaction Types**: `CREATE`
- **Source**: `PUMP_FUN`
- **Program ID** (opciono): `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`

### Alternativno: Svi CREATE Transakcije

Ako želite da pratite SVE CREATE transakcije (ne samo pump.fun):

- **Transaction Types**: `CREATE`
- **Source**: Ostavite prazno ili izaberite `ALL`
- **Program ID**: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`

## Webhook Payload Format

Helius šalje webhook u sledećem formatu:

```json
{
  "webhook_id": "webhook-id-123",
  "timestamp": 1234567890,
  "data": [
    {
      "signature": "transaction-signature",
      "slot": 123456,
      "accountKeys": [
        {"pubkey": "address", "signer": true},
        ...
      ],
      "logs": [
        "Program log: Instruction: Create",
        ...
      ],
      "tokenTransfers": [
        {
          "mint": "mint-address",
          ...
        }
      ],
      "instructions": [...],
      "transactionError": null
    }
  ]
}
```

## Redis Struktura

Webhook zapisuje podatke u Redis na sledeći način:

1. **Creator Count**: `pump:creates:{creator_address}` - Broj tokena po creatoru
2. **Token Data**: `pump:tokens:{mint_address}` - JSON sa punim podacima o tokenu
3. **Recent Tokens**: `pump:recent_tokens` - Sorted set sa recent tokenima (sorted by timestamp)

## API Endpoints

### POST /webhook
Prima Helius webhook payload i procesira CREATE transakcije.

**Response:**
```json
{
  "success": true,
  "processed": 1
}
```

### GET /recent?limit=N
Vraća N najnovijih CREATE tokena.

**Query Parameters:**
- `limit` (opciono): Broj tokena (default: 100, max: 1000)

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
    },
    ...
  ]
}
```

## Troubleshooting

### Webhook ne prima podatke

1. **Proverite da li je server dostupan**
   ```bash
   curl https://your-domain.com/health
   ```

2. **Proverite Helius webhook status**
   - Idite na Helius Dashboard → Webhooks
   - Proverite da li je webhook "Active"
   - Proverite "Last Triggered" timestamp

3. **Proverite server logs**
   ```bash
   # Server će logovati svaki primljeni webhook
   tail -f logs/server.log
   ```

### Transakcije se ne procesiraju

1. **Proverite da li su CREATE transakcije**
   - Webhook filtrira samo transakcije sa "Instruction: Create" u logs

2. **Proverite Redis konekciju**
   ```bash
   redis-cli ping
   ```

3. **Proverite server logs za greške**
   - Server će logovati greške pri procesiranju

### Duplikati u Redis-u

- Webhook automatski inkrementuje creator count, tako da duplikati nisu problem
- Token data se overwrite-uje ako isti token dođe ponovo (što je retko)

## Production Deployment

Za production:

1. **Koristite HTTPS** - Helius zahteva HTTPS za webhook URL
2. **Dodajte autentifikaciju** (opciono) - Možete dodati API key proveru
3. **Monitorujte performanse** - Koristite `/stats` endpoint
4. **Postavite rate limiting** - Zaštitite server od previše zahteva

## Poređenje: Webhook vs WebSocket

| Feature | Webhook | WebSocket |
|---------|---------|-----------|
| Konekcija | HTTP POST | Trajna konekcija |
| Latency | Niska | Veoma niska |
| Reliable | Da (retry) | Zavisi od konekcije |
| Setup | Jednostavniji | Komplikovaniji |
| Scaling | Lako | Teže |

**Preporuka**: Koristite webhook za production, WebSocket za development.

