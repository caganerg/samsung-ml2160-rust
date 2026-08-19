#!/usr/bin/env python3
"""
Samsung SPL2 Dosya Karşılaştırma ve Yapısal Analiz Aracı
Orijinal sürücü çıktısı ile Rust sürücü çıktısını kayıt (record) seviyesinde karşılaştırır.
Kullanım: python3 compare_spl.py <orijinal.spl> <rust.spl>
"""

import sys
import os

# Renkler
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BLUE = "\033[94m"
BOLD = "\033[1m"
NC = "\033[0m"

PAPER_MAP = {
    0: "Letter", 1: "Legal", 2: "A4", 3: "Executive",
    4: "Env10", 5: "EnvMonarch", 6: "EnvC5", 7: "EnvDL",
    8: "B5-ISO", 9: "B5-JIS", 10: "Folio", 12: "A5", 13: "A6"
}

COMP_MAP = {
    0x00: "None (Raw)", 0x11: "Algo 0x11 (RLE)", 0x13: "JBIG", 0x15: "JBIG QPDL3"
}

def parse_spl_structure(data, filename):
    report = {
        "filename": filename,
        "size": len(data),
        "pjl_lines": [],
        "binary_offset": -1,
        "doc_header": None,
        "page_header": None,
        "bands": [],
        "page_end_found": False,
        "job_end_found": False,
        "raw_decompressed_pixels": bytearray()
    }

    # 1. PJL Ayrıştırma
    bin_idx = data.find(b"\x1b\x7b")
    if bin_idx == -1:
        bin_idx = len(data)
    
    report["binary_offset"] = bin_idx
    pjl_part = data[:bin_idx].decode("latin1", errors="ignore")
    report["pjl_lines"] = [l.strip() for l in pjl_part.splitlines() if l.strip()]

    # 2. İkili Kayıtları Ayrıştırma
    idx = bin_idx
    while idx < len(data):
        if data[idx:idx+2] == b"\x1b\x7b":
            rec_type = data[idx+2]
            payload_len = int.from_bytes(data[idx+3:idx+7], "big")
            rec_start = idx
            payload = data[idx+7:idx+7+payload_len]
            idx = idx + 7 + payload_len

            if rec_type == 0x00:
                report["doc_header"] = {"offset": rec_start, "payload_len": payload_len, "payload": payload}
            elif rec_type == 0x01:
                if len(payload) >= 24:
                    report["page_header"] = {
                        "offset": rec_start,
                        "paper_id": payload[0],
                        "paper_name": PAPER_MAP.get(payload[0], f"Unknown({payload[0]})"),
                        "source": payload[1],
                        "media_type": payload[2],
                        "resolution_id": payload[3],
                        "resolution_dpi": "600 DPI" if payload[3] == 1 else ("1200 DPI" if payload[3] == 2 else "300 DPI"),
                        "duplex": payload[4],
                        "copies": int.from_bytes(payload[6:8], "big"),
                        "width_px": int.from_bytes(payload[8:12], "big"),
                        "height_px": int.from_bytes(payload[12:16], "big"),
                        "margins": (
                            int.from_bytes(payload[16:18], "big"),
                            int.from_bytes(payload[18:20], "big"),
                            int.from_bytes(payload[20:22], "big"),
                            int.from_bytes(payload[22:24], "big")
                        )
                    }
            elif rec_type == 0x02:
                report["page_end_found"] = True
            elif rec_type == 0x03:
                report["job_end_found"] = True
        elif data[idx] == 0x0C: # Band Header
            band_idx = data[idx+1]
            bytes_per_line = int.from_bytes(data[idx+2:idx+4], "big")
            band_height = int.from_bytes(data[idx+4:idx+6], "big")
            color = data[idx+6]
            comp = data[idx+7]
            payload_len = int.from_bytes(data[idx+8:idx+12], "big")
            band_payload = data[idx+12:idx+12+payload_len]
            
            report["bands"].append({
                "band_idx": band_idx,
                "offset": idx,
                "bytes_per_line": bytes_per_line,
                "band_height": band_height,
                "color": color,
                "compression": comp,
                "comp_name": COMP_MAP.get(comp, f"Unknown(0x{comp:02X})"),
                "payload_len": payload_len,
                "raw_data_size": bytes_per_line * band_height
            })
            idx += 12 + payload_len
        else:
            idx += 1

    return report

def compare_spl_files(file1, file2):
    print(f"{BOLD}{BLUE}======================================================================{NC}")
    print(f"{BOLD}{BLUE} SPL2 Dosya Karşılaştırma ve Uyuşmazlık Raporu{NC}")
    print(f"{BOLD}{BLUE}======================================================================{NC}")

    with open(file1, "rb") as f:
        d1 = f.read()
    with open(file2, "rb") as f:
        d2 = f.read()

    r1 = parse_spl_structure(d1, file1)
    r2 = parse_spl_structure(d2, file2)

    # 1. Genel Dosya Boyutu ve PJL
    print(f"\n{BOLD}1. Dosya Boyutları ve Genel Yapı:{NC}")
    print(f"  Referans ({file1}): {r1['size']} bayt | İkili Başlangıç Ofseti: {r1['binary_offset']}")
    print(f"  Rust     ({file2}): {r2['size']} bayt | İkili Başlangıç Ofseti: {r2['binary_offset']}")

    # 2. Sayfa Başlığı Karşılaştırması
    print(f"\n{BOLD}2. Sayfa Başlığı (Record 0x01 - Page Setup) Karşılaştırması:{NC}")
    p1 = r1.get("page_header")
    p2 = r2.get("page_header")

    if not p1 or not p2:
        print(f"{RED} [HATA] Bir veya her iki dosyada sayfa başlığı (0x01) bulunamadı!{NC}")
    else:
        fields = [
            ("Kağıt Boyutu", p1['paper_name'], p2['paper_name']),
            ("Çözünürlük", p1['resolution_dpi'], p2['resolution_dpi']),
            ("Genişlik (px)", p1['width_px'], p2['width_px']),
            ("Yükseklik (px)", p1['height_px'], p2['height_px']),
            ("Kopya Sayısı", p1['copies'], p2['copies']),
            ("Duplex Modu", p1['duplex'], p2['duplex']),
            ("Kenar Boşlukları (L,T,R,B)", p1['margins'], p2['margins']),
        ]

        mismatch = False
        for name, v1, v2 in fields:
            if v1 == v2:
                print(f"  {GREEN}[UYUŞUYOR]{NC} {name:<25}: {v1}")
            else:
                mismatch = True
                print(f"  {RED}[FARK]{NC}     {name:<25}: Referans={v1} <--> Rust={v2}")

        if not mismatch:
            print(f"  {GREEN}--> Sayfa başlığı parametreleri %100 birebir uyuşuyor!{NC}")

    # 3. Şerit ve Sıkıştırma Karşılaştırması
    print(f"\n{BOLD}3. Şeritler (Record 0x0C - Bands) ve Sıkıştırma Analizi:{NC}")
    b1_len = len(r1["bands"])
    b2_len = len(r2["bands"])
    print(f"  Referans Şerit Sayısı: {b1_len} | Rust Şerit Sayısı: {b2_len}")

    if b1_len > 0 and b2_len > 0:
        total_p1 = sum(b['payload_len'] for b in r1['bands'])
        total_p2 = sum(b['payload_len'] for b in r2['bands'])
        rle_1 = sum(1 for b in r1['bands'] if b['compression'] == 0x11)
        rle_2 = sum(1 for b in r2['bands'] if b['compression'] == 0x11)

        print(f"  Referans Toplam Şerit Verisi: {total_p1} bayt (RLE Şerit: {rle_1}/{b1_len})")
        print(f"  Rust     Toplam Şerit Verisi: {total_p2} bayt (RLE Şerit: {rle_2}/{b2_len})")

        # İlk 3 şeridin karşılaştırmalı özeti
        print(f"\n  İlk Şeritlerin Detayı:")
        for i in range(min(3, b1_len, b2_len)):
            b1 = r1["bands"][i]
            b2 = r2["bands"][i]
            print(f"    Şerit #{i}:")
            print(f"      Ref : SatırBayt={b1['bytes_per_line']}, Yükseklik={b1['band_height']}, Sıkıştırma={b1['comp_name']}, Boyut={b1['payload_len']}b")
            print(f"      Rust: SatırBayt={b2['bytes_per_line']}, Yükseklik={b2['band_height']}, Sıkıştırma={b2['comp_name']}, Boyut={b2['payload_len']}b")

    # 4. Kapanış ve Form Feed Kontrolü
    print(f"\n{BOLD}4. Sayfa Sonu ve İş Kapanış Kontrolü:{NC}")
    print(f"  Referans: PageEnd (0x02) = {r1['page_end_found']}, JobEnd (0x03) = {r1['job_end_found']}")
    print(f"  Rust    : PageEnd (0x02) = {r2['page_end_found']}, JobEnd (0x03) = {r2['job_end_found']}")

    print(f"\n{BOLD}Özet:{NC}")
    if p1 and p2 and p1 == p2 and r1['page_end_found'] and r2['page_end_found']:
        print(f"{GREEN}Protokol başlıkları ve kayıt sıralaması tam uyumlu.{NC}")
    else:
        print(f"{YELLOW}Başlıklar veya kayıt sıralamasında farklılıklar var, yukarıdaki detayları inceleyiniz.{NC}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(f"Kullanım: python3 {sys.argv[0]} <referans.spl> <rust.spl>")
        sys.exit(1)
    compare_spl_files(sys.argv[1], sys.argv[2])
