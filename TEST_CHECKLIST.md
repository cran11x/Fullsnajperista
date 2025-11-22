# Comprehensive Test Checklist

Ovaj dokument opisuje sve testove koji se pokreću i šta se testira.

## 🚀 Kako pokrenuti sve testove

### Brzo testiranje (preporučeno)
```powershell
.\test_all_comprehensive.ps1
```

### Sa detaljnim output-om
```powershell
.\test_all_comprehensive.ps1 -Verbose
```

### Bez integration testova (brže)
```powershell
.\test_all_comprehensive.ps1 -SkipIntegration
```

## 📋 Test Suites

### 1. Unit Tests (`--lib`)
**Šta se testira:**
- ✅ Config loading i validacija
- ✅ Target mint address loading
- ✅ DAS check response parsing
- ✅ Error handling
- ✅ Retry logic
- ✅ Rate limiting
- ✅ Utils funkcije
- ✅ Account tracking
- ✅ Metrics collection
- ✅ Health monitoring

**Broj testova:** 120+  
**Vreme:** ~1-2 sekunde  
**Zahteva:** Nema

---

### 2. DAS Check Tests (`das_check_test.rs`)
**Šta se testira:**
- ✅ JSON response parsing (success, error, edge cases)
- ✅ Request format (searchAssets, getAssetsByCreator)
- ✅ URL formatting sa API key
- ✅ Pubkey parsing i validacija
- ✅ Type conversions (u64 → u32)
- ✅ Fallback logic struktura
- ✅ Error handling struktura
- ✅ Edge cases (null, missing fields, negative numbers)

**Broj testova:** 20  
**Vreme:** <1 sekunda  
**Zahteva:** Nema

---

### 3. Target Mint Tests (`target_mint_test.rs`)
**Šta se testira:**
- ✅ Target mint config loading
- ✅ Target mint filtering logic
- ✅ Env var parsing
- ✅ Pubkey validation
- ✅ Display formatting
- ✅ Config cloning
- ✅ Seen tokens integration

**Broj testova:** 6  
**Vreme:** <1 sekunda  
**Zahteva:** Nema

---

### 4. Target Mint Integration Tests (`target_mint_integration_test.rs`)
**Šta se testira:**
- ✅ Target mint filtering sa real config
- ✅ Mock accounts filtering
- ✅ UI display format
- ✅ Config updates
- ✅ Integration sa seen tokens

**Broj testova:** 5  
**Vreme:** <1 sekunda  
**Zahteva:** Nema (može zahtevati API key za neke testove)

---

### 5. Mock Buy Tests (`mock_buy_test.rs`)
**Šta se testira:**
- ✅ Mock buy config
- ✅ Shared config updates
- ✅ Mock signature format
- ✅ Config state management

**Broj testova:** 3  
**Vreme:** <1 sekunda  
**Zahteva:** Nema

---

### 6. WebSocket Tests (`websocket_server_test.rs`)
**Šta se testira:**
- ✅ WebSocket connection (ako je API key postavljen)
- ✅ RPC connection (ako je API key postavljen)
- ✅ Subscription message creation
- ✅ Message parsing
- ✅ Config sa real URLs
- ✅ Health check sa real connections
- ✅ WebSocket helpers

**Broj testova:** 7  
**Vreme:** 2-5 sekundi  
**Zahteva:** `HELIUS_API_KEY` (opciono - testovi će se preskočiti ako nije postavljen)

---

### 7. Integration Tests (`integration.rs`)
**Šta se testira:**
- ⚠️ Full buy flow (zahteva real wallet i SOL)
- ⚠️ Jito bundle submission (zahteva real Jito endpoint)
- ⚠️ Helius transaction submission (zahteva real Helius endpoint)
- ⚠️ WebSocket connection (zahteva real WebSocket)
- ⚠️ Transaction parsing sa real RPC

**Broj testova:** 5+  
**Vreme:** 10-30 sekundi  
**Zahteva:** 
- `HELIUS_API_KEY`
- Real wallet sa SOL (za buy testove)
- Network access

**Napomena:** Ovi testovi su označeni sa `#[ignore]` i ne pokreću se automatski.

---

## ✅ Šta se pokriva testovima

### Core Functionality
- [x] Config loading i validacija
- [x] Target mint filtering
- [x] DAS API integration
- [x] WebSocket connection
- [x] Transaction parsing
- [x] Buy instruction building
- [x] Error handling
- [x] Retry logic

### Edge Cases
- [x] Invalid config values
- [x] Missing API keys
- [x] Network errors
- [x] Invalid pubkeys
- [x] Empty responses
- [x] Timeout handling

### Integration Points
- [x] Config ↔ GUI updates
- [x] WebSocket ↔ Bot core
- [x] RPC ↔ Detection
- [x] Filters ↔ Bot core
- [x] Metrics ↔ All modules

---

## 🎯 Kada su testovi prošli

Kada svi testovi prođu, možete biti sigurni da:

1. ✅ **Config system radi** - učitava i validira sve parametre
2. ✅ **Target mint filtering radi** - filtrira tokeni pravilno
3. ✅ **DAS check radi** - parsira odgovore i radi fallback
4. ✅ **WebSocket connection radi** - može se povezati i primati poruke
5. ✅ **Error handling radi** - greške se pravilno obrađuju
6. ✅ **Retry logic radi** - automatski retry sa backoff-om
7. ✅ **Rate limiting radi** - sprečava previše zahteva
8. ✅ **Metrics collection radi** - prikuplja statistiku

---

## ⚠️ Šta testovi NE pokrivaju

Ovi testovi **ne testiraju**:
- Real buy transakcije (zahteva SOL)
- Real Jito bundle submission (zahteva SOL i Jito endpoint)
- Real Helius submission (zahteva SOL i Helius endpoint)
- Production environment (koristi test/mock podatke)

Za ovo, koristite:
- Testnet environment
- Mock buy mode (`MOCK_BUY=true`)
- Manual testing sa malim iznosima

---

## 🔧 Troubleshooting

### Testovi padaju
1. Proverite da li je Cargo instaliran
2. Proverite da li je Rust toolchain aktivan
3. Pokrenite sa `-Verbose` da vidite detalje
4. Proverite `.env` fajl (neki testovi ga zahtevaju)

### Integration testovi padaju
- To je normalno ako nemate API keys
- Integration testovi su opcioni
- Koristite `-SkipIntegration` da ih preskočite

### WebSocket testovi padaju
- Proverite `HELIUS_API_KEY` u `.env`
- Proverite internet konekciju
- Testovi će se preskočiti ako API key nije postavljen

---

## 📊 Test Coverage

| Module | Unit Tests | Integration Tests | Coverage |
|--------|-----------|------------------|----------|
| Config | ✅ 15+ | ⚠️ 2 | ~90% |
| DAS Check | ✅ 20+ | ⚠️ 1 | ~85% |
| Target Mint | ✅ 6 | ✅ 5 | ~95% |
| WebSocket | ✅ 7 | ⚠️ 1 | ~80% |
| Bot Core | ✅ 10+ | ⚠️ 0 | ~70% |
| Buy | ✅ 5+ | ⚠️ 1 | ~75% |
| Utils | ✅ 8+ | ⚠️ 0 | ~90% |

**Ukupno:** 120+ unit testova, 10+ integration testova

---

## 🎓 Best Practices

1. **Uvek pokrenite testove pre commit-a**
   ```powershell
   .\test_all_comprehensive.ps1
   ```

2. **Ako menjate kritične funkcionalnosti, pokrenite sa `-Verbose`**
   ```powershell
   .\test_all_comprehensive.ps1 -Verbose
   ```

3. **Pre production deploy-a, pokrenite sve uključujući integration testove**
   ```powershell
   .\test_all_comprehensive.ps1
   ```

4. **Ako testovi padaju, proverite:**
   - Da li su sve zavisnosti instalirane
   - Da li je `.env` fajl ispravan
   - Da li je internet konekcija aktivna (za WebSocket testove)

---

## 📝 Dodavanje novih testova

Kada dodajete novu funkcionalnost:

1. Dodajte unit testove u odgovarajući modul
2. Dodajte integration testove u `tests/integration.rs` (ako je potrebno)
3. Ažurirajte ovaj checklist
4. Pokrenite `test_all_comprehensive.ps1` da proverite da sve radi

---

**Last Updated:** 2024-11-22  
**Test Suite Version:** 1.0

