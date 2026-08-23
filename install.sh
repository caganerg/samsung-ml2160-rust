#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 Series Rust CUPS Filter - Install Script
#
# 1. Checks that the required tools are installed
# 2. Builds the filter (cargo build --release)
# 3. Verifies that nothing outside your control can substitute the artefacts
# 4. Installs the filter binary into /usr/lib/cups/filter/ (requires root),
#    then validates the PPD file (cupstestppd) — in that order, since
#    cupstestppd checks that the file the PPD's cupsFilter/cupsFilter2 lines
#    point to actually exists
# 5. Registers the CUPS queue (requires root)
#
# Usage:
#   ./install.sh <queue-name> <device-uri>
#
#   queue-name  : Name of the CUPS queue to create/update, e.g. ML2160_Rust
#   device-uri  : The printer's CUPS device URI. Find it with:
#
#                     lpinfo -v
#
#                 and copy the URI (second column) of your printer. A USB
#                 printer looks like "usb://Samsung/ML-2165W%20Series?serial=..."
#                 A network/Wi-Fi model (e.g. ML-2165W) that mDNS/Bonjour has
#                 not discovered accepts raw data on the JetDirect port, so use
#                 "socket://<printer-ip>:9100".
#
# Both arguments are REQUIRED and deliberately so. An earlier version of this
# script auto-detected the printer by grepping `lpinfo -v`, but CUPS device
# discovery (mDNS/Bonjour/SNMP over the network, descriptor strings over USB)
# is unauthenticated: any device can advertise itself as a "Samsung ML-216x"
# and get wired up as the print destination, silently redirecting subsequent
# documents to it over unencrypted JetDirect. Guarding that with an on-screen
# confirmation meant rendering attacker-controlled text in the operator's
# terminal, which brought its own escape-sequence spoofing problem. Reading
# `lpinfo -v` yourself keeps the same human review without either hazard.
#
# Note: Do NOT run this script itself as root (the build step runs as the
# normal user); it only asks for `sudo` internally for the two steps that
# write to system paths.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
cd "$SCRIPT_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

FILTER_BIN="target/release/rastertospl-rust"
FILTER_DEST="/usr/lib/cups/filter/rastertospl-rust"
PPD_SRC="$SCRIPT_DIR/ppd/samsung-ml2160.ppd"

usage() {
    echo "Usage: ./install.sh <queue-name> <device-uri>" >&2
    echo >&2
    echo "  Find your printer's device URI with:  lpinfo -v" >&2
    echo "  Then, for example:" >&2
    echo "    ./install.sh ML2160_Rust 'usb://Samsung/ML-2165W%20Series?serial=Z1A2B3C4D5'" >&2
    echo "    ./install.sh ML2165W_Rust 'socket://192.168.1.50:9100'" >&2
    exit 1
}

[[ $# -eq 2 ]] || usage
PRINTER_NAME="$1"
DEVICE_URI="$2"

echo -e "${BOLD}${BLUE}======================================================${NC}"
echo -e "${BOLD}${BLUE} Samsung ML-2160 Rust CUPS Filter - Install ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

if [[ $EUID -eq 0 ]]; then
    echo -e "${RED}ERROR: Do not run this script directly as root (not 'sudo ./install.sh').${NC}"
    echo -e "${RED}It builds the project as the normal user and only invokes sudo${NC}"
    echo -e "${RED}internally when writing to system files.${NC}"
    exit 1
fi

# 0. Argümanları doğrula.
#
# İkisi de operatörün kendi yazdığı değerler, yani güvenilir kabul edilir; ama
# yine de ekrana basılıyor ve `sudo lpadmin`'e argüman olarak geçiyor. Süzmek
# yerine REDDETMEK tercih edildi: kontrol karakteri ya da boşluk içeren bir
# değer meşru bir kuyruk adı/aygıt adresi değildir, dolayısıyla sessizce
# temizlenip kullanılmaktansa net bir hatayla durmak doğru davranış.
#
# Baştaki '-' ayrıca ayrı bir sorun: `lpadmin -p -x` gibi bir çağrıda değer,
# seçenek olarak ayrıştırılabilir. Her iki desen de ilk karakteri harf/rakama
# zorlayarak bunu kapatıyor.
#
# Doğrulama iki aşamalı ve her ikisi de KASITLI olarak `LC_ALL=C` altında,
# yani bayt semantiğiyle çalışıyor. İkisinin de gerekçesi somut bir hatadan
# geliyor:
#
# 1) Yapı denetimi tek başına yetmiyor. `[^[:space:]]` yalnızca boşluğu dışlar,
#    ESC'i (0x1B) memnuniyetle kabul eder — `socket://1.2.3.4<ESC>[2K:9100`
#    ilk yazımda doğrulamadan geçiyordu. Bu yüzden yanına `is_printable_ascii`
#    konuyor: `tr -d '\041-\176'` yazdırılabilir ASCII'nin (0x21-0x7E) tamamını
#    siler, geriye ne kalırsa (kontrol karakteri, boşluk, satır sonu, ASCII dışı
#    bayt) değer reddedilir. Sondaki `printf 'X'` nöbetçisi şart: komut ikamesi
#    sondaki satır sonlarını kırpar, nöbetçi olmasa "socket://x\n" sessizce
#    geçerdi.
#
# 2) Düzenli ifade aralıkları YERELE bağlıdır. Bu, bu makinede birebir görüldü:
#    Türkçe yerelde `i` harfi `[a-z]` aralığına girmediği için `ipp://...`
#    reddediliyordu (`abc://...` kabul edilirken). `[A-Za-z]` da aynı nedenle
#    ASCII dışı harfleri kapsayabilir. Bu yüzden desen eşleştirme bash'in
#    `=~` operatörü yerine `LC_ALL=C grep -qE` ile yapılıyor; C yerelinde
#    aralıklar tanım gereği bayt aralığıdır.
#
# Sıra da önemli: önce bayt denetimi çalışıyor, böylece `grep`'e satır sonu
# içeren bir değer hiç ulaşamıyor (grep satır tabanlıdır ve `^...$` yalnızca
# tek bir satırı doğrular).
#
# Süzmek yerine REDDETMEK tercih edildi: kontrol karakteri içeren bir değer
# meşru bir kuyruk adı/aygıt adresi değildir, sessizce temizlenip
# kullanılmaktansa net bir hatayla durmak doğru davranış.
is_printable_ascii() {
    local rest
    rest="$(printf '%s' "$1" | LC_ALL=C tr -d '\041-\176'; printf 'X')"
    [[ "$rest" == "X" ]]
}

matches_ere() {
    printf '%s' "$1" | LC_ALL=C grep -qE "$2"
}

if ! is_printable_ascii "$PRINTER_NAME" \
   || ! matches_ere "$PRINTER_NAME" '^[A-Za-z0-9][A-Za-z0-9_.-]*$'; then
    echo -e "${RED}ERROR: Invalid queue name: only ASCII letters, digits, '_', '.' and '-' are allowed,${NC}"
    echo -e "${RED}and the name must start with a letter or digit (CUPS rejects spaces, '/' and '#').${NC}"
    exit 1
fi
if (( ${#PRINTER_NAME} > 127 )); then
    echo -e "${RED}ERROR: Queue name is longer than the 127 characters CUPS allows.${NC}"
    exit 1
fi
if ! is_printable_ascii "$DEVICE_URI" \
   || ! matches_ere "$DEVICE_URI" '^[a-z][a-z0-9+.-]*://[!-~]+$'; then
    echo -e "${RED}ERROR: Invalid device URI. Expected <scheme>://<address>, printable ASCII only,${NC}"
    echo -e "${RED}no whitespace or control characters. Example: 'socket://192.168.1.50:9100'${NC}"
    echo -e "${RED}List valid URIs with: lpinfo -v${NC}"
    exit 1
fi

# 1. Check required tools
echo -e "\n${YELLOW}[1/5] Checking required tools...${NC}"
for cmd in cargo lpadmin cupstestppd; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}ERROR: '$cmd' not found. A Rust toolchain and CUPS must be installed.${NC}"
        exit 1
    fi
done
echo -e "${GREEN} -> cargo, lpadmin, cupstestppd are present.${NC}"

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

# 3. Verify that the artefacts cannot be substituted before they are installed
echo -e "\n${YELLOW}[3/5] Verifying that build artefacts are under your control...${NC}"

# `sudo install` bu ikiliyi root olarak sistem yoluna kopyalıyor. Depo dizini
# başkalarınca yazılabilir bir yoldaysa (paylaşımlı mount, geniş izinli /srv),
# derleme ile kopyalama arasında ikili değiştirilip root sahipli bir CUPS
# filtresi olarak kurulabilir. Kopyalamadan önce kaynak zincirini doğrula.
#
# Aynı güvence PPD için de gerekli ve zincire onun yolu da dahil: aşağıdaki
# `sudo lpadmin -P "$PPD_SRC"` PPD'yi root olarak okuyup /etc/cups/ppd/ altına
# kopyalıyor. PPD bir yapılandırma dosyası değil, çalıştırılacak programları
# BELİRLEYEN bir dosyadır: `*cupsFilter`/`*cupsFilter2` satırında mutlak bir
# yol verilebilir ve CUPS bunu onurlandırır, yani değiştirilmiş bir PPD her
# baskı işinde `lp` kullanıcısı olarak keyfi bir program çalıştırır. İkili için
# kapatılan saldırı yolunun aynısı olduğu için aynı denetimden geçiyor.
assert_owned_by_us() {
    local path="$1" owner
    owner="$(stat -c '%u' "$path")" || exit 1
    if [[ "$owner" != "$EUID" && "$owner" != "0" ]]; then
        echo -e "${RED}ERROR: $path is owned by uid $owner (neither you nor root).${NC}"
        echo -e "${RED}Refusing to install a binary or PPD as root from a path you do not control.${NC}"
        exit 1
    fi
}

assert_not_writable_by_others() {
    local path="$1" perms
    assert_owned_by_us "$path"
    perms="$(stat -c '%a' "$path")" || exit 1
    if (( 8#$perms & 8#022 )); then
        echo -e "${RED}ERROR: $path is group- or world-writable (mode $perms).${NC}"
        echo -e "${RED}Another user could swap the filter binary or the PPD before they are${NC}"
        echo -e "${RED}installed as root; a substituted PPD runs arbitrary programs as user 'lp'.${NC}"
        echo -e "${RED}Fix with: chmod go-w $path${NC}"
        exit 1
    fi
}

# Depodaki dosyaların kendi izinlerini denetlemek YETMEZ: bir ÜST dizin
# başkalarınca yazılabilirse, saldırgan bu denetim ile aşağıdaki `sudo`
# çağrıları arasında depo girdisini kendi ağacına bir sembolik bağla
# değiştirebilir (klasik TOCTOU). Sahiplik denetimi tek başına bunu kapatmaz,
# çünkü denetim değişimden ÖNCE çalışmıştır. Bu yüzden zincir kök dizine kadar
# yürünüyor.
#
# Yapışkan (sticky, `t`) bit'i olan dizinler istisna: /tmp gibi 1777 modundaki
# bir dizinde, bir girdiyi yalnızca sahibi yeniden adlandırıp silebilir, yani
# yukarıdaki değiş-tokuş mümkün değildir. Bu istisna sadece ÜST dizinler için
# geçerli; deponun kendisi ve içindekiler yapışkan olsa bile grup/dünya
# yazılabilir olmamalı, çünkü orada yeni dosya oluşturmak da yeterli olabilir.
assert_ancestors_are_safe() {
    local path="$1" parent perms owner
    parent="$(dirname "$path")"
    while :; do
        owner="$(stat -c '%u' "$parent")" || exit 1
        perms="$(stat -c '%a' "$parent")" || exit 1
        if [[ "$owner" != "$EUID" && "$owner" != "0" ]]; then
            echo -e "${RED}ERROR: $parent is owned by uid $owner (neither you nor root).${NC}"
            echo -e "${RED}Its owner can replace the repository directory underneath it between${NC}"
            echo -e "${RED}this check and the 'sudo' steps below. Move the repository somewhere${NC}"
            echo -e "${RED}whose entire path is owned by you or by root.${NC}"
            exit 1
        fi
        if (( 8#$perms & 8#022 )) && ! (( 8#$perms & 8#1000 )); then
            echo -e "${RED}ERROR: $parent is group- or world-writable without the sticky bit (mode $perms).${NC}"
            echo -e "${RED}Another user could swap the repository directory for one of their own${NC}"
            echo -e "${RED}between this check and the 'sudo' steps below, having an arbitrary binary${NC}"
            echo -e "${RED}installed as a root-owned CUPS filter.${NC}"
            echo -e "${RED}Fix with: chmod go-w $parent   (or move the repository elsewhere)${NC}"
            exit 1
        fi
        [[ "$parent" == "/" ]] && break
        parent="$(dirname "$parent")"
    done
}

for p in "$SCRIPT_DIR" \
         "$SCRIPT_DIR/target" "$SCRIPT_DIR/target/release" "$SCRIPT_DIR/$FILTER_BIN" \
         "$SCRIPT_DIR/ppd" "$PPD_SRC"; do
    assert_not_writable_by_others "$p"
done
assert_ancestors_are_safe "$SCRIPT_DIR"
echo -e "${GREEN} -> Repository path, binary and PPD are writable only by you or root.${NC}"

# 4. Install the filter system-wide (requires root), then validate the PPD.
# The order matters: cupstestppd checks that the file referenced by
# cupsFilter/cupsFilter2 actually exists, so the filter must be in place first.
echo -e "\n${YELLOW}[4/5] Installing filter binary: $FILTER_DEST (sudo may be required)${NC}"
sudo install -m 755 -o root -g root "$SCRIPT_DIR/$FILTER_BIN" "$FILTER_DEST"
echo -e "${GREEN} -> Filter installed: $FILTER_DEST${NC}"

echo -e "${BLUE} -> Validating the PPD file (cupstestppd)...${NC}"
if ! cupstestppd "$PPD_SRC"; then
    echo -e "${RED}ERROR: PPD file failed validation: $PPD_SRC${NC}"
    exit 1
fi
echo -e "${GREEN} -> PPD is valid: $PPD_SRC${NC}"

# 5. Create/update the printer queue (requires root)
echo -e "\n${YELLOW}[5/5] Setting up printer queue (sudo may be required)...${NC}"
printf '%b -> Queue : %s\n' "${BLUE}" "$PRINTER_NAME"
printf '%b -> Device: %s%b\n' "${BLUE}" "$DEVICE_URI" "${NC}"
sudo lpadmin -p "$PRINTER_NAME" -E -v "$DEVICE_URI" -P "$PPD_SRC"
printf '%b -> Queue ready: %s%b\n' "${GREEN}" "$PRINTER_NAME" "${NC}"

echo -e "\n${BOLD}${GREEN}Installation complete.${NC}"
lpstat -p "$PRINTER_NAME" 2>/dev/null || true
printf '\nTo send a test print: %blp -d %s <file.pdf>%b\n' "${BOLD}" "$PRINTER_NAME" "${NC}"
