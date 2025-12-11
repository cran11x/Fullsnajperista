<!-- 711cd93d-66f4-48af-8bb2-4b3734794e7a f42baa64-1d1a-4b15-84c2-43022f6b160b -->
# Optimizacija brzine buy funkcije

## Problem

Funkcija `build_buy_instruction` u [src/buy.rs](src/buy.rs) ima značajne performanse probleme:

- **Preko 30 `eprintln!` poziva** koji se izvršavaju na svakom pozivu (linije 85-358)
- **String formatiranje** u hot path-u (formatiranje Pubkey, f64 konverzije)
- **Redundantne PDA kalkulacije** (User Volume se derivira 3 puta)
- **Parsiranje statičkih vrednosti** (Token Program 2022 ID se parsira iz stringa svaki put)
- **Kreiranje vektora** za account labels na svakom pozivu

## Rešenje

### 1. Uklanjanje debug logging-a (najveći uticaj)

- Ukloniti ili učiniti uslovnim sve `eprintln!` pozive (linije 85-86, 89-90, 97-98, 117-131, 146-358)
- Zadržati samo kritične error poruke
- Dodati feature flag `debug_buy` za opciono debug logging

### 2. Cache statičkih vrednosti

- Cache-ovati Token Program 2022 ID u `static` varijabli (linije 172-173, 244-245)
- Cache-ovati `account_labels` vektor kao `const` ili `static`
- Cache-ovati PUMP_PROGRAM_ID parsiranje

### 3. Eliminisanje redundantnih operacija

- Ukloniti duplu PDA verifikaciju (User Volume se derivira 3 puta - linije 144, 308, 346)
- Zadržati samo jednu verifikaciju pre build-a instrukcije
- Optimizovati creator vault proveru (linije 191-204)

### 4. Optimizacija string operacija

- Ukloniti sve formatiranje stringova iz hot path-a
- Zadržati samo minimalne println za korisnički output (linije 117-131)

### 5. Pre-allokacija vektora

- Koristiti `Vec::with_capacity` za accounts vektor (16 elemenata)
- Optimizovati data vektor alokaciju

## Očekivani rezultati

- **50-80% brže izvršavanje** build_buy_instruction funkcije
- **Smanjenje CPU usage** u hot path-u
- **Bolje performanse** pri brzom sniping-u tokena

## Fajlovi za izmenu

- [src/buy.rs](src/buy.rs) - glavne optimizacije

### To-dos

- [ ] Ukloniti sve eprintln! debug logove iz build_buy_instruction (linije 85-358), zadržati samo kritične error poruke
- [ ] Cache-ovati Token Program 2022 ID, account_labels, i PUMP_PROGRAM_ID kao static/const vrednosti
- [ ] Eliminisati redundantne PDA derivacije - User Volume se derivira 3 puta, zadržati samo jednu
- [ ] Pre-allokovati accounts vektor sa capacity 16 i optimizovati data vektor
- [ ] Ukloniti string formatiranje iz hot path-a, zadržati samo minimalne println poruke