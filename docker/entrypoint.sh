#!/bin/bash
set -e

CONFIG_DIR="/app/config"
CONFIG_FILE="${CONFIG_DIR}/moar.toml"

# Generate config from template on first run
if [ ! -f "$CONFIG_FILE" ]; then
    echo "First run detected — generating config from template..."
    mkdir -p "$CONFIG_DIR"
    envsubst < /app/moar.toml.template > "$CONFIG_FILE"
    echo "Config written to ${CONFIG_FILE}"
else
    echo "Existing config found at ${CONFIG_FILE}, skipping generation."
fi

exec /app/moar start --config "$CONFIG_FILE"
