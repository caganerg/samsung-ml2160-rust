#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 Series Rust CUPS Filter - Install Script
#
# 1. Builds the filter (cargo build --release)
# 2. Validates the PPD file (cupstestppd)
# 3. Installs the filter binary into /usr/lib/cups/filter/ (requires root)
# 4. Auto-detects a connected Samsung ML-2160 series USB printer and
#    registers it as a CUPS queue (requires root)
#
# Usage:
#   ./install.sh [queue-name] [device-uri]
#
#   queue-name  : Name of the CUPS queue to create/update
#                 (default: ML2160_Rust)
#   device-uri  : The printer's CUPS device URI (default: auto-detected
#                 from `lpinfo -v` output; provide manually for non-USB
#                 connections, e.g. a network printer's ipp:// address)
#
# Note: Do NOT run this script itself as root (the build step runs as the
# normal user); it only asks for `sudo` internally for the two steps that
# write to system paths.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

PRINTER_NAME="${1:-ML2160_Rust}"
DEVICE_URI="${2:-}"

FILTER_BIN="target/release/rastertospl-rust"
FILTER_DEST="/usr/lib/cups/filter/rastertospl-rust"
PPD_SRC="$SCRIPT_DIR/ppd/samsung-ml2160.ppd"

echo -e "${BOLD}${BLUE}======================================================${NC}"
echo -e "${BOLD}${BLUE} Samsung ML-2160 Rust CUPS Filter - Install ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

if [[ $EUID -eq 0 ]]; then
    echo -e "${RED}ERROR: Do not run this script directly as root (not 'sudo ./install.sh').${NC}"
    echo -e "${RED}It builds the project as the normal user and only invokes sudo${NC}"
    echo -e "${RED}internally when writing to system files.${NC}"
    exit 1
fi

# 1. Check required tools
echo -e "\n${YELLOW}[1/5] Checking required tools...${NC}"
for cmd in cargo lpadmin lpinfo cupstestppd; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}ERROR: '$cmd' not found. A Rust toolchain and CUPS must be installed.${NC}"
        exit 1
    fi
done
echo -e "${GREEN} -> cargo, lpadmin, lpinfo, cupstestppd are present.${NC}"

if ! systemctl is-active --quiet cups 2>/dev/null; then
    echo -e "${YELLOW} WARNING: The CUPS service does not appear to be running (systemctl is-active cups).${NC}"
fi

# 2. Build
echo -e "\n${YELLOW}[2/5] Building the filter (cargo build --release)...${NC}"
cargo build --release
if [ ! -f "$FILTER_BIN" ]; then
    echo -e "${RED}ERROR: $FILTER_BIN not found! Build failed.${NC}"
    exit 1
fi
echo -e "${GREEN} -> Filter binary ready: $FILTER_BIN${NC}"

# 3. Validate PPD
echo -e "\n${YELLOW}[3/5] Validating the PPD file (cupstestppd)...${NC}"
if ! cupstestppd "$PPD_SRC"; then
    echo -e "${RED}ERROR: PPD file failed validation: $PPD_SRC${NC}"
    exit 1
fi
echo -e "${GREEN} -> PPD is valid: $PPD_SRC${NC}"

# 4. Install the filter system-wide (requires root)
echo -e "\n${YELLOW}[4/5] Installing filter binary: $FILTER_DEST (sudo may be required)${NC}"
sudo install -m 755 -o root -g root "$SCRIPT_DIR/$FILTER_BIN" "$FILTER_DEST"
echo -e "${GREEN} -> Filter installed: $FILTER_DEST${NC}"

# 5. Create/update the printer queue (requires root)
echo -e "\n${YELLOW}[5/5] Setting up printer queue: $PRINTER_NAME${NC}"

if [[ -z "$DEVICE_URI" ]]; then
    echo -e "${BLUE} -> No device URI given, searching for a connected Samsung ML-2160 series USB printer...${NC}"
    DEVICE_URI="$(lpinfo -v 2>/dev/null | awk '/direct usb:\/\/Samsung\/ML-216/ {print $2; exit}')"
fi

if [[ -z "$DEVICE_URI" ]]; then
    echo -e "${RED}ERROR: Could not auto-detect a connected Samsung ML-2160 series USB printer.${NC}"
    echo -e "${RED}Make sure the printer is connected via USB and powered on, or specify the${NC}"
    echo -e "${RED}device URI manually:${NC}"
    echo -e "${RED}  $0 $PRINTER_NAME <device-uri>${NC}"
    echo -e "${RED}To list available devices: lpinfo -v${NC}"
    exit 1
fi
echo -e "${GREEN} -> Device found: $DEVICE_URI${NC}"

sudo lpadmin -p "$PRINTER_NAME" -E -v "$DEVICE_URI" -P "$PPD_SRC"
echo -e "${GREEN} -> Queue ready: $PRINTER_NAME${NC}"

echo -e "\n${BOLD}${GREEN}Installation complete.${NC}"
lpstat -p "$PRINTER_NAME" 2>/dev/null || true
echo -e "\nTo send a test print: ${BOLD}lp -d $PRINTER_NAME <file.pdf>${NC}"
