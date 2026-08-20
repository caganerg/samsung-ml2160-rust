#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 Series Rust CUPS Filter - Uninstall Script
#
# 1. Auto-detects CUPS queues pointed at a Samsung ML-2160 series device
#    (USB or network/Wi-Fi, e.g. ML-2165W) — or removes the queue name given
#    as an argument — and deletes them
# 2. Removes the filter binary from /usr/lib/cups/filter/
#
# Usage:
#   ./uninstall.sh [queue-name]
#
#   queue-name  : Name of the CUPS queue to remove (default: auto-detected
#                 by matching each queue's device URI against a Samsung
#                 ML-2160 series device; falls back to ML2160_Rust if
#                 nothing is auto-detected)
#
# Note: Do NOT run this script itself as root (not 'sudo ./uninstall.sh');
# it only asks for `sudo` internally for the steps that touch system paths.
# ==============================================================================
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

QUEUE_ARG="${1:-}"
FILTER_DEST="/usr/lib/cups/filter/rastertospl-rust"

echo -e "${BOLD}${BLUE}======================================================${NC}"
echo -e "${BOLD}${BLUE} Samsung ML-2160 Rust CUPS Filter - Uninstall ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

if [[ $EUID -eq 0 ]]; then
    echo -e "${RED}ERROR: Do not run this script directly as root (not 'sudo ./uninstall.sh').${NC}"
    echo -e "${RED}It only invokes sudo internally when touching system files.${NC}"
    exit 1
fi

# 1. Check required tools
echo -e "\n${YELLOW}[1/3] Checking required tools...${NC}"
for cmd in lpadmin lpstat; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}ERROR: '$cmd' not found. CUPS must be installed.${NC}"
        exit 1
    fi
done
echo -e "${GREEN} -> lpadmin, lpstat are present.${NC}"

# 2. Determine which queue(s) to remove
echo -e "\n${YELLOW}[2/3] Locating printer queue(s) to remove...${NC}"

QUEUES=()
if [[ -n "$QUEUE_ARG" ]]; then
    QUEUES=("$QUEUE_ARG")
    echo -e "${BLUE} -> Using queue name given on the command line: $QUEUE_ARG${NC}"
else
    # Matches queues pointed at a USB device ("usb://Samsung/ML-2165W...") as
    # well as network/Wi-Fi devices (e.g. "socket://192.168.1.50:9100" queues
    # won't have the model in the URI, but dnssd-discovered ones like
    # "dnssd://Samsung%20ML-2165W%20Series..." will), so this matches on the
    # device-info text CUPS attaches, not just the URI scheme.
    mapfile -t QUEUES < <(lpstat -v 2>/dev/null | grep -E 'Samsung.*ML-?216[0-9]W?' | awk -F'[ :]' '{print $3}')
    if [[ ${#QUEUES[@]} -eq 0 ]] && lpstat -p ML2160_Rust >/dev/null 2>&1; then
        QUEUES=("ML2160_Rust")
    fi
    if [[ ${#QUEUES[@]} -eq 0 ]]; then
        echo -e "${RED}ERROR: Could not auto-detect a CUPS queue for this printer.${NC}"
        echo -e "${RED}List your queues with 'lpstat -p' and specify the name manually:${NC}"
        echo -e "${RED}  $0 <queue-name>${NC}"
        exit 1
    fi
    echo -e "${GREEN} -> Found queue(s): ${QUEUES[*]}${NC}"
fi

# 3. Remove the queue(s) and the filter binary (requires root)
echo -e "\n${YELLOW}[3/3] Removing queue(s) and filter binary (sudo may be required)...${NC}"
for q in "${QUEUES[@]}"; do
    if lpstat -p "$q" >/dev/null 2>&1; then
        sudo lpadmin -x "$q"
        echo -e "${GREEN} -> Removed queue: $q${NC}"
    else
        echo -e "${YELLOW} -> Queue not found, skipping: $q${NC}"
    fi
done

if [[ -f "$FILTER_DEST" ]]; then
    sudo rm -f "$FILTER_DEST"
    echo -e "${GREEN} -> Removed filter binary: $FILTER_DEST${NC}"
else
    echo -e "${YELLOW} -> Filter binary not found, skipping: $FILTER_DEST${NC}"
fi

echo -e "\n${BOLD}${GREEN}Uninstall complete.${NC}"
