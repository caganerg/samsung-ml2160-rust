#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 Serisi Rust CUPS Filtresi - Kurulum Betiği
#
# 1. Filtreyi derler (cargo build --release)
# 2. PPD dosyasını doğrular (cupstestppd)
# 3. Filtre ikilisini /usr/lib/cups/filter/ altına kurar (root gerekir)
# 4. Bağlı Samsung ML-2160 serisi USB yazıcıyı otomatik bulup CUPS kuyruğu
#    olarak tanımlar (root gerekir)
#
# Kullanım:
#   ./install.sh [kuyruk-adi] [device-uri]
#
#   kuyruk-adi   : Oluşturulacak/güncellenecek CUPS kuyruğunun adı
#                  (varsayılan: ML2160_Rust)
#   device-uri   : Yazıcının CUPS aygıt URI'si (varsayılan: `lpinfo -v`
#                  çıktısından otomatik tespit edilir; USB dışı bağlantılar
#                  için elle verin, örn. bir ağ yazıcısının ipp:// adresi)
#
# Not: Betik kendisi root olarak ÇALIŞTIRILMAMALIDIR (derleme adımı normal
# kullanıcı olarak yapılır); yalnızca sistem dosyalarına yazan iki adım için
# gerektiğinde `sudo` ile parola sorulur.
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
echo -e "${BOLD}${BLUE} Samsung ML-2160 Rust CUPS Filtresi - Kurulum ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

if [[ $EUID -eq 0 ]]; then
    echo -e "${RED}HATA: Bu betiği doğrudan root olarak çalıştırmayın (sudo ./install.sh değil).${NC}"
    echo -e "${RED}Betik, derlemeyi normal kullanıcı olarak yapar ve yalnızca sistem${NC}"
    echo -e "${RED}dosyalarına yazarken kendi içinden sudo ister.${NC}"
    exit 1
fi

# 1. Gerekli araçların kontrolü
echo -e "\n${YELLOW}[1/5] Gerekli araçlar kontrol ediliyor...${NC}"
for cmd in cargo lpadmin lpinfo cupstestppd; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}HATA: '$cmd' bulunamadı. Rust toolchain ve CUPS kurulu olmalı.${NC}"
        exit 1
    fi
done
echo -e "${GREEN} -> cargo, lpadmin, lpinfo, cupstestppd mevcut.${NC}"

if ! systemctl is-active --quiet cups 2>/dev/null; then
    echo -e "${YELLOW} UYARI: CUPS servisi çalışmıyor gibi görünüyor (systemctl is-active cups).${NC}"
fi

# 2. Derleme
echo -e "\n${YELLOW}[2/5] Filtre derleniyor (cargo build --release)...${NC}"
cargo build --release
if [ ! -f "$FILTER_BIN" ]; then
    echo -e "${RED}HATA: $FILTER_BIN bulunamadı! Derleme başarısız.${NC}"
    exit 1
fi
echo -e "${GREEN} -> Filtre ikili dosyası hazır: $FILTER_BIN${NC}"

# 3. PPD doğrulama
echo -e "\n${YELLOW}[3/5] PPD dosyası doğrulanıyor (cupstestppd)...${NC}"
if ! cupstestppd "$PPD_SRC"; then
    echo -e "${RED}HATA: PPD dosyası doğrulamadan geçemedi: $PPD_SRC${NC}"
    exit 1
fi
echo -e "${GREEN} -> PPD geçerli: $PPD_SRC${NC}"

# 4. Filtreyi sisteme kur (root gerekir)
echo -e "\n${YELLOW}[4/5] Filtre ikilisi kuruluyor: $FILTER_DEST (sudo gerekebilir)${NC}"
sudo install -m 755 -o root -g root "$SCRIPT_DIR/$FILTER_BIN" "$FILTER_DEST"
echo -e "${GREEN} -> Filtre kuruldu: $FILTER_DEST${NC}"

# 5. Yazıcı kuyruğunu oluştur/güncelle (root gerekir)
echo -e "\n${YELLOW}[5/5] Yazıcı kuyruğu ayarlanıyor: $PRINTER_NAME${NC}"

if [[ -z "$DEVICE_URI" ]]; then
    echo -e "${BLUE} -> Aygıt URI'si verilmedi, bağlı Samsung ML-2160 serisi USB yazıcı aranıyor...${NC}"
    DEVICE_URI="$(lpinfo -v 2>/dev/null | awk '/direct usb:\/\/Samsung\/ML-216/ {print $2; exit}')"
fi

if [[ -z "$DEVICE_URI" ]]; then
    echo -e "${RED}HATA: Samsung ML-2160 serisi bağlı bir USB yazıcı otomatik bulunamadı.${NC}"
    echo -e "${RED}Yazıcının USB ile bağlı ve açık olduğundan emin olun, ya da aygıt URI'sini${NC}"
    echo -e "${RED}elle belirtin:${NC}"
    echo -e "${RED}  $0 $PRINTER_NAME <device-uri>${NC}"
    echo -e "${RED}Mevcut aygıtları görmek için: lpinfo -v${NC}"
    exit 1
fi
echo -e "${GREEN} -> Aygıt bulundu: $DEVICE_URI${NC}"

sudo lpadmin -p "$PRINTER_NAME" -E -v "$DEVICE_URI" -P "$PPD_SRC"
echo -e "${GREEN} -> Kuyruk hazır: $PRINTER_NAME${NC}"

echo -e "\n${BOLD}${GREEN}Kurulum tamamlandı.${NC}"
lpstat -p "$PRINTER_NAME" 2>/dev/null || true
echo -e "\nTest baskısı için: ${BOLD}lp -d $PRINTER_NAME <dosya.pdf>${NC}"
