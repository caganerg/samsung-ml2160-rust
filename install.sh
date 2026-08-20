#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 Series Rust CUPS Filter - Install Script
#
# 1. Builds the filter (cargo build --release)
# 2. Installs the filter binary into /usr/lib/cups/filter/ (requires root)
# 3. Validates the PPD file (cupstestppd) — this must happen after the
#    filter is in place, since cupstestppd checks that the file the PPD's
#    cupsFilter/cupsFilter2 lines point to actually exists
# 4. Auto-detects a connected Samsung ML-2160 series printer (USB, or
#    network/Wi-Fi models like ML-2165W if already discovered via
#    mDNS/Bonjour/SNMP) and registers it as a CUPS queue (requires root)
#
# Usage:
#   ./install.sh [queue-name] [device-uri]
#
#   queue-name  : Name of the CUPS queue to create/update (default:
#                 auto-detected from the printer's model, e.g. ML2165W_Rust;
#                 falls back to ML2160_Rust if the model can't be determined)
#   device-uri  : The printer's CUPS device URI (default: auto-detected
#                 from `lpinfo -v` output, USB or network; provide manually
#                 if the printer isn't listed there, e.g. a network/Wi-Fi
#                 printer's socket://<ip>:9100 JetDirect address)
#
# Both arguments are optional — with a single Samsung ML-2160-series
# printer connected (via USB, or over the network and already visible in
# `lpinfo -v`), `./install.sh` alone is enough.
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

PRINTER_NAME_ARG="${1:-}"
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

# 3. Install the filter system-wide (requires root)
echo -e "\n${YELLOW}[3/5] Installing filter binary: $FILTER_DEST (sudo may be required)${NC}"
sudo install -m 755 -o root -g root "$SCRIPT_DIR/$FILTER_BIN" "$FILTER_DEST"
echo -e "${GREEN} -> Filter installed: $FILTER_DEST${NC}"

# 4. Validate PPD (must run after the filter is installed — cupstestppd
# checks that the file referenced by cupsFilter/cupsFilter2 actually exists)
echo -e "\n${YELLOW}[4/5] Validating the PPD file (cupstestppd)...${NC}"
if ! cupstestppd "$PPD_SRC"; then
    echo -e "${RED}ERROR: PPD file failed validation: $PPD_SRC${NC}"
    exit 1
fi
echo -e "${GREEN} -> PPD is valid: $PPD_SRC${NC}"

# 5. Create/update the printer queue (requires root)
echo -e "\n${YELLOW}[5/5] Setting up printer queue...${NC}"

AUTO_DETECTED=0
if [[ -z "$DEVICE_URI" ]]; then
    echo -e "${BLUE} -> No device URI given, searching for a connected Samsung ML-2160 series printer (USB or network)...${NC}"
    # Matches both USB entries ("direct usb://Samsung/ML-2165W...") and
    # network entries discovered via mDNS/Bonjour/SNMP ("network dnssd://
    # Samsung%20ML-2165W%20Series..."), since ML-2165W and similar Wi-Fi
    # models never show up under "direct usb://". Full lines (not just the
    # URI column) are kept so the discovery method ("direct" vs "network")
    # can be reported below.
    mapfile -t FOUND_LINES < <(lpinfo -v 2>/dev/null | grep -E 'Samsung.*ML-?216[0-9]W?')
    if [[ ${#FOUND_LINES[@]} -gt 1 ]]; then
        echo -e "${YELLOW} WARNING: Multiple Samsung ML-216x printers found; using the first one:${NC}"
        printf '   %s\n' "${FOUND_LINES[@]}"
    fi
    DEVICE_URI="$(awk '{print $2}' <<< "${FOUND_LINES[0]:-}")"
    DEVICE_KIND="$(awk '{print $1}' <<< "${FOUND_LINES[0]:-}")"
    AUTO_DETECTED=1
fi

if [[ -z "$DEVICE_URI" ]]; then
    echo -e "${RED}ERROR: Could not auto-detect a connected Samsung ML-2160 series printer.${NC}"
    echo -e "${RED}Make sure the printer is connected via USB and powered on, or, for a network/Wi-Fi${NC}"
    echo -e "${RED}printer (e.g. ML-2165W) not found via mDNS/Bonjour, specify the device URI manually.${NC}"
    echo -e "${RED}These printers accept raw print data on the JetDirect port, so the usual form is:${NC}"
    echo -e "${RED}  $0 ${PRINTER_NAME_ARG:-ML2165W_Rust} socket://<printer-ip>:9100${NC}"
    echo -e "${RED}To list available devices: lpinfo -v${NC}"
    exit 1
fi
echo -e "${GREEN} -> Device found: $DEVICE_URI${NC}"

# Device auto-discovery (mDNS/Bonjour/SNMP for network devices, and in principle
# USB descriptor strings too) is unauthenticated: any device on the network —
# or plugged into USB — can advertise itself as a "Samsung ML-216x" and get
# silently wired up as the print destination, redirecting subsequently
# printed documents to it (JetDirect is unencrypted, unauthenticated plaintext).
# A device URI given explicitly on the command line is operator-trusted and
# skips this; an auto-detected one requires confirmation before use.
if [[ "$AUTO_DETECTED" -eq 1 ]]; then
    if [[ "$DEVICE_KIND" == "direct" ]]; then
        echo -e "${BLUE} -> Discovered via USB (direct physical connection).${NC}"
    else
        echo -e "${YELLOW} -> Discovered over the network (mDNS/Bonjour/SNMP), which is unauthenticated.${NC}"
        echo -e "${YELLOW}    A rogue device on the same network could impersonate a Samsung printer and${NC}"
        echo -e "${YELLOW}    have your print jobs silently redirected to it. Verify the address above.${NC}"
    fi
    echo -e "${BOLD}Set up the print queue using this device? [y/N]${NC}"
    read -r -p "> " CONFIRM || CONFIRM=""
    if [[ ! "$CONFIRM" =~ ^[Yy]([Ee][Ss])?$ ]]; then
        echo -e "${RED}Aborted. Re-run with an explicit device URI if you know the correct address:${NC}"
        echo -e "${RED}  $0 ${PRINTER_NAME_ARG:-ML2165W_Rust} socket://<printer-ip>:9100${NC}"
        exit 1
    fi
fi

if [[ -n "$PRINTER_NAME_ARG" ]]; then
    PRINTER_NAME="$PRINTER_NAME_ARG"
else
    MODEL="$(grep -oE 'ML-?216[0-9]W?' <<< "$DEVICE_URI" | head -n1 | tr -d '-')"
    PRINTER_NAME="${MODEL:-ML2160}_Rust"
    echo -e "${BLUE} -> No queue name given, using auto-detected name: $PRINTER_NAME${NC}"
fi

sudo lpadmin -p "$PRINTER_NAME" -E -v "$DEVICE_URI" -P "$PPD_SRC"
echo -e "${GREEN} -> Queue ready: $PRINTER_NAME${NC}"

echo -e "\n${BOLD}${GREEN}Installation complete.${NC}"
lpstat -p "$PRINTER_NAME" 2>/dev/null || true
echo -e "\nTo send a test print: ${BOLD}lp -d $PRINTER_NAME <file.pdf>${NC}"
