# Sljedeći koraci - Provjera PDA integracije

## ✅ Što je već urađeno

- [x] PDA derivation modul kreiran
- [x] Integriran u `buy.rs`
- [x] Integriran u `sell.rs`
- [x] Svi testovi prolaze

## 📋 Sljedeći koraci za provjeru

### 1. Build projekta
```powershell
# Cargo nije u PATH-u, koristi puni put:
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
& $cargo build --release

# ILI ako imaš cargo u PATH-u:
cargo build --release
```
Provjeri da se kompajlira bez grešaka. ✅ Build je prošao uspješno!

### 2. Pokreni program u test modu
```powershell
# Ako imaš .env fajl sa test wallet-om
cargo run --release
```

### 3. Prati console output
Kada program pokuša kupiti token, traži ove poruke:

**DOBRO (sve radi):**
```
🔍 BUILDING BUY INSTRUCTION - Recalculated PDAs:
   Mint: <address>
   User Wallet: <address>
   Global: <pda> (recalculated)
   Bonding Curve: <pda> (recalculated)
   Event Authority: <pda> (recalculated)
   User Volume: <pda> (recalculated)
   Global Volume: <address> (hardcoded)

🔍 VERIFYING PDA ACCOUNTS FOR ERROR 0x1f9 PREVENTION:
   Account 0 (Global): <pda> (from accounts: <old>) ✅
   Account 3 (Bonding Curve): <pda> (from accounts: <old>) ✅
   ...
```

**Također DOBRO (stari PDAs bili pogrešni, ali sada koristimo ispravne):**
```
   Account 0 (Global): <pda> (from accounts: <old>) ❌ MISMATCH - USING RECALCULATED!
```
Ovo je OK! Znači da su stari PDAs bili pogrešni, ali sada koristimo ispravne.

### 4. Provjeri da transakcije prolaze
- **Prije**: Prva transakcija → Error 0x1f9 (Seeds Constraint Was Violated)
- **Sada**: Prva transakcija → Trebala bi proći ✅

### 5. Ako vidiš Error 0x1f9
Ako i dalje vidiš Error 0x1f9:
1. Provjeri da li vidiš "recalculated" u logovima
2. Provjeri da li se koriste `pdas.*` umjesto `accounts.*` u instrukciji
3. Provjeri da li su svi PDAs ispravno rekalkulirani

### 6. Test sa različitim tokenima
Pokušaj kupiti nekoliko različitih tokena:
- Svaki token trebao bi imati različit `bonding_curve` PDA
- Svaki user trebao bi imati različit `user_volume` PDA
- Ali `global` i `event_authority` trebaju biti isti za sve

## 🔍 Debug provjere

### Provjeri da se koriste rekalkulirani PDAs
U `src/buy.rs` linija ~150:
```rust
AccountMeta::new(pdas.global, false),  // ✅ DOBRO - koristi rekalkulirani
// NE: AccountMeta::new(accounts.global, false),  // ❌ LOŠE - koristi stari
```

### Provjeri da se rekalkulira za svaku transakciju
U `src/buy.rs` linija ~100:
```rust
let pdas = PumpPdas::recalculate_all(&accounts.mint, user_wallet);
```
Ovo se poziva za **svaku** transakciju, što je ispravno.

## ✅ Checklist - Kada znaš da sve radi

- [ ] Program se kompajlira bez grešaka
- [ ] Vidiš "recalculated" poruke u logovima
- [ ] Prva transakcija prolazi (nema Error 0x1f9)
- [ ] Svaka sljedeća transakcija također prolazi
- [ ] Različiti tokeni imaju različite bonding curve PDAs
- [ ] Različiti users imaju različite user volume PDAs

## 🚨 Ako nešto ne radi

1. **Provjeri logove** - traži "recalculated" poruke
2. **Provjeri kod** - da li se koristi `pdas.*` umjesto `accounts.*`
3. **Pokreni testove** - `cargo test pda_derivation --lib`
4. **Provjeri da li se `recalculate_all()` poziva** u `build_buy_instruction()`

## 📝 Napomene

- `creator_vault` i `associated_bonding_curve` se još uvijek uzimaju iz `accounts` jer se ne mogu izračunati kao PDA
- Sve ostale PDAs se rekalkuliraju svježe
- `bot_core.rs` još koristi `accounts.*` za provjeru, ali to je OK jer se ne koriste za instrukcije

