#!/bin/bash

# Setup script za Rust nightly toolchain na Ubuntu

set -e

echo "🔧 Postavljanje Rust nightly toolchain-a..."

# Proveri da li rustup radi pravilno
if command -v rustup &> /dev/null; then
    # Proveri da li je rustup pravilno instaliran
    if ! rustup show &> /dev/null; then
        echo "⚠️  rustup je instaliran ali nije pravilno konfigurisan"
        echo "🗑️  Uklanjanje apt verzije rustup-a..."
        apt-get remove -y rustup 2>/dev/null || true
        apt-get purge -y rustup 2>/dev/null || true
    else
        echo "✅ rustup je već pravilno instaliran"
        source $HOME/.cargo/env 2>/dev/null || true
    fi
fi

# Ako rustup nije instaliran ili je uklonjen, instaliramo ga
if ! command -v rustup &> /dev/null || ! rustup show &> /dev/null; then
    echo "📦 Instaliranje rustup-a preko zvaničnog installera..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    
    # Postavi PATH
    export PATH="$HOME/.cargo/bin:$PATH"
    source $HOME/.cargo/env 2>/dev/null || true
    
    # Dodaj u .bashrc i .profile za trajnost
    if ! grep -q '.cargo/env' $HOME/.bashrc 2>/dev/null; then
        echo 'source $HOME/.cargo/env' >> $HOME/.bashrc
    fi
    if ! grep -q '.cargo/env' $HOME/.profile 2>/dev/null; then
        echo 'source $HOME/.cargo/env' >> $HOME/.profile
    fi
fi

# Ažuriraj rustup
echo "🔄 Ažuriranje rustup-a..."
export PATH="$HOME/.cargo/bin:$PATH"
rustup update

# Instaliraj nightly toolchain
echo "🌙 Instaliranje nightly toolchain-a..."
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install nightly

# Postavi nightly kao default (opciono - rust-toolchain.toml će automatski koristiti nightly)
echo "⚙️  Postavljanje nightly kao default toolchain-a..."
rustup default nightly

# Verifikuj instalaciju
echo ""
echo "✅ Instalacija završena!"
echo ""
echo "Verzije:"
export PATH="$HOME/.cargo/bin:$PATH"
rustc --version
cargo --version
echo ""
echo "🚀 Sada možete pokrenuti: cargo build --release"

