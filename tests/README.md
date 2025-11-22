# Testovi za Fullsnajperista

Ova mapa sadrži testove za websocket konekciju, RPC konekciju i ostale funkcionalnosti.

## Pokretanje testova

### Svi testovi
```bash
cargo test
```

### Samo unit testovi (bez integration testova koji zahtijevaju vanjski server)
```bash
cargo test --lib
```

### WebSocket i RPC server testovi
```bash
cargo test --test websocket_server_test
```

### Mock buy testovi
```bash
cargo test --test mock_buy_test
```

## Integration testovi

### WebSocket Server Test (`websocket_server_test.rs`)

Testovi koji testiraju stvarnu konekciju na Helius WebSocket i RPC servere.

**Zahtjevi:**
- `HELIUS_API_KEY` mora biti postavljen u `.env` fajlu ili environment varijablama
- Mrežni pristup (internet konekcija)

**Testovi:**
1. `test_websocket_connection` - Testira WebSocket konekciju na Helius server
2. `test_rpc_connection` - Testira RPC konekciju na Helius server
3. `test_websocket_subscription_message` - Testira kreiranje subscription poruke
4. `test_websocket_message_parsing` - Testira parsiranje WebSocket poruka i detekciju novih tokena
5. `test_config_with_real_urls` - Testira konfiguraciju s realnim server URL-ovima
6. `test_health_check_real` - Testira health check s realnim konekcijama

**Primjer pokretanja:**
```bash
# Postavi HELIUS_API_KEY
export HELIUS_API_KEY="your-api-key-here"

# Pokreni testove
cargo test --test websocket_server_test -- --nocapture
```

## Unit testovi

### Mock Buy Test (`mock_buy_test.rs`)

Testovi koji testiraju mock buy funkcionalnost - ne zahtijevaju vanjski server.

### Integration Test (`integration.rs`)

Testovi su označeni s `#[ignore]` i zahtijevaju:
- Real RPC konekciju
- Valid wallet sa SOL
- Mrežni pristup

**Pokretanje ignoriranih testova:**
```bash
cargo test --test integration -- --ignored
```

## Debugging

Za više informacija o testovima, koristi `--nocapture` flag:
```bash
cargo test --test websocket_server_test -- --nocapture
```

## Napomene

- Integration testovi mogu biti spori zbog mrežnih zahtjeva
- Ako `HELIUS_API_KEY` nije postavljen, neki testovi će se jednostavno preskočiti (skip)
- Testovi s vanjskim serverima mogu vremenski neuspešno završiti ako server nije dostupan

