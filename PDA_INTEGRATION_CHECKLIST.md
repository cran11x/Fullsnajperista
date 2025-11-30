# PDA Integration Checklist - Kako provjeriti da sve radi

## ✅ Što je urađeno

1. **Kreiran `pda_derivation.rs` modul** - sve PDA funkcije na jednom mjestu
2. **Integriran u `buy.rs`** - sada koristi `PumpPdas::recalculate_all()` umjesto `accounts.*`
3. **Integriran u `sell.rs`** - sada koristi `PumpPdas::recalculate_all()` umjesto `accounts.*`
4. **Svi testovi prolaze** - unit testovi, integration testovi, i flow testovi

## 🧪 Kako provjeriti da sve radi

### 1. Pokreni sve testove

```powershell
# Unit testovi za PDA derivation
cargo test pda_derivation --lib

# Integration testovi
cargo test --test pda_derivation_test
cargo test --test pda_integration_flow_test

# Testovi za buy.rs
cargo test buy --lib

# Testovi za sell.rs
cargo test sell --lib

# Svi testovi zajedno
cargo test --lib
```

### 2. Provjeri u kodu

**U `src/buy.rs` linija ~100:**
```rust
let pdas = PumpPdas::recalculate_all(&accounts.mint, user_wallet);
// Zatim koristi pdas.global, pdas.bonding_curve, itd.
```

**U `src/sell.rs` linija ~48:**
```rust
let pdas = PumpPdas::recalculate_all(&accounts.mint, user_wallet);
// Zatim koristi pdas.global, pdas.bonding_curve, itd.
```

### 3. Debug output

Kada pokreneš program, trebao bi vidjeti u konzoli:
```
🔍 BUILDING BUY INSTRUCTION - Recalculated PDAs:
   Mint: <mint_address>
   User Wallet: <user_wallet>
   Global: <global_pda> (recalculated)
   Bonding Curve: <bonding_curve_pda> (recalculated)
   Event Authority: <event_authority_pda> (recalculated)
   User Volume: <user_volume_pda> (recalculated)
   Global Volume: <global_volume> (hardcoded)
```

Ako vidiš "❌ MISMATCH - USING RECALCULATED!" - to je OK! To znači da su stari PDAs bili pogrešni, ali sada koristimo ispravne.

### 4. Što očekivati u produkciji

**Prije (pogrešno):**
- Prva transakcija: Error 0x1f9 (Seeds Constraint Was Violated)
- Druga transakcija: ✅ Prolazi (jer su PDAs već bili ispravni)

**Sada (ispravno):**
- Prva transakcija: ✅ Prolazi (PDAs se rekalkuliraju svježe)
- Druga transakcija: ✅ Prolazi (PDAs se rekalkuliraju svježe)
- Svaka transakcija: ✅ Prolazi (PDAs se uvijek rekalkuliraju)

## 🔍 Kako provjeriti u stvarnom programu

1. **Pokreni program** i prati console output
2. **Pokušaj kupiti token** - prati debug poruke
3. **Provjeri da li vidiš "recalculated"** u logovima
4. **Provjeri da li transakcija prolazi** bez Error 0x1f9

## 📝 Napomene

- `bot_core.rs` još uvijek koristi `accounts.*` za provjeru, ali to je OK jer se koriste samo za logging/verification, ne za stvarne instrukcije
- `creator_vault` i `associated_bonding_curve` se još uvijek uzimaju iz `accounts` jer se ne mogu izračunati kao PDA - oni se ekstraktiraju iz BUY instrukcije
- Sve ostale PDAs se sada rekalkuliraju svježe

## ✅ Finalna provjera

Ako svi testovi prolaze i vidiš "recalculated" u logovima, sve radi ispravno!

