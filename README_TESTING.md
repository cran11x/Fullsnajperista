# 🧪 Testing Guide - Fullsnajperista Sniper Bot

## 🚀 Quick Start

**Pokrenite sve testove sa jednom komandom:**
```powershell
.\test_all_comprehensive.ps1
```

Ovo će pokrenuti **sve testove** i dati vam potpunu sliku o tome da li sve radi kako treba.

---

## 📋 Šta se testira?

### ✅ Unit Tests (120+ testova)
- Config loading i validacija
- Target mint filtering
- DAS check parsing
- Error handling
- Retry logic
- Rate limiting
- Utils funkcije
- Account tracking
- Metrics collection

### ✅ Integration Tests
- WebSocket connection
- RPC connection
- Target mint filtering
- Mock buy functionality

### ⚠️ Optional Tests (zahtevaju API keys)
- Real WebSocket connection
- Real RPC calls
- Real transaction parsing

---

## 🎯 Kada su testovi prošli

Kada vidite:
```
🎉 ALL TESTS PASSED!
✅ Your sniper bot is ready to use!
```

**Možete biti 100% sigurni da:**
1. ✅ Config system radi
2. ✅ Target mint filtering radi
3. ✅ DAS check radi
4. ✅ Error handling radi
5. ✅ Retry logic radi
6. ✅ Rate limiting radi
7. ✅ Metrics collection radi

---

## 📊 Test Coverage

| Feature | Unit Tests | Integration | Status |
|---------|-----------|-------------|--------|
| Config | ✅ 15+ | ✅ | 90%+ |
| Target Mint | ✅ 6 | ✅ 5 | 95%+ |
| DAS Check | ✅ 20+ | ⚠️ | 85%+ |
| WebSocket | ✅ 7 | ⚠️ | 80%+ |
| Bot Core | ✅ 10+ | ⚠️ | 70%+ |
| Buy Logic | ✅ 5+ | ⚠️ | 75%+ |

**Ukupno: 120+ testova**

---

## 🔧 Opcije testiranja

### 1. Brzo testiranje (preporučeno)
```powershell
.\test_all_comprehensive.ps1
```
Pokreće sve testove osim integration testova koji zahtevaju API keys.

### 2. Sa detaljnim output-om
```powershell
.\test_all_comprehensive.ps1 -Verbose
```
Prikazuje detaljne informacije o svakom testu.

### 3. Bez integration testova
```powershell
.\test_all_comprehensive.ps1 -SkipIntegration
```
Preskače integration testove (brže).

### 4. Samo unit testovi
```powershell
cargo test --lib
```

### 5. Specifičan test suite
```powershell
cargo test --test das_check_test
cargo test --test target_mint_test
cargo test --test websocket_server_test
```

---

## ⚠️ Troubleshooting

### Testovi padaju?
1. **Proverite Cargo:**
   ```powershell
   cargo --version
   ```

2. **Proverite .env fajl:**
   - Neki testovi zahtevaju `HELIUS_API_KEY`
   - Ako nije postavljen, testovi će se preskočiti (to je OK)

3. **Pokrenite sa -Verbose:**
   ```powershell
   .\test_all_comprehensive.ps1 -Verbose
   ```

### WebSocket testovi padaju?
- **To je normalno** ako nemate `HELIUS_API_KEY`
- Testovi će se preskočiti automatski
- Ne utiče na funkcionalnost bot-a

### Integration testovi padaju?
- Integration testovi zahtevaju:
  - `HELIUS_API_KEY` u `.env`
  - Internet konekciju
  - Real wallet (za buy testove)
- Koristite `-SkipIntegration` da ih preskočite

---

## 📝 Pre commit-a

**Uvek pokrenite:**
```powershell
.\test_all_comprehensive.ps1
```

Ako svi testovi prođu, možete commit-ovati sa sigurnošću.

---

## 🎓 Best Practices

1. **Pokrenite testove pre svake promene**
2. **Ako menjate kritične funkcionalnosti, koristite `-Verbose`**
3. **Pre production deploy-a, pokrenite sve testove**
4. **Ako testovi padaju, proverite output sa `-Verbose`**

---

## 📚 Dodatne informacije

Za detaljne informacije o svakom testu, pogledajte:
- `TEST_CHECKLIST.md` - Kompletan spisak testova
- `tests/README.md` - Detalji o testovima

---

**Last Updated:** 2024-11-22

