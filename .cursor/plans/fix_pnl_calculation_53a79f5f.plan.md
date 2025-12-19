---
name: Fix PnL Calculation
overview: PnL procenat se prikazuje kao polovica stvarne vrednosti (300% -> 150%). Problem je u formuli za izračunavanje PnL procenta ili u nekonzistentnom rukovanju decimalima između cene tokena i količine tokena.
todos:
  - id: investigate-decimals
    content: Istražiti i ispraviti decimalne neskladove u bonding_curve.rs
    status: pending
  - id: fix-pnl-formula
    content: Ispraviti PnL formulu u tracker.rs ako je potrebno
    status: pending
  - id: add-debug-logs
    content: Dodati debug logove za verifikaciju izračunavanja
    status: pending
  - id: test-fix
    content: Testirati ispravku sa stvarnim podacima
    status: pending
---

# Fix PnL Calculation - Prikazuje 150% umesto 300%

## Problem

Kada korisnik ostvari 300% profit, sistem zapisuje samo 150% u CSV/JSON. Ovo je tačno faktor 2 greške.

## Root Cause Analiza

Nakon pregleda koda, pronašao sam sledeće potencijalne probleme:

### 1. Formula za PnL procenat u `tracker.rs`

Trenutna formula:

```rust
let current_pnl_percent = (pnl / cost_basis) * 100.0;
```

Gde je:

- `pnl = current_value_net - cost_basis`
- `cost_basis = our_buy_sol + buy_fees`

**Problem**: Formula je ispravna za PnL kao procenat od uloženog. Ali ako je `current_value_net` pogrešno izračunat, rezultat će biti pogrešan.

### 2. Decimalni nesklad u `bonding_curve.rs`

U `get_token_price_sol()`:

```rust
(self.virtual_sol_reserves as f64 / self.virtual_token_reserves as f64) / 1000.0
```

Komentari kažu:

- `virtual_sol_reserves` je u lamportima (1e9)
- `virtual_token_reserves` je u raw units (1e6)

Ali u `display()` funkciji:

```rust
self.virtual_token_reserves as f64 / 1e9  // Deli sa 1e9 umesto 1e6!
```

Ovo je nekonzistentno i može ukazivati na dublji problem.

### 3. Mogući scenariji greške

**Scenarijo A**: `token_amount` je pogrešno skaliran

- Ako je `token_amount` sačuvan sa 9 decimala umesto 6, a dele se sa 1e6, dobijamo faktor 1000 greške

**Scenarijo B**: `entry_price` i `current_price` koriste različite decimale

- Ako je entry_price u SOL/token sa 6 decimala, a current_price sa 9 decimala, dobijamo nesklad

**Scenarijo C**: Pump.fun formula koristi drugačiji decimalni sistem

- Treba proveriti pump.fun dokumentaciju

## Rešenje

### Korak 1: Ispraviti `get_token_price_sol()` u `bonding_curve.rs`

Treba verifikovati da je formula ispravna koristeći stvarne podatke iz pump.fun.

### Korak 2: Ispraviti decimalne konverzije u `tracker.rs`

Osigurati konzistentno rukovanje decimalima:

- `token_amount` je sa 6 decimala (pump.fun standard)
- `token_price_sol` je SOL po tokenu

### Korak 3: Dodati debug logging za verifikaciju

Dodati detaljne logove koji pokazuju:

- Ulazne vrednosti (token_amount, entry_price, current_price)
- Međurezultate (tokens_actual, current_value_gross)
- Finalni PnL

### Korak 4: Ispraviti `display()` funkciju

Ispraviti nekonzistentnost u prikazu (1e9 vs 1e6).

## Fajlovi za izmenu

1. [src/accounts/bonding_curve.rs](src/accounts/bonding_curve.rs) - Ispraviti `get_token_price_sol()` i `display()`
2. [src/accounts/tracker.rs](src/accounts/tracker.rs) - Ispraviti decimalne konverzije u `update_position_pnl_fast()`

## Predložena ispravka

U `bonding_curve.rs`, `display()` funkcija treba da koristi 1e6 za tokene:

```rust
self.virtual_token_reserves as f64 / 1e6  // Ispravno: 6 decimala
```

U `tracker.rs`, verifikovati formulu za PnL:

```rust
// Trenutna vrednost = tokeni * trenutna_cena
// PnL = trenutna_vrednost - uloženo
// PnL% = (PnL / uloženo) * 100
```