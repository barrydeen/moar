# MOAR (Mother Of All Relays)

A customizable, multi-tenant Nostr relay gateway built in Rust. Run multiple independent relay instances with different policies behind a single server, all managed through a web-based admin interface.

## Features

- **Multi-Tenant Architecture** - Run multiple relays on one server, each with its own database, policies, and subdomain
- **Flexible Policy Engine** - Per-relay read/write access control, pubkey allow/block lists, event kind filtering, content length limits, proof-of-work requirements, and rate limiting
- **Web Admin Dashboard** - Manage relays through a browser UI with NIP-98 authentication via Nostr browser extensions
- **LMDB Storage** - Fast embedded storage with full indexing (by author, kind, tags, timestamp)
- **Nostr Protocol Support** - NIP-01 (basic protocol), NIP-13 (proof of work), NIP-42 (authentication), NIP-98 (HTTP auth), replaceable and parameterized replaceable events
- **TOML Configuration** - Human and LLM-friendly config format

## Quick Start

### Docker (Recommended)

The fastest way to deploy MOAR with automatic TLS:

```bash
git clone https://github.com/example/moar.git
cd moar
cp .env.example .env
# Edit .env — set MOAR_DOMAIN and ADMIN_PUBKEY
docker compose up -d
```

This starts MOAR behind Caddy, which automatically provisions TLS certificates for your domain and all relay subdomains. Just point your DNS `A` record (and `*.yourdomain`) at your server and open ports 80 + 443.

Or use the one-line installer:

```bash
curl -fsSL https://raw.githubusercontent.com/example/moar/master/install.sh | bash
```

See [DEPLOYMENT.md](DEPLOYMENT.md) for full Docker and bare-metal deployment guides.

### From Source

Prerequisites: Rust 1.75+ and Cargo

```bash
git clone https://github.com/example/moar.git
cd moar
cp moar.example.toml moar.toml   # Create your config from the example
cargo build --release
./target/release/moar start
```

The gateway starts on `http://localhost:8080` by default. Access the admin dashboard at `http://localhost:8080/admin`.

### CLI Usage

```bash
# Start with default config (moar.toml)
moar start

# Start with a custom config file
moar start --config /path/to/config.toml
moar start -c config.toml
```

## Configuration

MOAR is configured via a TOML file. See `moar.example.toml` for a complete example.

### Global Settings

```toml
domain = "relay.example.com"   # Base domain for all relays
port = 8080                    # HTTP listen port
```

### Relay Instances

Each relay is defined under `[relays.<id>]` and gets its own subdomain, database, and policy:

```toml
[relays.outbox]
name = "My Outbox"
description = "Only I can post here"
subdomain = "outbox"                # wss://outbox.relay.example.com/
db_path = "data/outbox.mdb"
```

### Policies

Policies are optional - omitting them defaults to open access.

**Write Policy** - Control who can publish events:
```toml
[relays.outbox.policy.write]
require_auth = false
allowed_pubkeys = ["npub1..."]    # Whitelist (hex or bech32)
blocked_pubkeys = ["npub1..."]    # Blacklist
```

**Read Policy** - Control who can query events:
```toml
[relays.outbox.policy.read]
require_auth = false
allowed_pubkeys = ["npub1..."]
```

**Event Policy** - Filter by event properties:
```toml
[relays.outbox.policy.events]
allowed_kinds = [1, 4]            # Only accept these kinds
blocked_kinds = [5]               # Reject these kinds
min_pow = 0                       # NIP-13 proof-of-work difficulty
max_content_length = 10000        # Max content size in bytes
```

**Rate Limiting:**
```toml
[relays.outbox.policy.rate_limit]
writes_per_minute = 60
reads_per_minute = 120
```

### Common Relay Patterns

**Public Relay** (open read/write):
```toml
[relays.public]
name = "Public Relay"
subdomain = "www"
db_path = "data/public.mdb"
```

**Outbox Relay** (public read, whitelisted write):
```toml
[relays.outbox]
name = "My Outbox"
subdomain = "outbox"
db_path = "data/outbox.mdb"

[relays.outbox.policy.write]
allowed_pubkeys = ["npub1..."]
```

**DM Relay** (authenticated read/write, specific kinds only):
```toml
[relays.dms]
name = "DM Relay"
subdomain = "dm"
db_path = "data/dm.mdb"

[relays.dms.policy.write]
require_auth = true
allowed_pubkeys = ["npub1..."]

[relays.dms.policy.read]
require_auth = true
allowed_pubkeys = ["npub1..."]

[relays.dms.policy.events]
allowed_kinds = [4, 1059]
```

## Admin API

The admin dashboard is available at the gateway's root domain. Authentication uses NIP-98 via a Nostr browser extension (nos2x, Alby, etc.).

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/login` | Authenticate with NIP-98 signed event |
| `POST` | `/api/logout` | Clear session |
| `GET` | `/api/status` | Server status and pending restart flag |
| `GET` | `/api/relays` | List all relays |
| `GET` | `/api/relays/:id` | Get relay config |
| `POST` | `/api/relays` | Create relay |
| `PUT` | `/api/relays/:id` | Update relay |
| `DELETE` | `/api/relays/:id` | Delete relay |

Changes made via the admin API are persisted to the TOML config file. Some changes require a server restart to take effect (the UI will indicate this).

## Architecture

```
Client (WebSocket)
    │
    ▼
┌─────────────────────────────────────────┐
│  Gateway (host-based routing)           │
│                                         │
│  relay.example.com → Admin UI/API       │
│  outbox.relay.example.com → Relay WS    │
│  dm.relay.example.com → Relay WS        │
└─────────────────────────────────────────┘
    │                │
    ▼                ▼
┌─────────┐   ┌─────────┐
│ Policy  │   │ Policy  │   ← Per-relay policy engine
│ Engine  │   │ Engine  │
└─────────┘   └─────────┘
    │                │
    ▼                ▼
┌─────────┐   ┌─────────┐
│  LMDB   │   │  LMDB   │   ← Independent databases
└─────────┘   └─────────┘
```

## Development

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- start

# Run specific test suites
cargo test --lib policy         # Policy engine tests
cargo test integration_         # Integration tests
```

## Roadmap

- **JSONL Import/Export** - Bulk import and export events in JSONL format for backups and migration between relays
- **Custom Relay Home Pages** - Serve a customizable HTML landing page per relay when accessed via browser
- **Relay Metadata (NIP-11)** - Serve relay information documents with operator info, supported NIPs, and policy details
- **Blossom Support** - Integrate with the Blossom protocol for media/file hosting alongside your relay
- **Web of Trust Filter** - Policy rules based on social graph distance, allowing writes from followed/trusted pubkeys
- **Onboarding/Setup Wizard** - Guided first-run experience to configure your domain, create initial relays, and set up admin access

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
