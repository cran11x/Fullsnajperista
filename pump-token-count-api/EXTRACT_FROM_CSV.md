# Extract Creators from Transaction CSV

Kako da ekstraktuješ creator counts iz pump.fun transaction CSV fajla.

## CSV Format

Tvoj CSV ima kolone:
```
block_time,slot,tx_idx,signing_wallet,direction,base_coin,base_coin_amount,...
```

Gde je `base_coin` mint adresa tokena.

## Kako koristiti

### 1. Pripremi CSV fajl

Tvoj fajl sa transakcijama (npr. `transactions.csv`)

### 2. Pokreni ekstrakciju

```bash
# Output kao JSON (default)
cargo run --bin extract_creators_from_tx_csv transactions.csv

# Output kao CSV
cargo run --bin extract_creators_from_tx_csv transactions.csv --output csv

# Direktno u Redis
cargo run --bin extract_creators_from_tx_csv transactions.csv --output redis
```

### 3. Rezultat

**JSON format** (`transactions.creators.json`):
```json
[
  {"creator": "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1", "count": 28},
  {"creator": "9taecBUD4uqAbvB5bsob3wN1b946J7ZhgGr1jYVnpump", "count": 5}
]
```

**CSV format** (`transactions.creators.csv`):
```csv
creator,count
uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1,28
9taecBUD4uqAbvB5bsob3wN1b946J7ZhgGr1jYVnpump,5
```

## Kako radi

1. **Parsira CSV** - čita sve linije i ekstraktuje unique `base_coin` (mint addresses)
2. **Dobija creatore** - za svaki mint, koristi DAS API da dobije creator address
3. **Grupiše i broji** - grupiše po creator i broji koliko tokena je svaki creator napravio
4. **Output** - piše rezultat u JSON/CSV ili direktno u Redis

## Performance

- **Parsiranje CSV**: ~10K linija/sekunda
- **DAS API pozivi**: ~20 poziva/sekunda (sa rate limiting delay)
- **Za 100K transakcija**: ~5-10 minuta (zavisi od broja unique mints)

## Napomene

- Zahteva `HELIUS_API_KEY` u environment varijablama
- Koristi DAS API `getAsset` metodu da dobije creator za svaki mint
- Preferira verified creator, ali koristi bilo koji creator ako verified nije dostupan
- Rate limiting: 50ms delay između API poziva
- Ako mint nema creator u DAS API-ju, preskače se

## Primer

```bash
# Set environment
export HELIUS_API_KEY=your_key_here

# Extract to JSON
cargo run --bin extract_creators_from_tx_csv huge_file.csv

# Then load to Redis
cargo run --bin bulk_load huge_file.creators.json
```

