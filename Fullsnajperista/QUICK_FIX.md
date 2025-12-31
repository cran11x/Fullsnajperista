# Brzo rešenje za Ubuntu server

## Problem
`rustup` je instaliran preko `apt` ali nije pravilno konfigurisan.

## Brzo rešenje (kopiraj i pokreni):

```bash
# 1. Ukloni apt verziju
apt-get remove -y rustup 2>/dev/null || true
apt-get purge -y rustup 2>/dev/null || true

# 2. Instaliraj zvaničnu verziju
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# 3. Postavi PATH
export PATH="$HOME/.cargo/bin:$PATH"
source $HOME/.cargo/env

# 4. Instaliraj nightly
rustup toolchain install nightly
rustup default nightly

# 5. Verifikuj
rustc --version
cargo --version

# 6. Build
cargo build --release
```

## Ili koristi automatsku skriptu:

```bash
chmod +x setup_rust.sh
./setup_rust.sh
cargo build --release
```

