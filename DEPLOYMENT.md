# Deployment Guide

## Building

### From Source

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
