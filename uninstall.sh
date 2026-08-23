#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 Series Rust CUPS Filter - Uninstall Script
#
# 1. Removes the named CUPS queue
# 2. Removes the filter binary from /usr/lib/cups/filter/, but only once no
#    installed PPD references it any more
#
# Usage:
#   ./uninstall.sh <queue-name>
#
#   queue-name  : Name of the CUPS queue to remove. List your queues with:
#
#                     lpstat -p
#
# The queue name is REQUIRED. An earlier version auto-detected queues by
# grepping `lpstat -v` for a broad "Samsung.*ML-216x" pattern, which could
# match — and silently delete — an unrelated queue whose device-info text
# happened to contain that substring, and which had to render that untrusted
# text in the operator's terminal to ask for confirmation. Naming the queue
# yourself removes both problems, and is also more reliable: a queue created
# against a plain "socket://<ip>:9100" address carries no model name for such
# a pattern to match in the first place.
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

FILTER_DEST="/usr/lib/cups/filter/rastertospl-rust"
PPD_DIR="/etc/cups/ppd"

usage() {
    echo "Usage: ./uninstall.sh <queue-name>" >&2
    echo >&2
    echo "  List your queues with:  lpstat -p" >&2
    echo "  Then, for example:" >&2
    echo "    ./uninstall.sh ML2160_Rust" >&2
    exit 1
}

[[ $# -eq 1 ]] || usage
QUEUE="$1"

echo -e "${BOLD}${BLUE}======================================================${NC}"
echo -e "${BOLD}${BLUE} Samsung ML-2160 Rust CUPS Filter - Uninstall ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

if [[ $EUID -eq 0 ]]; then
    echo -e "${RED}ERROR: Do not run this script directly as root (not 'sudo ./uninstall.sh').${NC}"
    echo -e "${RED}It only invokes sudo internally when touching system files.${NC}"
    exit 1
fi

# Kuyruk adı doğrulaması install.sh ile birebir aynı gerekçelere dayanıyor:
# yerelden bağımsız bayt denetimi (`tr` + nöbetçi) artı C yerelinde desen
# eşleştirme (`grep -qE`), çünkü bash'in `=~` operatöründeki `[A-Za-z]` gibi
# aralıklar yerele bağlıdır — Türkçe yerelde `i` harfi `[a-z]` aralığına
# girmez. Ayrıntılı gerekçe için install.sh'taki aynı bloğa bakın.
is_printable_ascii() {
    local rest
    rest="$(printf '%s' "$1" | LC_ALL=C tr -d '\041-\176'; printf 'X')"
    [[ "$rest" == "X" ]]
}

matches_ere() {
    printf '%s' "$1" | LC_ALL=C grep -qE "$2"
}

if ! is_printable_ascii "$QUEUE" \
   || ! matches_ere "$QUEUE" '^[A-Za-z0-9][A-Za-z0-9_.-]*$'; then
    echo -e "${RED}ERROR: Invalid queue name: only ASCII letters, digits, '_', '.' and '-' are allowed,${NC}"
    echo -e "${RED}and the name must start with a letter or digit.${NC}"
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

# 2. Remove the queue (requires root)
echo -e "\n${YELLOW}[2/3] Removing printer queue (sudo may be required)...${NC}"
if lpstat -p "$QUEUE" >/dev/null 2>&1; then
    sudo lpadmin -x "$QUEUE"
    printf '%b -> Removed queue: %s%b\n' "${GREEN}" "$QUEUE" "${NC}"
else
    printf '%b -> Queue not found, nothing to remove: %s%b\n' "${YELLOW}" "$QUEUE" "${NC}"
fi

# 3. Remove the filter binary, but only if nothing still uses it
echo -e "\n${YELLOW}[3/3] Removing filter binary if no queue still needs it...${NC}"

# Filtre ikilisi bu sürücüyü kullanan TÜM kuyruklarca paylaşılıyor. Tek bir
# kuyruk kaldırıldığında ikiliyi de silmek, geride kalan kuyrukları sessizce
# bozar: işleri "filter failed" ile başarısız olur.
#
# Kalan kullanıcıları KUYRUK ADRESİNDEN değil, kurulu PPD'lerden tespit
# ediyoruz. Önceki sürüm `lpstat -v` çıktısını "Samsung.*ML-216x" desenine
# karşı sayıyordu; bu, `socket://<ip>:9100` ile kurulmuş bir kuyruğu (adreste
# model adı geçmez) hiç göremez ve ikiliyi hâlâ kullanan bir kuyruk varken
# silinmesine yol açardı. CUPS her kuyruk için PPD'yi /etc/cups/ppd/<ad>.ppd
# olarak saklıyor ve filtreyi çağıran satır (`*cupsFilter`/`*cupsFilter2`)
# orada; dolayısıyla doğru soru "hangi PPD bu ikiliyi anıyor" sorusudur.
# `lpadmin -x` kuyruğun PPD'sini zaten sildiği için bu sayım, az önce
# kaldırılan kuyruğu içermez.
#
# Sondaki `|| true` şart: eşleşme bulunmadığında `grep` 1, glob hiç açılmadığında
# 2 ile çıkar ve `set -o pipefail` bunu boru hattının durumu yapar — yani
# `set -e` betiği tam da ikilinin SİLİNMESİ gereken durumda sonlandırırdı.
REMAINING=0
if [[ -d "$PPD_DIR" ]]; then
    REMAINING="$( { sudo grep -lsF -- 'rastertospl-rust' "$PPD_DIR"/*.ppd 2>/dev/null || true; } | wc -l)"
fi

if (( REMAINING > 0 )); then
    echo -e "${YELLOW} -> $REMAINING other queue(s) still use this driver; keeping the shared${NC}"
    echo -e "${YELLOW}    filter binary so they keep working: $FILTER_DEST${NC}"
    echo -e "${YELLOW}    Remove those queues too, then re-run this script to delete it.${NC}"
elif [[ -f "$FILTER_DEST" ]]; then
    sudo rm -f "$FILTER_DEST"
    echo -e "${GREEN} -> Removed filter binary: $FILTER_DEST${NC}"
else
    echo -e "${YELLOW} -> Filter binary not found, skipping: $FILTER_DEST${NC}"
fi

echo -e "\n${BOLD}${GREEN}Uninstall complete.${NC}"
