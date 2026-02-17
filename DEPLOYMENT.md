# Deployment Guide

## Docker Deployment (Recommended)

The Docker setup includes Caddy for automatic TLS — no wildcard certs or DNS API tokens needed. Caddy uses on-demand TLS (HTTP-01 challenge) to provision individual certificates per subdomain automatically.

### Prerequisites

- Docker and Docker Compose
- A domain with DNS A records pointing to your server:
  - `relay.example.com` → your server IP
  - `*.relay.example.com` → your server IP
- Ports 80 and 443 open

### Quick Start

```bash
git clone https://github.com/example/moar.git
cd moar
cp .env.example .env
```

Edit `.env` with your domain and admin pubkey:

```
MOAR_DOMAIN=relay.example.com
ADMIN_PUBKEY=your-hex-pubkey-here
RUST_LOG=info
```

Then start:

```bash
docker compose up -d
```

On first run, MOAR generates a starter config at `config/moar.toml` with sensible defaults (outbox, inbox, DM relays + blossom media server). After that, the config is managed through the admin UI and persists across restarts.

### What Gets Created

```
moar/
  config/moar.toml     # Generated on first run, preserved on restarts
  data/                # LMDB databases and blossom media files
  pages/               # Custom relay landing pages
```

### Managing

```bash
# View logs
docker compose logs -f

# Restart after config changes
docker compose restart moar

# Rebuild after code updates
git pull
docker compose up -d --build

# Stop everything
docker compose down
```

### How TLS Works

Caddy uses "on-demand TLS" — when a new subdomain is first accessed, Caddy:

1. Calls MOAR's `/.well-known/caddy-ask` endpoint to verify the hostname is valid
2. If valid, provisions a Let's Encrypt certificate via HTTP-01 challenge
3. Caches the cert in a named Docker volume (`caddy_data`)

This means when you add a new relay via the admin UI, its TLS cert is provisioned automatically on first access. No manual cert management needed.

### One-Line Installer

For a guided setup experience:

```bash
curl -fsSL https://raw.githubusercontent.com/example/moar/master/install.sh | bash
```

The installer detects Docker, prompts for your domain and pubkey, and starts everything.

---

## Bare-Metal Deployment

### Building

#### From Source

```bash
git clone https://github.com/example/moar.git
cd moar
cargo build --release
```

The binary is at `./target/release/moar`. It's self-contained with HTML templates embedded at compile time.

### System Requirements

- Linux (recommended), macOS, or Windows
- ~50MB RAM base + ~10MB per active relay
- LMDB requires a filesystem that supports sparse files and mmap (most standard filesystems work)

## Configuration Setup

```bash
# Copy the example config - never edit moar.example.toml directly
cp moar.example.toml moar.toml

# Edit your config
$EDITOR moar.toml
```

**Important:** `moar.toml` is your runtime config and is listed in `.gitignore`. The example file `moar.example.toml` is tracked in git and serves as a reference. When updating MOAR via `git pull`, your `moar.toml` will not be overwritten.

## Running

### Direct

```bash
./target/release/moar start --config moar.toml
```

### Systemd Service

Create `/etc/systemd/system/moar.service`:

```ini
[Unit]
Description=MOAR Nostr Relay Gateway
After=network.target

[Service]
Type=simple
User=moar
Group=moar
WorkingDirectory=/opt/moar
ExecStart=/opt/moar/moar start --config /opt/moar/moar.toml
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/moar/data
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```bash
# Install
sudo cp target/release/moar /opt/moar/moar
sudo cp moar.example.toml /opt/moar/moar.toml
sudo mkdir -p /opt/moar/data

# Create service user
sudo useradd -r -s /sbin/nologin moar
sudo chown -R moar:moar /opt/moar

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable moar
sudo systemctl start moar
sudo journalctl -u moar -f
```

## Reverse Proxy Setup

MOAR uses host-based routing, so your reverse proxy must forward the `Host` header and support WebSocket upgrades.

### Nginx

```nginx
# Wildcard server block for all relay subdomains
server {
    listen 443 ssl;
    server_name relay.example.com *.relay.example.com;

    ssl_certificate /etc/letsencrypt/live/relay.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/relay.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;

        # Required for WebSocket
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        # Required for host-based routing
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket timeouts
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }
}
```

### Caddy

```caddyfile
relay.example.com, *.relay.example.com {
    reverse_proxy localhost:8080
}
```

Caddy handles TLS, WebSocket upgrades, and host headers automatically.

### SSL Certificates

You need a wildcard certificate for `*.relay.example.com` since each relay uses a subdomain. With Let's Encrypt:

```bash
# Using certbot with DNS challenge (required for wildcards)
sudo certbot certonly --manual --preferred-challenges dns \
    -d relay.example.com -d '*.relay.example.com'
```

Or use a DNS provider plugin for automatic renewal:

```bash
# Example with Cloudflare
sudo certbot certonly --dns-cloudflare \
    --dns-cloudflare-credentials /etc/cloudflare.ini \
    -d relay.example.com -d '*.relay.example.com'
```

## DNS Setup

Create these DNS records:

| Type | Name | Value |
|------|------|-------|
| A | relay.example.com | Your server IP |
| A | *.relay.example.com | Your server IP |

The wildcard record ensures all relay subdomains resolve to your server.

## Data Management

### Database Location

Each relay stores its data in the `db_path` specified in config. Default convention is `data/<name>.mdb`:

```
data/
  public.mdb     # Public relay data
  outbox.mdb     # Outbox relay data
  dm.mdb         # DM relay data
```

### Backups

LMDB supports hot backups - you can copy the `.mdb` files while the server is running:

```bash
# Simple backup
cp -r /opt/moar/data /backups/moar-$(date +%Y%m%d)
```

### Storage Limits

Each relay database has a default maximum size of 10GB (configured in code). Monitor disk usage:

```bash
du -sh /opt/moar/data/*
```

## Updating

```bash
cd /path/to/moar
git pull
cargo build --release

# Your moar.toml is in .gitignore and won't be affected by git pull.
# Check moar.example.toml for any new config options.

sudo systemctl restart moar
```

## Logging

MOAR uses the `tracing` framework. Control log level with the `RUST_LOG` environment variable:

```bash
# In systemd, add to [Service]:
Environment="RUST_LOG=info"

# Available levels: error, warn, info, debug, trace
# Per-module filtering:
Environment="RUST_LOG=moar=debug,tower_http=info"
```

## Troubleshooting

**Relay not accessible via subdomain**
- Verify DNS wildcard record is set up
- Check that the reverse proxy forwards the `Host` header
- Confirm the subdomain in config matches what you're accessing

**WebSocket connections dropping**
- Increase proxy timeouts (see nginx config above)
- Check that `Upgrade` and `Connection` headers are forwarded

**Permission denied on data directory**
- Ensure the service user owns the data directory: `chown -R moar:moar /opt/moar/data`

**Admin login not working**
- Requires a Nostr browser extension (nos2x, Alby, etc.)
- The extension must be unlocked and have a key available

**Config changes not taking effect**
- Some changes require a restart. The admin UI shows a "pending restart" banner when needed.
- Restart the service: `sudo systemctl restart moar`
