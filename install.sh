#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────
#  MOAR Installer — Modern TUI
#  Wrapped in main() for `curl | bash` compatibility
# ─────────────────────────────────────────────────────────────

main() {

set -e

REPO_URL="https://github.com/barrydeen/moar.git"
INSTALL_DIR="$HOME/moar"

# ── Colors & Styles ──────────────────────────────────────────

BOLD='\033[1m'
DIM='\033[2m'
ITALIC='\033[3m'
RESET='\033[0m'
RED='\033[1;31m'
GREEN='\033[1;32m'
YELLOW='\033[1;33m'
CYAN='\033[1;36m'
MAGENTA='\033[1;35m'
WHITE='\033[1;37m'
GRAY='\033[0;90m'
BG_RED='\033[41m'
BG_GREEN='\033[42m'
BG_CYAN='\033[46m'
BG_MAGENTA='\033[45m'

# ── Symbols ──────────────────────────────────────────────────

CHECK="${GREEN}✓${RESET}"
CROSS="${RED}✗${RESET}"
ARROW="${CYAN}▸${RESET}"
DOT="${GRAY}·${RESET}"
WARN="${YELLOW}⚠${RESET}"
INFO="${CYAN}ℹ${RESET}"
ROCKET="${MAGENTA}🚀${RESET}"

# ── Drawing Helpers ──────────────────────────────────────────

hr() {
    printf "${GRAY}  ──────────────────────────────────────────────────────${RESET}\n"
}

blank() {
    printf "\n"
}

box_top() {
    printf "${CYAN}  ╭──────────────────────────────────────────────────────╮${RESET}\n"
}

box_mid() {
    printf "${CYAN}  ├──────────────────────────────────────────────────────┤${RESET}\n"
}

box_bot() {
    printf "${CYAN}  ╰──────────────────────────────────────────────────────╯${RESET}\n"
}

box_line() {
    # Pad content to 54 chars
    content="$1"
    len=$(printf '%s' "$content" | sed 's/\x1b\[[0-9;]*m//g' | wc -m)
    pad=$((54 - len))
    if [ "$pad" -lt 0 ]; then pad=0; fi
    spaces=$(printf '%*s' "$pad" '')
    printf "${CYAN}  │${RESET} %b%s ${CYAN}│${RESET}\n" "$content" "$spaces"
}

box_empty() {
    printf "${CYAN}  │${RESET}                                                        ${CYAN}│${RESET}\n"
}

step_start() {
    printf "  ${GRAY}⠸${RESET} %b" "$1"
}

step_done() {
    printf "\r  ${CHECK} %b\n" "$1"
}

step_fail() {
    printf "\r  ${CROSS} %b\n" "$1"
}

step_warn() {
    printf "\r  ${WARN} %b\n" "$1"
}

step_info() {
    printf "  ${INFO} %b\n" "$1"
}

# Spinner character by index (avoids cut -c on multibyte UTF-8)
spin_char() {
    case $1 in
        0) printf '⠋' ;; 1) printf '⠙' ;; 2) printf '⠹' ;; 3) printf '⠸' ;;
        4) printf '⠼' ;; 5) printf '⠴' ;; 6) printf '⠦' ;; 7) printf '⠧' ;;
        8) printf '⠇' ;; 9) printf '⠏' ;;
    esac
}

# Spinner for long-running commands
spin() {
    msg="$1"
    shift

    # Run the command in background
    "$@" > /tmp/moar_install_out 2>&1 &
    pid=$!

    i=0
    while kill -0 "$pid" 2>/dev/null; do
        i=$(( (i + 1) % 10 ))
        char=$(spin_char $i)
        printf "\r  ${CYAN}%s${RESET} %b" "$char" "$msg"
        sleep 0.1
    done

    # Avoid set -e killing us before we can handle the error
    wait "$pid" && status=0 || status=$?

    if [ "$status" -eq 0 ]; then
        step_done "$msg"
    else
        step_fail "$msg"
        blank
        printf "  ${RED}Error output:${RESET}\n"
        sed 's/^/    /' /tmp/moar_install_out
        blank
        rm -f /tmp/moar_install_out
        exit 1
    fi
    rm -f /tmp/moar_install_out
}

# Styled prompt
ask() {
    var_name="$1"
    prompt_text="$2"
    printf "\n  ${ARROW} ${WHITE}%b${RESET}\n" "$prompt_text"
    printf "  ${GRAY}  ❯${RESET} "
    read -r value < /dev/tty
    eval "$var_name=\"\$value\""
}

# ── Banner ───────────────────────────────────────────────────

clear

printf "\n"
printf "${MAGENTA}${BOLD}"
printf "    ╔╦╗╔═╗╔═╗╦═╗\n"
printf "    ║║║║ ║╠═╣╠╦╝\n"
printf "    ╩ ╩╚═╝╩ ╩╩╚═\n"
printf "${RESET}"
printf "${DIM}${GRAY}    Nostr Relay • Docker Installer${RESET}\n"
blank
hr
blank

# ── Pre-flight Checks ───────────────────────────────────────

printf "  ${WHITE}${BOLD}Pre-flight checks${RESET}\n"
blank

# 1. Check if Docker is installed
DOCKER_CMD="docker"
COMPOSE_CMD="docker compose"
NEEDS_SUDO=false

if ! command -v docker >/dev/null 2>&1; then
    step_warn "Docker is ${RED}not installed${RESET}"
    blank
    box_top
    box_line "${WHITE}${BOLD}Docker is required to run MOAR${RESET}"
    box_empty
    box_line "Install Docker automatically?"
    box_line "${DIM}This will run the official Docker install script${RESET}"
    box_line "${DIM}from ${CYAN}get.docker.com${RESET}"
    box_bot
    blank
    printf "  ${ARROW} ${WHITE}Install Docker now? ${GRAY}[Y/n]${RESET} "
    read -r install_docker < /dev/tty
    install_docker="${install_docker:-Y}"
    blank

    if [ "$install_docker" = "Y" ] || [ "$install_docker" = "y" ]; then
        spin "Downloading Docker install script" curl -fsSL https://get.docker.com -o /tmp/get-docker.sh

        printf "\n"
        step_info "Running Docker installer ${DIM}(this may ask for your password)${RESET}"
        blank
        sh /tmp/get-docker.sh
        rm -f /tmp/get-docker.sh
        blank

        if command -v docker >/dev/null 2>&1; then
            step_done "Docker installed successfully"
        else
            step_fail "Docker installation failed"
            printf "\n  ${RED}Please install Docker manually:${RESET}\n"
            printf "  ${CYAN}https://docs.docker.com/engine/install/${RESET}\n\n"
            exit 1
        fi
    else
        blank
        printf "  ${DIM}Install Docker and re-run this script:${RESET}\n"
        printf "  ${CYAN}curl -fsSL https://get.docker.com | sh${RESET}\n\n"
        exit 0
    fi
fi

# 2. Check if Docker daemon is running
if docker info >/dev/null 2>&1; then
    step_done "Docker daemon is ${GREEN}running${RESET}"
else
    # Maybe it's a permissions issue — try with sudo
    if sudo docker info >/dev/null 2>&1; then
        DOCKER_CMD="sudo docker"
        COMPOSE_CMD="sudo docker compose"
        NEEDS_SUDO=true
        step_done "Docker daemon is ${GREEN}running${RESET} ${DIM}(requires sudo)${RESET}"
    else
        step_fail "Docker daemon is ${RED}not running${RESET}"
        blank
        box_top
        box_line "${WHITE}${BOLD}Docker needs to be started${RESET}"
        box_empty
        box_line "Try one of these:"
        box_line "  ${CYAN}sudo systemctl start docker${RESET}"
        box_line "  ${CYAN}sudo service docker start${RESET}"
        box_empty
        box_line "Then re-run this installer."
        box_bot
        blank
        exit 1
    fi
fi

# 3. Check docker compose
if $DOCKER_CMD compose version >/dev/null 2>&1; then
    compose_ver=$($DOCKER_CMD compose version --short 2>/dev/null || echo "unknown")
    step_done "Docker Compose ${DIM}v${compose_ver}${RESET}"
else
    step_fail "Docker Compose ${RED}not available${RESET}"
    blank
    printf "  ${DIM}Docker Compose v2 is required. Update Docker Desktop${RESET}\n"
    printf "  ${DIM}or install the compose plugin:${RESET}\n"
    printf "  ${CYAN}  https://docs.docker.com/compose/install/${RESET}\n\n"
    exit 1
fi

# 4. Check git
if command -v git >/dev/null 2>&1; then
    step_done "Git is available"
else
    step_fail "Git is ${RED}not installed${RESET}"
    printf "\n  ${DIM}Please install git and re-run this script.${RESET}\n\n"
    exit 1
fi

# 5. Show sudo notice if needed
if [ "$NEEDS_SUDO" = true ]; then
    blank
    printf "  ${WARN}  ${YELLOW}Docker requires ${BOLD}sudo${RESET}${YELLOW} on this system${RESET}\n"
    printf "  ${DIM}    All docker commands will be run with sudo.${RESET}\n"
    printf "  ${DIM}    To fix: ${CYAN}sudo usermod -aG docker \$USER${DIM} then log out/in.${RESET}\n"
fi

blank
hr
blank

# ── Configuration ────────────────────────────────────────────

printf "  ${WHITE}${BOLD}Configuration${RESET}\n"

# Domain
while true; do
    ask MOAR_DOMAIN "Enter your domain ${DIM}(e.g. relay.example.com)${RESET}"
    if [ -n "$MOAR_DOMAIN" ]; then
        break
    fi
    printf "  ${RED}  Domain is required${RESET}\n"
done

# Admin pubkey
while true; do
    ask ADMIN_PUBKEY "Enter your admin pubkey ${DIM}(64 char hex)${RESET}"
    if [ -z "$ADMIN_PUBKEY" ]; then
        printf "  ${RED}  Pubkey is required${RESET}\n"
        continue
    fi
    # Basic hex validation
    cleaned=$(printf '%s' "$ADMIN_PUBKEY" | tr -cd '0-9a-fA-F')
    if [ "${#cleaned}" -ne 64 ]; then
        printf "  ${YELLOW}  Expected 64 hex characters, got ${#cleaned}${RESET}\n"
        printf "  ${DIM}    Continue anyway? ${GRAY}[y/N]${RESET} "
        read -r force_pk < /dev/tty
        if [ "$force_pk" = "y" ] || [ "$force_pk" = "Y" ]; then
            break
        fi
        continue
    fi
    break
done

blank
hr
blank

# ── Installation ─────────────────────────────────────────────

printf "  ${WHITE}${BOLD}Installing MOAR${RESET}\n"
blank

# Clone or pull
if [ -d "$INSTALL_DIR" ]; then
    step_info "Directory ${DIM}$INSTALL_DIR${RESET} already exists"
    spin "Pulling latest changes" git -C "$INSTALL_DIR" pull --ff-only
else
    spin "Cloning MOAR" git clone "$REPO_URL" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"

# Generate secrets
MANAGER_SECRET=$(openssl rand -hex 32)
step_done "Generated manager secret"

# Write .env
cat > .env <<EOF
MOAR_DOMAIN=${MOAR_DOMAIN}
ADMIN_PUBKEY=${ADMIN_PUBKEY}
RUST_LOG=info
MANAGER_SECRET=${MANAGER_SECRET}
EOF
step_done "Wrote ${DIM}.env${RESET} configuration"

# Create directories
mkdir -p data config pages
step_done "Created data directories"

blank

# Docker compose up — show live output for this long step
printf "  ${ROCKET} ${WHITE}${BOLD}Building containers${RESET} ${DIM}(this may take a few minutes)${RESET}\n"
blank

# Run with live output so users can see build progress
# Use a subshell + temp file to capture exit status (POSIX-compatible, no PIPESTATUS)
set +e
( $COMPOSE_CMD up -d --build 2>&1; echo $? > /tmp/moar_build_status ) | sed 's/^/    /'
build_status=$(cat /tmp/moar_build_status 2>/dev/null || echo 1)
rm -f /tmp/moar_build_status
set -e

blank
if [ "$build_status" -ne 0 ]; then
    step_fail "Docker build failed ${DIM}(exit code $build_status)${RESET}"
    blank
    printf "  ${DIM}Check the output above for details.${RESET}\n"
    printf "  ${DIM}You can also try: ${CYAN}$COMPOSE_CMD up -d --build${RESET}\n"
    blank
    exit 1
fi
step_done "Containers built and started"

blank
hr
blank

# ── Success ──────────────────────────────────────────────────

printf "${GREEN}${BOLD}"
printf "    ╔╦╗╔═╗╔═╗╦═╗\n"
printf "    ║║║║ ║╠═╣╠╦╝\n"
printf "    ╩ ╩╚═╝╩ ╩╩╚═\n"
printf "${RESET}"
printf "    ${GREEN}${BOLD}is running! 🎉${RESET}\n"
blank

box_top
box_empty
box_line "${WHITE}${BOLD}  Your Endpoints${RESET}"
box_empty
box_line "  ${CYAN}Admin${RESET}   https://${MOAR_DOMAIN}:8888"
box_line "  ${CYAN}Outbox${RESET}   wss://outbox.${MOAR_DOMAIN}/"
box_line "  ${CYAN}Inbox${RESET}    wss://inbox.${MOAR_DOMAIN}/"
box_line "  ${CYAN}Private${RESET}  wss://private.${MOAR_DOMAIN}/"
box_line "  ${CYAN}DMs${RESET}      wss://dm.${MOAR_DOMAIN}/"
box_line "  ${CYAN}Blossom${RESET}  https://blossom.${MOAR_DOMAIN}/"
box_empty
box_mid
box_empty
box_line "${WHITE}${BOLD}  DNS Setup${RESET}"
box_empty
box_line "  Point these to your server's IP address:"
box_line "  ${YELLOW}  ${MOAR_DOMAIN}${RESET}"
box_line "  ${YELLOW}  *.${MOAR_DOMAIN}${RESET}"
box_empty
box_line "  Open ports: ${CYAN}80${RESET}, ${CYAN}443${RESET}, ${CYAN}8888${RESET}"
box_empty
box_mid
box_empty
box_line "${WHITE}${BOLD}  Useful Commands${RESET}"
box_empty
box_line "  ${DIM}View logs${RESET}     $COMPOSE_CMD logs -f"
box_line "  ${DIM}Stop${RESET}          $COMPOSE_CMD down"
box_line "  ${DIM}Restart${RESET}       $COMPOSE_CMD restart"
box_line "  ${DIM}Update${RESET}        git pull && $COMPOSE_CMD up -d --build"
box_empty
box_bot
blank

}

# Run main — must be at the very end for curl|bash to work
main "$@"
