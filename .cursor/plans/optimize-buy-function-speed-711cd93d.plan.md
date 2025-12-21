---
name: Optimizacija brzine buy funkcije
overview: ""
todos:
  - id: f7801efb-6e0d-4062-9fc5-c84336075952
    content: Ukloniti sve eprintln! debug logove iz build_buy_instruction (linije 85-358), zadržati samo kritične error poruke
    status: pending
  - id: 4e37bc08-678c-49b8-9018-d5a56e92850d
    content: Cache-ovati Token Program 2022 ID, account_labels, i PUMP_PROGRAM_ID kao static/const vrednosti
    status: pending
  - id: f5c0861c-452a-4b4d-a43c-16970b62ae92
    content: Eliminisati redundantne PDA derivacije - User Volume se derivira 3 puta, zadržati samo jednu
    status: pending
  - id: 5e5f86fa-da39-4de2-9d27-4c4daa87db9d
    content: Pre-allokovati accounts vektor sa capacity 16 i optimizovati data vektor
    status: pending
  - id: 4ef2a66e-ace7-478e-ab53-ed0cf6c79e0d
    content: Ukloniti string formatiranje iz hot path-a, zadržati samo minimalne println poruke
    status: pending
---

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