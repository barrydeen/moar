#!/bin/bash
set -e

# ==============================================================================
#  MOAR (Mother Of All Relays) Installer
# ==============================================================================

# --- Colors & Styling ---------------------------------------------------------
RESET='\033[0m'
BOLD='\033[1m'
DIM='\033[2m'
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'

# Check if stdout is a terminal
if [ -t 1 ]; then
    # We have color
    :
else
    # No color
    RESET=''
    BOLD=''
    DIM=''
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    CYAN=''
    MAGENTA=''
fi

# --- Constants ---------------------------------------------------------------
REPO_URL="https://github.com/barrydeen/moar.git"
INSTALL_DIR="$HOME/moar"
LOG_FILE="/tmp/moar_install.log"

# --- Cleanup -----------------------------------------------------------------

cleanup() {
    tput cnorm 2>/dev/null || true
    echo -e "${RESET}"
}
trap cleanup EXIT

# --- Helpers -----------------------------------------------------------------

clear_screen() {
    printf "\033c"
}

print_banner() {
    echo -e "${MAGENTA}${BOLD}"
    cat << "EOF"
  __  __   ____      _      ____  
 |  \/  | / __ \    / \    |  _ \ 
 | |\/| || |  | |  / _ \   | |_) |
 | |  | || |__| | / ___ \  |  _ < 
 |_|  |_| \____/ /_/   \_\ |_| \_\
                                  
EOF
    echo -e "${RESET}"
    echo -e "${DIM}  Mother Of All Relays - Setup Wizard${RESET}"
    echo -e "${DIM}  ===================================${RESET}\n"
}

step() {
    echo -e "\n${BLUE}${BOLD}>> $1${RESET}"
}

info() {
    echo -e "   ${CYAN}ℹ${RESET} $1"
}

success() {
    echo -e "   ${GREEN}✔${RESET} $1"
}

warn() {
    echo -e "   ${YELLOW}⚠${RESET} $1"
}

error() {
    echo -e "   ${RED}✖ $1${RESET}"
    echo -e "\n${RED}Installation failed. Check $LOG_FILE for details.${RESET}"
    exit 1
}

# Spinner function for running commands
run_task() {
    local msg="$1"
    shift
    
    # Hide cursor
    tput civis 2>/dev/null || true
    
    # Print the styling
    echo -ne "   ${DIM}⟳${RESET} $msg..."
    
    # Run command in background, redirect output to log
    "$@" >> "$LOG_FILE" 2>&1 &
    local pid=$!
    
    local delay=0.1
    local spinstr='|/-\'
    
    while kill -0 "$pid" 2>/dev/null; do
        local temp=${spinstr#?}
        printf " [%c]  " "$spinstr"
        local spinstr=$temp${spinstr%"$temp"}
        sleep $delay
        printf "\b\b\b\b\b\b"
    done
    
    # Capture exit code safely — plain `wait` under `set -e` would
    # terminate the script on non-zero, swallowing the error message.
    local exit_code=0
    wait "$pid" || exit_code=$?
    
    # Clear spinner line
    printf "\r\033[K"
    
    # Show cursor
    tput cnorm 2>/dev/null || true
    
    if [ $exit_code -eq 0 ]; then
        echo -e "   ${GREEN}✔${RESET} $msg"
    else
        echo -e "   ${RED}✖${RESET} $msg"
        echo -e "\n${RED}Command failed (exit code $exit_code).${RESET}"
        echo -e "${DIM}Tail of log:${RESET}"
        tail -n 10 "$LOG_FILE"
        exit 1
    fi
}

# --- Main Flow ---------------------------------------------------------------

# Initialize log
echo "MOAR Installation Log - $(date)" > "$LOG_FILE"

clear_screen
print_banner

# 1. Pre-flight Checks
step "Checking Environment"

if command -v git &>/dev/null; then
    success "Git is installed"
else
    error "Git is required but not installed."
fi

# Check Docker connectivity explicitly to catch permission errors
if ! command -v docker &>/dev/null; then
    error "Docker is not installed. Please install Docker first."
fi

if ! docker compose version &>/dev/null; then
    warn "Docker Compose check failed. Checking permissions..."
    # Run again without redirection to show error to user
    if ! docker compose version; then
        echo -e "\n${RED}Error: Docker is installed but not working correctly.${RESET}"
        echo -e "${YELLOW}Common fix: Add your user to the docker group:${RESET}"
        echo -e "  sudo usermod -aG docker \$USER"
        echo -e "  newgrp docker"
        exit 1
    fi
else
    success "Docker is installed and running"
fi

# 2. User Input
step "Configuration"

echo -e "   Please enter your domain ${DIM}(e.g. relay.example.com)${RESET}:"
echo -ne "   ${CYAN}➜${RESET} "
read -r MOAR_DOMAIN

if [ -z "$MOAR_DOMAIN" ]; then
    error "Domain is required."
fi

echo -e "\n   Please enter your Admin Pubkey ${DIM}(hex format)${RESET}:"
echo -ne "   ${CYAN}➜${RESET} "
read -r ADMIN_PUBKEY

if [ -z "$ADMIN_PUBKEY" ]; then
    error "Admin pubkey is required."
fi

# 3. Installation
step "Installing MOAR"

info "Target Directory: ${BOLD}$INSTALL_DIR${RESET}"

if [ -d "$INSTALL_DIR" ]; then
    info "Updating existing installation..."
    run_task "Pulling latest changes" git -C "$INSTALL_DIR" pull
else
    run_task "Cloning repository" git clone "$REPO_URL" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"

# Config and Environment
MANAGER_SECRET=$(openssl rand -hex 32)

info "Generating configuration..."
cat > .env <<EOF
MOAR_DOMAIN=${MOAR_DOMAIN}
ADMIN_PUBKEY=${ADMIN_PUBKEY}
RUST_LOG=info
MANAGER_SECRET=${MANAGER_SECRET}
EOF

run_task "Creating data directories" mkdir -p data config pages

step "Building Containers"
info "This process may take a few minutes."
run_task "Running docker compose build" docker compose up -d --build

# Summary
echo -e "\n${GREEN}===========================================${RESET}"
echo -e "   ${BOLD}MOAR Installed Successfully! 🚀${RESET}"
echo -e "${GREEN}===========================================${RESET}"
echo -e ""
echo -e "   ${BOLD}Endpoints:${RESET}"
echo -e "   • Admin UI:    ${CYAN}https://${MOAR_DOMAIN}:8888${RESET}"
echo -e "   • Outbox:      ${CYAN}wss://outbox.${MOAR_DOMAIN}/${RESET}"
echo -e "   • Inbox:       ${CYAN}wss://inbox.${MOAR_DOMAIN}/${RESET}"
echo -e "   • Media:       ${CYAN}https://media.${MOAR_DOMAIN}/${RESET}"
echo -e ""
echo -e "   ${BOLD}Next Steps:${RESET}"
echo -e "   1. Ensure DNS A records for ${BOLD}${MOAR_DOMAIN}${RESET} and ${BOLD}*.${MOAR_DOMAIN}${RESET}"
echo -e "      point to this server."
echo -e "   2. Ensure ports ${BOLD}80${RESET}, ${BOLD}443${RESET}, and ${BOLD}8888${RESET} are open."
echo -e ""
echo -e "   ${DIM}Logs: docker compose logs -f${RESET}"
exit 0
