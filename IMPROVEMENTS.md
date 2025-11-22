# Predložena Poboljšanja za Snajper Bot

## 🚀 Kritična Poboljšanja (Visok Prioritet)

### 1. **Paralelno Procesiranje Filtera**
**Problem:** DAS check, MC fetch, i socials check se rade sekvencijalno (~500-1000ms ukupno)
**Rešenje:** Pokrenuti sve paralelno sa `tokio::join!`
**Ušteda:** ~300-500ms po tokenu

```rust
// Trenutno (sekvencijalno):
let creator_count = check_creator_token_count_das(&accounts.creator).await?;
let (curve, _, mc_usd) = fetch_bonding_curve_mc(...).await?;
let socials = check_token_socials(&mint).await?;

// Poboljšano (paralelno):
let (creator_count, mc_data, socials) = tokio::join!(
    check_creator_token_count_das(&accounts.creator),
    fetch_bonding_curve_mc(...),
    check_token_socials(&mint)
);
```

### 2. **Shared HTTP Client Pool**
**Problem:** Svaki modul kreira svoj `reqwest::Client` (jito.rs, helius.rs, socials.rs, das_check.rs)
**Rešenje:** Globalni shared client sa connection pooling
**Ušteda:** Brže HTTP request-ove, manje memory overhead

```rust
// U utils.rs
lazy_static! {
    pub static ref HTTP_CLIENT: reqwest::Client = create_http_client(10).unwrap();
}
```

### 3. **Optimizacija Transaction Building**
**Problem:** TX se kreira pre nego što proverimo sve filtere
**Rešenje:** Build TX tek nakon što svi filteri prođu
**Ušteda:** Ne kreiramo TX za tokenove koji neće proći filtere

### 4. **Early Exit za Filtere**
**Problem:** Proveravamo sve filtere čak i kada prvi ne prođe
**Rešenje:** Early return nakon prvog failed filtera

## ⚡ Performance Poboljšanja (Srednji Prioritet)

### 5. **Caching Socials Metadata**
**Problem:** Isti token se može proveravati više puta
**Rešenje:** LRU cache za socials metadata
**Ušteda:** Brže za duplicate token checks

### 6. **Batch RPC Calls**
**Problem:** Više pojedinačnih RPC poziva
**Rešenje:** Batch RPC calls gde je moguće
**Ušteda:** Manje network overhead

### 7. **Metrics & Monitoring**
**Problem:** Nema tracking performansi
**Rešenje:** Dodati metrics za:
- Detection latency
- Filter pass rate
- TX submission success rate
- Average processing time

## 🛠️ Code Quality (Nizak Prioritet)

### 8. **Cleanup Warnings**
- Ukloniti unused imports
- Prefix unused variables sa `_`
- Dodati `#[allow(dead_code)]` gde je potrebno

### 9. **Better Error Messages**
**Problem:** Neki error messages nisu dovoljno detaljni
**Rešenje:** Dodati context u error messages

### 10. **Dokumentacija**
- Dodati rustdoc komentare
- Dodati primere korišćenja
- Dokumentovati API

## 📊 Dodatne Funkcionalnosti

### 11. **Dynamic Priority Fee**
**Problem:** Fiksni priority fee
**Rešenje:** Dinamički fee na osnovu network congestion

### 12. **Blacklist/Whitelist**
**Problem:** Nema načina da se blokiraju određeni tokeni/creatori
**Rešenje:** Dodati blacklist/whitelist support

### 13. **Rate Limiting**
**Problem:** Može da šalje previše request-ova
**Rešenje:** Rate limiting za RPC/API pozive

### 14. **Health Checks**
**Problem:** Nema načina da se proveri da li bot radi
**Rešenje:** Health check endpoint ili periodic status report

