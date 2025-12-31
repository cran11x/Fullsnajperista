# Ubuntu Setup Instructions

## Problem
Projekat zahteva `edition2024` koja je dostupna samo u nightly verziji Rust-a.

**Važno:** Ako ste instalirali `rustup` preko `apt`, možda nije pravilno konfigurisan. Koristite zvanični installer.

## Rešenje

### 1. Uklonite apt verziju rustup-a (ako postoji i ne radi)
```bash
apt-get remove -y rustup
apt-get purge -y rustup
```

### 2. Instalirajte rustup preko zvaničnog installera
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
export PATH="$HOME/.cargo/bin:$PATH"
source $HOME/.cargo/env
```

### 3. Dodajte u PATH trajno (opciono, ali preporučeno)
```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'source $HOME/.cargo/env' >> ~/.bashrc
source ~/.bashrc
```

### 2. Ažurirajte rustup i instalirajte nightly toolchain
```bash
rustup update
rustup toolchain install nightly
rustup default nightly
```

### 3. Verifikujte instalaciju
```bash
rustc --version
cargo --version
```

Oba bi trebalo da pokazuju "nightly" verziju.

### 4. Pokrenite build
```bash
cd ~/Desktop/sniper/Fullsnajperista
cargo build --release
```

## Alternativno: Koristite rust-toolchain.toml

Ako ne želite da menjate globalni default toolchain, projekat već ima `rust-toolchain.toml` fajl koji automatski koristi nightly verziju za ovaj projekat. Samo pokrenite:

```bash
rustup toolchain install nightly
cargo build --release
```

Rustup će automatski prepoznati `rust-toolchain.toml` i koristiti nightly verziju.

## Napomena

- Nightly verzija Rust-a može imati nestabilne funkcije
- Preporučuje se testiranje aplikacije pre produkcije
- Ako imate problema, možete se vratiti na stable sa: `rustup default stable`

