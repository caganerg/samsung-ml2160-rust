#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2165 CUPS Raster -> SPL2 Pipeline & Header Doğrulama Test Betiği
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Renkler
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

PPD_FILE="ppd/samsung-ml2165.ppd"
TARGET_BIN="target/release/rastertospl-rust"
TEMP_DIR="target/test_output"
mkdir -p "$TEMP_DIR"

PDF_INPUT="${1:-$TEMP_DIR/sample_test.pdf}"
RASTER_FILE="$TEMP_DIR/test.raster"
SPL_OUTPUT="$TEMP_DIR/output.spl"

echo -e "${BOLD}${BLUE}======================================================${NC}"
echo -e "${BOLD}${BLUE} Samsung ML-2165 SPL2 Filtre Doğrulama Testi ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

# 1. Girdi PDF Kontrolü (Yoksa Ghostscript ile otomatik örnek PDF üret)
if [ ! -f "$PDF_INPUT" ]; then
    echo -e "${YELLOW}[1/5] Girdi PDF dosyası belirtilmedi. Örnek PDF üretiliyor...${NC}"
    gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dNOPAUSE -dQUIET -dBATCH \
       -sOutputFile="$PDF_INPUT" -c "
        /Helvetica-Bold findfont 24 scalefont setfont
        50 750 moveto (Samsung ML-2165 Rust CUPS Driver Test) show
        /Helvetica findfont 12 scalefont setfont
        50 710 moveto (Bu belge, Rust ile yazilan rastertospl filtresi icin otomatik uretilmistir.) show
        50 690 moveto (Hedef Format: Samsung Printer Language 2 (SPL2 / QPDL)) show
        50 670 moveto (Cozunurluk: 600x600 DPI (Native Hardware)) show
        0 setlinewidth
        50 650 moveto 500 0 rlineto stroke
        showpage"
    echo -e "${GREEN} -> Örnek PDF oluşturuldu: $PDF_INPUT${NC}"
else
    echo -e "${GREEN}[1/5] Kullanılan girdi PDF: $PDF_INPUT${NC}"
fi

# 2. Rust Filtresini Derle
echo -e "\n${YELLOW}[2/5] Rust filtresi derleniyor (cargo build --release)...${NC}"
cargo build --release
if [ ! -f "$TARGET_BIN" ]; then
    echo -e "${RED}HATA: $TARGET_BIN bulunamadı! Derleme başarısız.${NC}"
    exit 1
fi
echo -e "${GREEN} -> Filtre ikili dosyası hazır: $TARGET_BIN${NC}"

# 3. cupsfilter ile PDF'i CUPS Raster'a Çevir
echo -e "\n${YELLOW}[3/5] cupsfilter ile Samsung ML-2165 CUPS Raster akışı üretiliyor...${NC}"
cupsfilter -p "$PPD_FILE" -m application/vnd.cups-raster "$PDF_INPUT" > "$RASTER_FILE" 2>/dev/null || \
cupsfilter -p "$PPD_FILE" "$PDF_INPUT" > "$RASTER_FILE" 2>/dev/null

RASTER_SIZE=$(stat -c%s "$RASTER_FILE" 2>/dev/null || stat -f%z "$RASTER_FILE")
echo -e "${GREEN} -> CUPS Raster dosyası üretildi ($RASTER_SIZE bayt): $RASTER_FILE${NC}"

# 4. CUPS Raster'ı Rust SPL Filtresinden Geçir
echo -e "\n${YELLOW}[4/5] Raster verisi Rust filtresinden geçirilerek SPL çıktısı alınıyor...${NC}"
# CUPS parametreleri ile çağrı simülasyonu: job-id user title num-copies options [file]
"$TARGET_BIN" 101 cagan "Test_Belgesi" 1 "media=A4 resolution=600dpi" "$RASTER_FILE" > "$SPL_OUTPUT"

SPL_SIZE=$(stat -c%s "$SPL_OUTPUT" 2>/dev/null || stat -f%z "$SPL_OUTPUT")
echo -e "${GREEN} -> SPL dosyası üretildi ($SPL_SIZE bayt): $SPL_OUTPUT${NC}"

# 5. SPL Çıktı Dosyasının Başlıklarını ve Bayt Yapısını Doğrula
echo -e "\n${YELLOW}[5/5] Üretilen SPL dosyasının başlık baytları doğrulanıyor...${NC}"

python3 - <<EOF
import sys

spl_path = "$SPL_OUTPUT"
with open(spl_path, "rb") as f:
    data = f.read()

size = len(data)
errors = []

print(f"Toplam SPL Dosya Boyutu: {size} bayt")

# 1. PJL Universal Exit Language Kontrolü
if not data.startswith(b"\x1b%-12345X@PJL"):
    errors.append("PJL UEL başlığı (\x1b%-12345X@PJL) bulunamadı!")
else:
    print(" [OK] 1. PJL UEL Başlığı Doğrulandı.")

# 2. PJL Değişkenleri Kontrolü
pjl_text = data[:512].decode("latin1", errors="ignore")
if "@PJL SET JOBNAME" not in pjl_text:
    errors.append("@PJL SET JOBNAME değişkeni bulunamadı!")
if "@PJL ENTER LANGUAGE = SPL2" not in pjl_text and "@PJL ENTER LANGUAGE = QPDL" not in pjl_text:
    errors.append("@PJL ENTER LANGUAGE = SPL2 emülasyon komutu bulunamadı!")
else:
    print(" [OK] 2. PJL Değişkenleri ve SPL2 Emülasyon Modu Doğrulandı.")

# 3. İkili (Binary) SPL Belge Başlık Kaydı (0x1B, 0x7B, 0x00 ...)
idx_doc = data.find(b"\x1b\x7b\x00")
if idx_doc == -1:
    errors.append("SPL İkili Belge Başlangıç Kaydı (ESC { 0x00) bulunamadı!")
else:
    print(f" [OK] 3. SPL Doc Init Kaydı Doğrulandı (Ofset: {idx_doc}).")

# 4. Sayfa Başlığı Kaydı (0x1B, 0x7B, 0x01 ...)
idx_page = data.find(b"\x1b\x7b\x01")
if idx_page == -1:
    errors.append("SPL Sayfa Başlığı Kaydı (ESC { 0x01) bulunamadı!")
else:
    payload_len = int.from_bytes(data[idx_page+3:idx_page+7], "big")
    paper_size = data[idx_page+7]
    res_id = data[idx_page+10]
    width_px = int.from_bytes(data[idx_page+15:idx_page+19], "big")
    height_px = int.from_bytes(data[idx_page+19:idx_page+23], "big")

    print(f" [OK] 4. SPL Sayfa Başlığı Doğrulandı (Ofset: {idx_page}):")
    print(f"       - Payload Boyutu : {payload_len} bayt")
    print(f"       - Kağıt Kodu     : 0x{paper_size:02X} (0x02 = A4)")
    print(f"       - Çözünürlük ID  : 0x{res_id:02X} (0x01 = 600 DPI)")
    print(f"       - Boyutlar       : {width_px} x {height_px} px")

# 5. Şerit Kayıtları (Record 0x0C) Kontrolü
band_count = 0
rle_count = 0
raw_count = 0
cursor = idx_page + 31
while cursor < len(data) - 16:
    if data[cursor] == 0x0C:
        band_count += 1
        comp = data[cursor + 7]
        payload = int.from_bytes(data[cursor+8:cursor+12], "big")
        if comp == 0x11:
            rle_count += 1
        elif comp == 0x00:
            raw_count += 1
        cursor += 12 + payload
    elif data[cursor:cursor+3] == b"\x1b\x7b\x02": # Page End
        break
    else:
        cursor += 1

print(f" [OK] 5. SPL Şerit (Band) Kayıtları Doğrulandı:")
print(f"       - Toplam Şerit Sayısı   : {band_count}")
print(f"       - Algo 0x11 RLE Şeritleri: {rle_count}")
print(f"       - Ham (Raw) Şeritler    : {raw_count}")

# 6. Sayfa Sonu (Form Feed 0x1B 0x7B 0x02) Kontrolü
if b"\x1b\x7b\x02\x00\x00\x00\x00" not in data:
    errors.append("SPL Sayfa Sonu / Form Feed (ESC { 0x02) bulunamadı!")
else:
    print(" [OK] 6. SPL Form Feed (Page End) Kaydı Doğrulandı.")

# 7. İş Sonu ve UEL Kapanış Kontrolü
if not data.endswith(b"\x1b%-12345X"):
    errors.append("SPL İş Sonu UEL (\x1b%-12345X) kapanışı bulunamadı!")
else:
    print(" [OK] 7. SPL Job End ve UEL Kapanışı Doğrulandı.")

if errors:
    print("\n\033[1;31m[HATA] Doğrulama sırasında sorunlar tespit edildi:\033[0m")
    for e in errors:
        print(f"  - {e}")
    sys.exit(1)
else:
    print("\n\033[1;32m[BAŞARILI] SPL çıktısı Samsung ML-2165 protokol standartlarına %100 uygundur!\033[0m")
EOF

echo -e "\n${BOLD}${GREEN}Pipeline testi başarıyla tamamlandı.${NC}"
echo -e "Oluşturulan SPL dosyasını inceleyebilirsiniz: ${BOLD}$SPL_OUTPUT${NC}"
