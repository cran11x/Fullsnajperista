# Bulk Load Guide

Kako da učitam bulk podatke u Redis pre nego što server krene.

## Formati podataka

### Format 1: JSON sa creator i count
```json
[
  {
    "creator": "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1",
    "count": 28
  },
  {
    "creator": "9taecBUD4uqAbvB5bsob3wN1b946J7ZhgGr1jYVnpump",
    "count": 5
  }
]
```

### Format 2: JSON sa tokenima (grupiše po creator_address)
```json
[
  {
    "creator_address": "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1",
    "mint": "...",
    "name": "..."
  },
  {
    "creator_address": "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1",
    "mint": "...",
    "name": "..."
  }
]
```
Automatski će grupisati po `creator_address` i izbrojati.

### Format 3: CSV
```csv
creator,count
uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1,28
9taecBUD4uqAbvB5bsob3wN1b946J7ZhgGr1jYVnpump,5
```

## Kako koristiti

### 1. Pripremi podatke

Kreiraj fajl sa podacima (JSON ili CSV format).

### 2. Pokreni bulk loader

```bash
# JSON format (auto-detect)
cargo run --bin bulk_load data.json

# CSV format
cargo run --bin bulk_load data.csv --format csv

# Ili eksplicitno JSON
cargo run --bin bulk_load data.json --format json
```

### 3. Proveri rezultat

```bash
# Proveri u Redis
redis-cli GET pump:creates:uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1

# Ili preko API
curl http://localhost:3000/count/uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1
```

## Primeri

Pogledaj `examples/` folder za primer fajlove:
- `bulk_data_example.json` - JSON format
- `bulk_data_example.csv` - CSV format

## Napomene

- Svi podaci se učitavaju sa TTL od 30 dana
- Bulk write koristi pipeline za brzinu (1000 zapisa po batch-u)
- Ako creator već postoji u Redis-u, vrednost će biti zamenjena novom
- Format se automatski detektuje po ekstenziji (.json ili .csv)

## Performance

- ~1000 creators/sekunda
- Za 100K creators: ~100 sekundi
- Za 1M creators: ~17 minuta

