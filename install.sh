#!/bin/bash
set -e

REPO_URL="https://github.com/barrydeen/moar.git"
INSTALL_DIR="$HOME/moar"

echo "==============================="
echo "  MOAR Installer"
echo "==============================="
echo ""

# --- Gather required info ---

read -rp "Enter your domain (e.g. relay.example.com): " MOAR_DOMAIN
if [ -z "$MOAR_DOMAIN" ]; then
    echo "Error: domain is required."
    exit 1
fi

read -rp "Enter your admin pubkey (hex): " ADMIN_PUBKEY
if [ -z "$ADMIN_PUBKEY" ]; then
    echo "Error: admin pubkey is required."
    exit 1
fi

# --- Detect Docker ---

HAS_DOCKER=false
if command -v docker &>/dev/null && docker compose version &>/dev/null; then
    HAS_DOCKER=true
fi

if [ "$HAS_DOCKER" = true ]; then
    echo ""
    echo "Docker detected."
    read -rp "Install with Docker (recommended) or bare-metal? [docker/bare]: " INSTALL_METHOD
    INSTALL_METHOD="${INSTALL_METHOD:-docker}"
else
    echo ""
    echo "Docker not detected — using bare-metal install."
    INSTALL_METHOD="bare"
fi

# --- Docker install ---

if [ "$INSTALL_METHOD" = "docker" ]; then
    echo ""
    echo "Cloning MOAR..."
    if [ -d "$INSTALL_DIR" ]; then
        echo "Directory $INSTALL_DIR already exists, pulling latest..."
        cd "$INSTALL_DIR"
        git pull
    else
        git clone "$REPO_URL" "$INSTALL_DIR"
        cd "$INSTALL_DIR"
    fi

    echo ""
    echo "Writing .env..."
    cat > .env <<EOF
MOAR_DOMAIN=${MOAR_DOMAIN}
ADMIN_PUBKEY=${ADMIN_PUBKEY}
RUST_LOG=info
EOF

    mkdir -p data config pages

    echo ""
    echo "Building and starting containers..."
    docker compose up -d --build

    echo ""
    echo "==============================="
    echo "  MOAR is running!"
    echo "==============================="
    echo ""
    echo "  Admin:  https://${MOAR_DOMAIN}:8888"
    echo "  Outbox: wss://outbox.${MOAR_DOMAIN}/"
    echo "  Inbox:  wss://inbox.${MOAR_DOMAIN}/"
    echo "  DMs:    wss://dm.${MOAR_DOMAIN}/"
    echo "  Media:  https://media.${MOAR_DOMAIN}/"
    echo ""
    echo "Make sure your DNS has an A record for"
    echo "  ${MOAR_DOMAIN} and *.${MOAR_DOMAIN}"
    echo "pointing to this server, and ports 80, 443, and 8888 are open."
    echo ""
    echo "Logs: docker compose logs -f"
    exit 0
fi

# --- Bare-metal install ---

echo ""
echo "Bare-metal install"

# Check for Rust
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo ""
echo "Cloning MOAR..."
if [ -d "$INSTALL_DIR" ]; then
    echo "Directory $INSTALL_DIR already exists, pulling latest..."
    cd "$INSTALL_DIR"
    git pull
else
    git clone "$REPO_URL" "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi

echo ""
echo "Building Rust binary (this may take a few minutes)..."
cargo build --release

# Build admin panel
if command -v node &>/dev/null && command -v npm &>/dev/null; then
    echo ""
    echo "Building admin panel..."
    cd admin && npm ci && npm run build
    cd "$INSTALL_DIR"
else
    echo ""
    echo "Node.js not found — skipping admin panel build."
    echo "Install Node.js 20+ and run: cd admin && npm ci && npm run build"
fi

# Generate config if it doesn't exist
if [ ! -f moar.toml ]; then
    echo ""
    echo "Generating moar.toml..."
    export MOAR_DOMAIN ADMIN_PUBKEY
    envsubst < docker/moar.toml.template > moar.toml
    # Fix paths for bare-metal (use relative paths instead of /app/data/)
    sed -i 's|/app/data/||g' moar.toml
    sed -i 's|pages_dir = "pages"|pages_dir = "pages"|' moar.toml
fi

mkdir -p data pages

echo ""
echo "==============================="
echo "  MOAR built successfully!"
echo "==============================="
echo ""
echo "  Binary: $INSTALL_DIR/target/release/moar"
echo "  Config: $INSTALL_DIR/moar.toml"
echo ""
echo "  Start:  ./target/release/moar start"
echo ""
echo "  You still need to set up a reverse proxy (Caddy or nginx)"
echo "  with TLS for your domain. See DEPLOYMENT.md for details."
echo ""
