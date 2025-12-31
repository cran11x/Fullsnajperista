# Testovi za optimizacije

Ovaj dokument opisuje testove koji verifikuju optimizacije implementirane u projektu.

## Struktura testova

### Unit testovi

#### `src/accounts/bonding_curve.rs`
- `test_bonding_curve_cache_basic` - Testira osnovnu funkcionalnost cache-a
- `test_bonding_curve_cache_with_custom_ttl` - Testira cache sa custom TTL
- `test_bonding_curve_cache_clear` - Testira brisanje cache-a
- `test_bonding_curve_cache_cleanup_expired` - Testira automatsko brisanje isteklih unosa

#### `src/bot_core.rs`
- `test_batch_check_token_balances_empty` - Testira batch balance check sa praznim inputom
- `test_batch_fetch_bonding_curves_empty` - Testira batch bonding curve fetch sa praznim inputom
- `test_batch_fetch_bonding_curves_batch_size` - Testira da se batch size limit poštuje
- `test_batch_check_token_balances_batch_size` - Testira batch size limit za token balances

### Integration testovi

#### `tests/optimization_tests.rs`
- `test_batch_operations_empty_input` - Testira prazan input
- `test_batch_size_limits` - Testira batch size limite (100 accounts per batch)
- `test_cache_ttl_logic` - Testira TTL logiku
- `test_retry_parameters` - Testira retry parametre (2 retries, 150ms sleep)
- `test_batch_operation_ordering` - Testira da se redosled čuva
- `test_batch_error_handling` - Testira error handling

## Pokretanje testova

### Svi testovi
```bash
cargo test
```

### Samo unit testovi
```bash
cargo test --lib
```

### Samo integration testovi
```bash
cargo test --test optimization_tests
```

### Ignorisanje testova koji zahtevaju RPC
```bash
cargo test -- --ignored
```

### Pokretanje ignorisanih testova
```bash
cargo test -- --include-ignored
```

## Testovi koji zahtevaju RPC konekciju

Neki testovi su označeni sa `#[ignore]` jer zahtevaju stvarnu RPC konekciju:
- `test_batch_check_token_balances_single` - Zahteva stvarni token account
- `test_batch_fetch_bonding_curves_single` - Zahteva stvarni bonding curve account

Ovi testovi se mogu pokrenuti sa:
```bash
cargo test -- --ignored
```

## Očekivani rezultati

### Batch operacije
- Prazan input treba da vrati prazan rezultat bez greške
- Batch size limit od 100 accounts po pozivu treba da se poštuje
- Redosled rezultata treba da odgovara redosledu inputa

### Cache
- Cache treba da čuva podatke 2 sekunde (default TTL)
- Expired entries treba da se automatski brišu
- Cache miss treba da triggeruje RPC fetch

### Retry logika
- Buy balance check treba da koristi 2 retry pokušaja
- Sleep između retry-ja treba da bude 150ms

## Debugging

Ako testovi ne prolaze:
1. Proverite da li su sve zavisnosti instalirane (`cargo build`)
2. Proverite da li su RPC endpoint-i dostupni (za integration testove)
3. Proverite logove za detalje grešaka

