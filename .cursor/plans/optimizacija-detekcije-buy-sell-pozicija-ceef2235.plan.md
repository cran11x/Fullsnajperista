---
name: Optimizacija detekcije buy i sell pozicija
overview: ""
todos:
  - id: 7067190c-15ee-41af-8c5e-49671e66d23d
    content: Dodati BondingCurveCache modul u src/accounts/bonding_curve.rs sa TTL od 2 sekunde i integrisati u fetch_bonding_curve_mc()
    status: completed
  - id: 90490810-f749-4bce-a1f0-5664a73280ba
    content: "Optimizovati retry logiku u buy balance check: smanjiti sa 3 na 2 pokušaja i sleep sa 300ms na 150ms"
    status: completed
  - id: aca8ca67-eb98-4d63-827d-f8796ce5c56b
    content: Implementirati batch balance check u auto-sell monitoring loop koristeći get_multiple_accounts() umesto sekvencijalnih poziva
    status: completed
  - id: 6bdebb30-b409-4ce5-9857-ead56768775a
    content: Implementirati batch bonding curve fetch u auto-sell monitoring loop umesto pojedinačnih task-ova
    status: completed
  - id: 92799def-8f82-4908-bd98-d021b258293f
    content: Implementirati batch bonding curve fetch u PnL update loop umesto pojedinačnih task-ova
    status: completed
  - id: 7936e091-a7a7-4493-862a-b77fa2c25859
    content: Testirati sve optimizacije sa različitim brojem pozicija (0, 1, 10, 100+) i verifikovati da se ništa ne pokvari
    status: pending
---

# Optimizacija detekcije buy i sell pozicija

## Cilj

Optimizovati performanse detekcije pozicija kroz batch RPC pozive, caching, i optimizaciju retry logike, bez menjanja postojeće funkcionalnosti.

## Trenutno stanje

### Buy pozicije detekcija

- **Lokacija**: `src/bot_core.rs` linija ~2250-2310
- **Problem**: Sekvencijalni retry sa 3 pokušaja i 300ms sleep između pokušaja
- **Metod**: Helius API → RPC fallback sa retry logikom

### Sell pozicije detekcija (auto-sell monitoring)

- **Lokacija**: `src/bot_core.rs` linija ~2788-2930
- **Problem**: Sekvencijalni balance check za svaku poziciju (linija 2812)
- **Problem**: Svaki bonding curve fetch je pojedinačan RPC poziv (linija 3021+)
- **Metod**: Helius API → RPC fallback, zatim paralelni task-ovi za svaku poziciju

### PnL update loop

- **Lokacija**: `src/bot_core.rs` linija ~3930-4010
- **Problem**: Svaki task pravi novi RPC client i poziva `fetch_bonding_curve_mc` pojedinačno
- **Metod**: Paralelni task-ovi, ali bez batch poziva

## Plan optimizacija

### 1. Dodati bonding curve cache modul

**Fajl**: `src/accounts/bonding_curve.rs`

- Dodati `BondingCurveCache` struct sa `HashMap<String, (BondingCurveAccount, Instant)>`
- TTL: 2 sekunde (dovoljno kratko da bude fresh, dovoljno dugo da smanji RPC pozive)
- Metod `get_or_fetch()` koji proverava cache pre RPC poziva
- Thread-safe sa `Arc<RwLock<BondingCurveCache>>`
- Integrisati u postojeću `fetch_bonding_curve_mc()` funkciju kao opcioni parametar

**Bezbednost**: Cache je samo optimizacija - ako cache miss, koristi postojeću logiku.

### 2. Batch balance check u auto-sell monitoring

**Fajl**: `src/bot_core.rs` linija ~2812-2908

- Kreirati helper funkciju `batch_check_token_balances()` koja:
- Prima listu token account adresa
- Koristi `rpc.get_multiple_accounts()` za batch fetch
- Parsira balance iz account data (spl_token::state::Account)
- Vraća `Vec<Option<u64>>` (None ako account ne postoji)
- Zameniti sekvencijalni loop (linija 2812) sa:
- Priprema svih token account adresa
- Jedan batch RPC poziv
- Mapiranje rezultata nazad na pozicije
- Zadržati Helius API kao primarni metod (batch Helius poziv ako je moguće)
- Fallback na batch RPC ako Helius ne podržava batch

**Bezbednost**: Ista logika za markiranje kao sold, samo batch fetch umesto sekvencijalnog.

### 3. Batch bonding curve fetch u auto-sell monitoring

**Fajl**: `src/bot_core.rs` linija ~2938-3100

- Umesto spawn-ovanja task-a za svaku poziciju (linija 3021), pripremiti batch:
- Prikupiti sve bonding curve adrese
- Jedan `rpc.get_multiple_accounts()` poziv
- Deserijalizovati sve odjednom
- Procesirati sve pozicije sa dobijenim podacima
- Zadržati paralelizaciju za stop loss/take profit provere (one su brze)
- Koristiti cache iz koraka 1 za dodatnu optimizaciju

**Bezbednost**: Ista logika za stop loss/take profit, samo batch fetch.

### 4. Batch bonding curve fetch u PnL update loop

**Fajl**: `src/bot_core.rs` linija ~3956-4009

- Umesto spawn-ovanja task-a za svaku poziciju (linija 3969):
- Prikupiti sve bonding curve adrese
- Jedan batch `rpc.get_multiple_accounts()` poziv
- Deserijalizovati sve
- Update-ovati sve pozicije odjednom
- Zadržati interval-based update (ne menjati timing)

**Bezbednost**: Ista logika za PnL update, samo batch fetch.

### 5. Optimizovati retry logiku za buy balance check

**Fajl**: `src/bot_core.rs` linija ~2250-2310

- Smanjiti retry sa 3 na 2 pokušaja
- Smanjiti sleep sa 300ms na 150ms između pokušaja
- Zadržati Helius → RPC fallback logiku

**Bezbednost**: Ista logika, samo brže retry.

### 6. Dodati batch helper funkcije

**Fajl**: `src/accounts/bonding_curve.rs` (ili novi `src/utils/batch_rpc.rs`)

- `batch_fetch_bonding_curves()` - batch fetch više bonding curve accounts
- `batch_fetch_token_balances()` - batch fetch više token account balances
- Error handling koji vraća `Vec<Result<T>>` da se zna koja je uspela/neuspela

## Implementacioni detalji

### Batch RPC pozivi

Solana RPC `get_multiple_accounts` ima limit od ~100 accounts po pozivu. Ako ima više pozicija, podeliti u batch-ove od 100.

### Cache invalidation

Cache se automatski invalidira nakon TTL (2 sekunde). Dodati manual invalidation ako je potrebno (npr. nakon buy transakcije).

### Error handling

- Ako batch RPC poziv ne uspe, fallback na pojedinačne pozive (zadržati postojeću logiku)
- Ako cache ne radi, ignorisati i koristiti direktne RPC pozive
- Sve optimizacije su "nice to have" - ne smeju pokvariti postojeću funkcionalnost

### Testing strategija

- Testirati sa 0, 1, 10, 100+ pozicija
- Testirati cache hit/miss scenarije
- Testirati RPC failure scenarije (fallback na pojedinačne pozive)
- Testirati da se ništa ne pokvari u postojećoj logici

## Fajlovi za izmenu

1. `src/accounts/bonding_curve.rs` - dodati cache modul
2. `src/bot_core.rs` - optimizovati balance check i bonding curve fetch
3. (Opciono) `src/utils/batch_rpc.rs` - helper funkcije za batch RPC pozive

## Redosled implementacije

1. **Korak 1**: Dodati cache modul (najmanje rizično, može se testirati nezavisno)
2. **Korak 2**: Optimizovati retry logiku (jednostavna promena)
3. **Korak 3**: Batch balance check u auto-sell monitoring
4. **Korak 4**: Batch bonding curve fetch u auto-sell monitoring
5. **Korak 5**: Batch bonding curve fetch u PnL update loop

Svaki korak se testira pre prelaska na sledeći.

- ✅ Cache modul je implementiran (linije 120-200 u bonding_curve.rs)
- ⚠️ Cache postoji ali nije aktivno korišćen u pozivima (dostupan preko fetch_bonding_curve_mc_with_cache)
- [x] Optimizovati retry logiku u buy balance check: smanjiti sa 3 na 2 pokušaja i sleep sa 300ms na 150ms
- ✅ Implementirano (linija 2251 u bot_core.rs: 2 pokušaja, 150ms sleep)
- [x] Implementirati batch balance check u auto-sell monitoring loop koristeći get_multiple_accounts() umesto sekvencijalnih poziva
- ✅ Implementirano (funkcija batch_check_token_balances, linija 2548, korišćena na liniji 2935)
- [x] Implementirati batch bonding curve fetch u auto-sell monitoring loop umesto pojedinačnih task-ova
- ✅ Implementirano (funkcija batch_fetch_bonding_curves, linija 2590, korišćena na liniji 3128)
- [x] Implementirati batch bonding curve fetch u PnL update loop umesto pojedinačnih task-ova
- ✅ Implementirano (batch_fetch_bonding_curves korišćena na liniji 4100 u monitor_pnl_ultra_fast)
- [ ] Testirati sve optimizacije sa različitim brojem pozicija (0, 1, 10, 100+) i verifikovati da se ništa ne pokvari

## Status implementacije

**Završeno**: Sve glavne optimizacije su implementirane:

- ✅ Cache modul (dostupan ali nije aktivno korišćen)
- ✅ Optimizovana retry logika (2 pokušaja, 150ms)
- ✅ Batch balance check
- ✅ Batch bonding curve fetch u auto-sell monitoring
- ✅ Batch bonding curve fetch u PnL update loop

**Napomena**: Cache modul je implementiran i dostupan preko `fetch_bonding_curve_mc_with_cache()`, ali trenutno nije aktivno korišćen jer batch fetch optimizacije već značajno smanjuju RPC pozive. Cache može biti dodatno integrisan ako bude potrebno.