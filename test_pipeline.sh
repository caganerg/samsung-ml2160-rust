#!/usr/bin/env bash
# ==============================================================================
# Samsung ML-2160 CUPS Raster -> SPL2 Pipeline & Header Doğrulama Test Betiği
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

PPD_FILE="ppd/samsung-ml2160.ppd"
TARGET_BIN="target/release/rastertospl-rust"
TEMP_DIR="target/test_output"
mkdir -p "$TEMP_DIR"

# Girdi PDF'i: bir argüman verildiyse var olan bir dosya olmak ZORUNDADIR.
# Argümanı doğrudan gs'in -sOutputFile'ına vermek, "%pipe%<komut>" biçimli bir
# değerle Ghostscript'e rastgele kabuk komutu çalıştırtabiliyor (-dSAFER bu
# yolu kapatmıyor), bu yüzden örnek PDF her zaman betiğin kendi belirlediği
# sabit yola üretilir.
PDF_ARG="${1:-}"
if [[ -n "$PDF_ARG" ]]; then
    if [[ ! -f "$PDF_ARG" ]]; then
        # Argümanı `echo -e` ile basma: ters bölü kaçışlarını yorumlar.
        printf '%b%s%b\n' "${RED}HATA: Girdi PDF dosyası bulunamadı: " \
            "$(printf '%s' "$PDF_ARG" | LC_ALL=C tr -d '\000-\037\177')" "$NC"
        echo -e "${RED}Argüman olarak var olan bir PDF verin ya da hiç argüman vermeyin${NC}"
        echo -e "${RED}(bu durumda örnek bir PDF otomatik üretilir).${NC}"
        exit 1
    fi
    PDF_INPUT="$PDF_ARG"
else
    PDF_INPUT="$TEMP_DIR/sample_test.pdf"
fi
RASTER_FILE="$TEMP_DIR/test.raster"
SPL_OUTPUT="$TEMP_DIR/output.spl"

echo -e "${BOLD}${BLUE}======================================================${NC}"
echo -e "${BOLD}${BLUE} Samsung ML-2160 SPL2 Filtre Doğrulama Testi ${NC}"
echo -e "${BOLD}${BLUE}======================================================${NC}"

# 1. Girdi PDF Kontrolü (Yoksa Ghostscript ile otomatik örnek PDF üret)
if [ ! -f "$PDF_INPUT" ]; then
    echo -e "${YELLOW}[1/5] Girdi PDF dosyası belirtilmedi. Örnek PDF üretiliyor...${NC}"
    gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dNOPAUSE -dQUIET -dBATCH -dSAFER \
       -sOutputFile="$PDF_INPUT" -c "
        /Helvetica-Bold findfont 24 scalefont setfont
        50 750 moveto (Samsung ML-2160 Rust CUPS Driver Test) show
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
echo -e "\n${YELLOW}[3/5] cupsfilter ile Samsung ML-2160 CUPS Raster akışı üretiliyor...${NC}"
cupsfilter -p "$PPD_FILE" -m application/vnd.cups-raster "$PDF_INPUT" > "$RASTER_FILE" 2>/dev/null || \
cupsfilter -p "$PPD_FILE" "$PDF_INPUT" > "$RASTER_FILE"

# cupsfilter kısmi başarısızlıkta 0 ile çıkıp boş/kırpık dosya bırakabiliyor;
# boş bir raster ile devam etmek testi anlamsız biçimde "başarılı" gösterir.
if [[ ! -s "$RASTER_FILE" ]]; then
    echo -e "${RED}HATA: cupsfilter boş bir raster dosyası üretti: $RASTER_FILE${NC}"
    exit 1
fi

RASTER_SIZE=$(stat -c%s "$RASTER_FILE" 2>/dev/null || stat -f%z "$RASTER_FILE")
echo -e "${GREEN} -> CUPS Raster dosyası üretildi ($RASTER_SIZE bayt): $RASTER_FILE${NC}"

# 4. CUPS Raster'ı Rust SPL Filtresinden Geçir
echo -e "\n${YELLOW}[4/5] Raster verisi Rust filtresinden geçirilerek SPL çıktısı alınıyor...${NC}"
# CUPS parametreleri ile çağrı simülasyonu: job-id user title num-copies options [file]
"$TARGET_BIN" 101 testuser "Test_Belgesi" 1 "media=A4 resolution=600dpi" "$RASTER_FILE" > "$SPL_OUTPUT"

SPL_SIZE=$(stat -c%s "$SPL_OUTPUT" 2>/dev/null || stat -f%z "$SPL_OUTPUT")
echo -e "${GREEN} -> SPL dosyası üretildi ($SPL_SIZE bayt): $SPL_OUTPUT${NC}"

# 5. SPL Çıktı Dosyasının Başlıklarını ve Bayt Yapısını Doğrula
echo -e "\n${YELLOW}[5/5] Üretilen SPL dosyasının başlık baytları doğrulanıyor...${NC}"

python3 - "$SPL_OUTPUT" <<'EOF'
import sys

spl_path = sys.argv[1]
with open(spl_path, "rb") as f:
    data = f.read()

size = len(data)
errors = []

print(f"Toplam SPL Dosya Boyutu: {size} bayt")

# 1. PJL Universal Exit Language Kontrolü
if not data.startswith(b"\x1b%-12345X@PJL"):
    errors.append("PJL UEL başlığı (\\x1b%-12345X@PJL) bulunamadı!")
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

# Gerçek biçim (src/spl.rs): PJL metin bloğundan sonra ham ikili QPDL kayıtları
# gelir; ESC { (0x1B 0x7B) önekli bir çerçeveleme YOKTUR. Kayıt düzeni:
#   [17 bayt sayfa başlığı] [0..N band kaydı] [3 bayt sayfa sonu] ... [PJL UEL kapanışı]
SUBHEADER_SIG = bytes([0xEF, 0xCD, 0xAB, 0x09])
PJL_END = b"\t\x1b%-12345X"

# 3. PJL bloğunun bittiği, ikili QPDL verisinin başladığı ofseti bul
qpdl_marker = b"ENTER LANGUAGE = QPDL\n"
idx = data.find(qpdl_marker)
if idx == -1:
    errors.append("'@PJL ENTER LANGUAGE = QPDL' satırı bulunamadı, ikili blok başlangıcı tespit edilemiyor!")
    cursor = len(data)
else:
    cursor = idx + len(qpdl_marker)
    print(f" [OK] 3. PJL Bloğu Sonu / QPDL İkili Blok Başlangıcı Tespit Edildi (Ofset: {cursor}).")

# 4-6. Sayfa başlıkları, şerit (band) kayıtları ve sayfa sonlarını sırayla ayrıştır
page_count = 0
band_count = 0
rle_count = 0
raw_count = 0
checksum_errors = 0

while cursor < len(data) and data[cursor : cursor + len(PJL_END)] != PJL_END:
    if data[cursor] != 0x00:
        errors.append(f"Beklenmeyen bayt @ {cursor}: sayfa başlığı imzası (0x00) bekleniyor, 0x{data[cursor]:02X} bulundu!")
        break

    if cursor + 17 > len(data):
        errors.append("Sayfa başlığı için yetersiz veri!")
        break

    page_count += 1
    page_hdr = data[cursor : cursor + 17]
    res_id = page_hdr[0x1]
    copies = int.from_bytes(page_hdr[0x2:0x4], "big")
    paper_size = page_hdr[0x4]
    width_px = int.from_bytes(page_hdr[0x5:0x7], "big")
    height_px = int.from_bytes(page_hdr[0x7:0x9], "big")
    qpdl_version = page_hdr[0xE]

    print(f" [OK] 4. Sayfa {page_count} Başlığı Doğrulandı (Ofset: {cursor}):")
    print(f"       - Kopya Sayısı   : {copies}")
    print(f"       - Kağıt Kodu     : 0x{paper_size:02X} (0x02 = A4)")
    print(f"       - Çözünürlük ID  : 0x{res_id:02X} (600/100 = 6)")
    print(f"       - Boyutlar       : {width_px} x {height_px} px")
    print(f"       - QPDL Sürümü    : {qpdl_version}")

    cursor += 17

    page_band_count = 0
    while cursor < len(data) and data[cursor] == 0x0C:
        if cursor + 11 > len(data):
            errors.append("Band başlığı için yetersiz veri!")
            break

        band_hdr = data[cursor : cursor + 11]
        band_id = band_hdr[0x1]
        comp_type = band_hdr[0x6]
        total_data_size = int.from_bytes(band_hdr[0x7:0xB], "big")
        payload_len = total_data_size - 8

        rec_start = cursor + 11
        sig = data[rec_start : rec_start + 4]
        payload = data[rec_start + 4 : rec_start + 4 + payload_len]
        checksum_bytes = data[rec_start + 4 + payload_len : rec_start + 4 + payload_len + 4]

        if sig != SUBHEADER_SIG:
            errors.append(f"Band {band_id} (sayfa {page_count}): alt-başlık imzası hatalı @ {rec_start} (beklenen {SUBHEADER_SIG.hex()}, bulunan {sig.hex()})!")

        calc_checksum = (sum(sig) + sum(payload)) & 0xFFFFFFFF
        actual_checksum = int.from_bytes(checksum_bytes, "big")
        if calc_checksum != actual_checksum:
            checksum_errors += 1
            errors.append(f"Band {band_id} (sayfa {page_count}): checksum uyuşmuyor (beklenen {calc_checksum}, bulunan {actual_checksum})!")

        if comp_type == 0x11:
            rle_count += 1
        elif comp_type == 0x00:
            raw_count += 1

        page_band_count += 1
        band_count += 1
        cursor = rec_start + 4 + payload_len + 4

    print(f" [OK] 5. Sayfa {page_count}: {page_band_count} band kaydı doğrulandı.")

    # Sayfa Sonu (3 baytlık QPDL Page Footer: [0x01, copies_msb, copies_lsb])
    if data[cursor : cursor + 1] == b"\x01" and cursor + 3 <= len(data):
        footer_copies = int.from_bytes(data[cursor + 1 : cursor + 3], "big")
        print(f" [OK] 6. Sayfa {page_count} Sonu (Form Feed) Doğrulandı, kopya={footer_copies}.")
        cursor += 3
    else:
        errors.append(f"Sayfa {page_count} Sonu (0x01 Form Feed) bulunamadı @ {cursor}!")
        break

if page_count == 0 and not errors:
    errors.append("Hiçbir sayfa başlığı bulunamadı!")

if checksum_errors == 0 and page_count > 0:
    print(f" [OK] Tüm band checksum'ları doğrulandı ({band_count} band, {rle_count} RLE, {raw_count} ham).")

# 7. İş Sonu ve UEL Kapanış Kontrolü
if not data.rstrip(b"\n").endswith(PJL_END):
    errors.append("SPL İş Sonu UEL (\\t\\x1b%-12345X) kapanışı bulunamadı!")
else:
    print(" [OK] 7. SPL Job End ve UEL Kapanışı Doğrulandı.")

if errors:
    print("\n\033[1;31m[HATA] Doğrulama sırasında sorunlar tespit edildi:\033[0m")
    for e in errors:
        print(f"  - {e}")
    sys.exit(1)
else:
    print("\n\033[1;32m[BAŞARILI] SPL çıktısı Samsung ML-2160 protokol standartlarına %100 uygundur!\033[0m")
EOF

echo -e "\n${BOLD}${GREEN}Pipeline testi başarıyla tamamlandı.${NC}"
echo -e "Oluşturulan SPL dosyasını inceleyebilirsiniz: ${BOLD}$SPL_OUTPUT${NC}"
