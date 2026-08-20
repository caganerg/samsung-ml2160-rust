//! # Samsung Printer Language (SPL2 / QPDL v3) Protokol Modülü
//!
//! Samsung ML-2160 serisi ve uyumlu monokrom QPDL/SPL2 lazer yazıcılar için
//! tam uyumlu PJL iş kontrolü, 17-baytlık sayfa başlığı, 0x09ABCDEF alt-başlıklı
//! ve sağlama toplamlı (checksum) Algo 0x11 RLE şerit kodlaması.
//!
//! Kaynak: OpenPrinting SpliX (QPDL v3 / ML-2160 serisi)
//! Lisans: GPLv2 (yalnızca v2 — SpliX kaynağıyla aynı)

use std::io::{self, Write};

/// PJL Universal Exit Language (UEL)
pub const PJL_UEL: &[u8] = b"\x1b%-12345X";
pub const PJL_END: &[u8] = b"\t\x1b%-12345X";

/// Alt Başlık İmzası (Sub-header Signature: 0x09ABCDEF - Little Endian)
pub const SUBHEADER_SIG_LE: [u8; 4] = [0xEF, 0xCD, 0xAB, 0x09];

/// Algo 0x11 RLE Sıkıştırma Sabitleri
pub const COMPRESS_SAMPLE_RATE: usize = 0x800; // 2048 bayt
pub const TABLE_PTR_SIZE: usize = 0x40;        // 64 adet işaretçi/ofset
pub const MAX_UNCOMPRESSED_BYTES: usize = 0x80; // 128 bayt
pub const MIN_COMPRESSED_BYTES: usize = 2;     // > 2 bayt (en az 3 bayt eşleşme)
pub const MAX_COMPRESSED_BYTES: usize = 0x1FF + 3; // 514 bayt
pub const COMPRESSION_FLAG: u8 = 0x80;

/// Samsung QPDL Kağıt Boyutu Tanımları
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SplPaperSize {
    Letter = 0,
    Legal = 1,
    #[default]
    A4 = 2,
    Executive = 3,
    Ledger = 4,
    A3 = 5,
    Env10 = 6,
    Monarch = 7,
    C5 = 8,
    Dl = 9,
    B4 = 10,
    B5 = 11,
    EnvIsoB5 = 12,
    Postcard = 14,
    A5 = 16,
    A6 = 17,
    B6 = 18,
    Custom = 21,
    C6 = 23,
    Folio = 24,
    EnvPersonal = 25,
    Env9 = 26,
    Oficio = 28,
}

impl SplPaperSize {
    pub fn from_dimensions_pt(width_pt: u32, height_pt: u32) -> Self {
        match (width_pt, height_pt) {
            (595, 842) | (842, 595) => SplPaperSize::A4,
            (612, 792) | (792, 612) => SplPaperSize::Letter,
            (612, 1008) | (1008, 612) => SplPaperSize::Legal,
            (420, 595) | (595, 420) => SplPaperSize::A5,
            (297, 420) | (420, 297) => SplPaperSize::A6,
            (522, 756) | (756, 522) => SplPaperSize::Executive,
            (612, 936) | (936, 612) => SplPaperSize::Folio,
            (516, 729) | (729, 516) => SplPaperSize::B5,
            (297, 684) | (684, 297) => SplPaperSize::Env10,
            (312, 624) | (624, 312) => SplPaperSize::Dl,
            (459, 649) | (649, 459) => SplPaperSize::C5,
            _ => SplPaperSize::A4,
        }
    }
}

/// Kağıt Kaynağı (Tray)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SplPaperSource {
    #[default]
    Auto = 1,
    Manual = 2,
    Multi = 3,
    Upper = 4,
    Lower = 5,
}

/// Çözünürlük
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplResolution {
    Dpi300,
    #[default]
    Dpi600,
    Dpi1200,
}

impl SplResolution {
    #[inline]
    pub fn dpi(&self) -> u32 {
        match self {
            SplResolution::Dpi300 => 300,
            SplResolution::Dpi600 => 600,
            SplResolution::Dpi1200 => 1200,
        }
    }

    pub fn from_dpi(dpi: u32) -> Self {
        if dpi >= 1200 {
            SplResolution::Dpi1200
        } else if dpi >= 600 {
            SplResolution::Dpi600
        } else {
            SplResolution::Dpi300
        }
    }
}

/// Duplex Modu
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplDuplex {
    #[default]
    Simplex,
    LongEdge,
    ShortEdge,
}

/// Şerit Sıkıştırma Algoritması
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SplCompression {
    #[default]
    None = 0x00,
    Rle = 0x11,
}

/// İş Konfigürasyonu
#[derive(Debug, Clone)]
pub struct JobConfig {
    pub job_name: String,
    pub user_name: String,
    pub service_date: String,
    pub duplex: SplDuplex,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            job_name: "CUPS Document".to_string(),
            user_name: "cagan".to_string(),
            service_date: "20120101".to_string(),
            duplex: SplDuplex::Simplex,
        }
    }
}

/// Sayfa Konfigürasyonu
#[derive(Debug, Clone)]
pub struct PageConfig {
    pub paper_size: SplPaperSize,
    pub paper_source: SplPaperSource,
    pub resolution: SplResolution,
    pub duplex: SplDuplex,
    pub copies: u16,
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub qpdl_version: u8,
}

impl Default for PageConfig {
    fn default() -> Self {
        Self {
            paper_size: SplPaperSize::A4,
            paper_source: SplPaperSource::Auto,
            resolution: SplResolution::Dpi600,
            duplex: SplDuplex::Simplex,
            copies: 1,
            width_pixels: 4768,
            height_pixels: 6816,
            qpdl_version: 3,
        }
    }
}

// ============================================================================
// Algo 0x11 RLE Sıkıştırıcı (SpliX / Samsung QPDL Uyumlu)
// ============================================================================

pub struct Algo0x11;

impl Algo0x11 {
    pub fn lookup_best_offsets(data: &[u8]) -> [u16; TABLE_PTR_SIZE] {
        let mut occurrences = [(0u32, 0usize); COMPRESS_SAMPLE_RATE];
        for i in 0..COMPRESS_SAMPLE_RATE {
            occurrences[i] = (0, i);
        }

        if data.len() >= COMPRESS_SAMPLE_RATE {
            let mut i = COMPRESS_SAMPLE_RATE;
            while i < data.len() {
                let b = data[i];
                let max_j = COMPRESS_SAMPLE_RATE.min(i);
                for j in 1..max_j {
                    if data[i - j] == b {
                        occurrences[j - 1].0 += 1;
                    }
                }
                i += COMPRESS_SAMPLE_RATE;
            }
        } else {
            for i in 1..data.len() {
                let b = data[i];
                let max_j = COMPRESS_SAMPLE_RATE.min(i);
                for j in 1..max_j {
                    if data[i - j] == b {
                        occurrences[j - 1].0 += 1;
                    }
                }
            }
        }

        occurrences.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        let mut table = [1u16; TABLE_PTR_SIZE];
        let mut one_present = false;

        for i in 0..TABLE_PTR_SIZE {
            let offset = (occurrences[i].1 + 1) as u16;
            table[i] = offset;
            if offset == 1 {
                one_present = true;
            }
        }

        if !one_present {
            table[TABLE_PTR_SIZE - 1] = 1;
        }

        table
    }

    pub fn compress(data: &[u8]) -> Option<Vec<u8>> {
        if data.is_empty() {
            return None;
        }

        let ptr_array = Self::lookup_best_offsets(data);
        let max_output_size = data.len() + 256;
        let mut out = Vec::with_capacity(max_output_size);

        // 1. uncompressed_initial_size için 4 bayt yer aç
        out.extend_from_slice(&[0u8; 4]);

        // 2. 64 adet u16 ofset tablosu
        let mut max_offset: usize = 0;
        for &ptr in &ptr_array {
            out.extend_from_slice(&ptr.to_le_bytes());
            if (ptr as usize) > max_offset {
                max_offset = ptr as usize;
            }
        }

        // 3. İlk ham baytlar
        let mut uncomp_size = max_offset.min(MAX_UNCOMPRESSED_BYTES).min(data.len());
        if uncomp_size == 0 {
            uncomp_size = 1.min(data.len());
        }

        let uncomp_size_u32 = uncomp_size as u32;
        out[0..4].copy_from_slice(&uncomp_size_u32.to_le_bytes());

        for i in 0..uncomp_size {
            out.push(data[i]);
        }

        let mut r = uncomp_size;
        let mut raw_data_counter: usize = 0;
        let mut raw_data_counter_ptr: usize = 0;

        while r < data.len() {
            let max_comp_size = (data.len() - r).min(MAX_COMPRESSED_BYTES);

            if max_comp_size >= 3 {
                let mut best_comp_counter: usize = 0;
                let mut best_ptr: usize = 0;

                for (i, &offset) in ptr_array.iter().enumerate() {
                    let off = offset as usize;
                    if off > r {
                        continue;
                    }
                    let r_ref = r - off;
                    let mut counter = 0;
                    while counter < max_comp_size && data[r + counter] == data[r_ref + counter] {
                        counter += 1;
                    }
                    if counter > best_comp_counter {
                        best_comp_counter = counter;
                        best_ptr = i;
                        if counter == max_comp_size {
                            break;
                        }
                    }
                }

                if best_comp_counter >= 3 {
                    if raw_data_counter > 0 {
                        out[raw_data_counter_ptr] = (raw_data_counter - 1) as u8;
                        raw_data_counter = 0;
                    }

                    r += best_comp_counter;
                    let comp_len = (best_comp_counter - 3) as u16;

                    let byte0 = COMPRESSION_FLAG | ((comp_len & 0x7F) as u8);
                    let byte1 = (((comp_len >> 1) & 0xC0) as u8) | ((best_ptr & 0x3F) as u8);

                    out.push(byte0);
                    out.push(byte1);
                    continue;
                }
            }

            raw_data_counter += 1;
            if raw_data_counter == 1 {
                raw_data_counter_ptr = out.len();
                out.push(0);
            } else if raw_data_counter == MAX_UNCOMPRESSED_BYTES {
                out[raw_data_counter_ptr] = 0x7F;
                raw_data_counter = 0;
            }

            out.push(data[r]);
            r += 1;
        }

        if raw_data_counter > 0 {
            out[raw_data_counter_ptr] = (raw_data_counter - 1) as u8;
        }

        Some(out)
    }

    pub fn calculate_checksum(data: &[u8]) -> u32 {
        let mut sum: u32 = 0;
        for &b in data {
            sum = sum.wrapping_add(b as u32);
        }
        sum
    }

    /// `compress`'in ürettiği akışı geri çözer (test/teşhis amaçlı).
    /// Format spec'inin (bkz. gerçek SpliX algo0x11.cpp) ters uygulamasıdır.
    #[cfg(test)]
    pub fn decompress(data: &[u8]) -> Vec<u8> {
        let uncomp_size = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let mut ptr_array = [0u16; TABLE_PTR_SIZE];
        for i in 0..TABLE_PTR_SIZE {
            let off = 4 + i * 2;
            ptr_array[i] = u16::from_le_bytes(data[off..off + 2].try_into().unwrap());
        }
        let table_end = 4 + TABLE_PTR_SIZE * 2;
        let mut out = Vec::new();
        out.extend_from_slice(&data[table_end..table_end + uncomp_size]);

        let mut pos = table_end + uncomp_size;
        while pos < data.len() {
            let b0 = data[pos];
            if b0 & COMPRESSION_FLAG != 0 {
                let b1 = data[pos + 1];
                let comp_len = ((b0 & 0x7F) as u16) | ((((b1 >> 6) & 0x3) as u16) << 7);
                let ptr_idx = (b1 & 0x3F) as usize;
                let offset = ptr_array[ptr_idx] as usize;
                let match_len = comp_len as usize + 3;
                let mut ref_pos = out.len() - offset;
                for _ in 0..match_len {
                    let b = out[ref_pos];
                    out.push(b);
                    ref_pos += 1;
                }
                pos += 2;
            } else {
                let count = (b0 as usize) + 1;
                pos += 1;
                out.extend_from_slice(&data[pos..pos + count]);
                pos += count;
            }
        }
        out
    }
}

// ============================================================================
// SPL2 / QPDL Akış Yöneticisi (SplStreamWriter)
// ============================================================================

/// PJL akışına gömülecek serbest metin alanlarını (JOBNAME, USERNAME vb.) güvenli hale getirir.
///
/// PJL satır tabanlıdır ve alıntılanmış (`"..."`) dizeleri için bir kaçış
/// (escape) mekanizması yoktur: gömülen bir CR/LF ya da ESC (Universal Exit
/// Language'ın başlangıcı) baytı, alanı erken sonlandırıp ardından gelen
/// metnin yazıcı tarafından tamamen yeni, keyfi bir PJL komutu/işi olarak
/// yorumlanmasına yol açabilir. `job_name`/`user_name` CUPS iş başlığından
/// (dolayısıyla işi gönderen istemciden) geldiği için güvenilmez kabul
/// edilmeli. Kaçış mekanizması olmadığından, izin verilmeyen baytları
/// (tüm kontrol karakterleri) ve alıntılanmış alanları bozmamak için çift
/// tırnağı kaçırmak yerine tamamen kaldırıyoruz.
fn sanitize_pjl_field(input: &str) -> String {
    input.chars().filter(|c| !c.is_control() && *c != '"').collect()
}

pub struct SplStreamWriter<W: Write> {
    writer: W,
    current_band: u8,
}

impl<W: Write> SplStreamWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            current_band: 0,
        }
    }

    /// Samsung ML-2160 serisi PJL Başlığını gönderir.
    pub fn begin_job(&mut self, config: &JobConfig) -> io::Result<()> {
        // Gerçek SpliX (printer.cpp sendPJLHeader) sırası: UEL'den sonra doğrudan
        // "@PJL DEFAULT SERVICEDATE=..." ile başlar; ayrı bir çıplak "@PJL\n" satırı
        // GÖNDERİLMEZ. PowerSave/JamRecovery satırları da ml2160.ppd varsayılanlarına
        // göre (PowerSave=5, JamRecovery=False) her zaman gönderilir.
        let mut pjl = Vec::with_capacity(256);
        pjl.extend_from_slice(PJL_UEL);
        pjl.extend_from_slice(
            format!(
                "@PJL DEFAULT SERVICEDATE={}\n",
                sanitize_pjl_field(&config.service_date)
            )
            .as_bytes(),
        );
        pjl.extend_from_slice(
            format!(
                "@PJL SET USERNAME=\"{}\"\n",
                sanitize_pjl_field(&config.user_name)
            )
            .as_bytes(),
        );
        pjl.extend_from_slice(
            format!(
                "@PJL SET JOBNAME=\"{}\"\n",
                sanitize_pjl_field(&config.job_name)
            )
            .as_bytes(),
        );
        pjl.extend_from_slice(b"@PJL DEFAULT POWERSAVE=ON\n");
        pjl.extend_from_slice(b"@PJL DEFAULT POWERSAVETIME=5\n");
        pjl.extend_from_slice(b"@PJL SET JAMRECOVERY=OFF\n");

        // Gerçek SpliX (printer.cpp sendPJLHeader) duplex durumuna göre
        // DUPLEX=ON/OFF ve (açıksa) BINDING=LONGEDGE/SHORTEDGE gönderir.
        match config.duplex {
            SplDuplex::Simplex => {
                pjl.extend_from_slice(b"@PJL SET DUPLEX=OFF\n");
            }
            SplDuplex::LongEdge => {
                pjl.extend_from_slice(b"@PJL SET DUPLEX=ON\n");
                pjl.extend_from_slice(b"@PJL SET BINDING=LONGEDGE\n");
            }
            SplDuplex::ShortEdge => {
                pjl.extend_from_slice(b"@PJL SET DUPLEX=ON\n");
                pjl.extend_from_slice(b"@PJL SET BINDING=SHORTEDGE\n");
            }
        }

        pjl.extend_from_slice(b"@PJL SET PAPERTYPE=OFF\n");
        pjl.extend_from_slice(b"@PJL SET ALTITUDE=LOW\n");
        pjl.extend_from_slice(b"@PJL SET DENSITY=3\n");
        pjl.extend_from_slice(b"@PJL SET RET=NORMAL\n");
        pjl.extend_from_slice(b"@PJL ENTER LANGUAGE = QPDL\n");

        self.writer.write_all(&pjl)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Samsung QPDL 17-Baytlık Sayfa Başlığını Gönderir.
    pub fn begin_page(&mut self, config: &PageConfig) -> io::Result<()> {
        self.current_band = 0;

        let mut header = [0u8; 17];
        header[0x0] = 0x00; // Sayfa Başlığı İmzası
        header[0x1] = (config.resolution.dpi() / 100) as u8; // 6 (600 DPI)
        header[0x2] = (config.copies >> 8) as u8;
        header[0x3] = (config.copies & 0xFF) as u8;
        header[0x4] = config.paper_size as u8; // A4 = 2
        header[0x5] = (config.width_pixels >> 8) as u8;
        header[0x6] = (config.width_pixels & 0xFF) as u8;
        header[0x7] = (config.height_pixels >> 8) as u8;
        header[0x8] = (config.height_pixels & 0xFF) as u8;
        header[0x9] = config.paper_source as u8; // Auto = 1
        header[0xA] = 0x00; // unknownByte1
        header[0xB] = match config.duplex {
            SplDuplex::Simplex => 1,
            _ => 0,
        };
        header[0xC] = 0x00; // tumble = 0
        header[0xD] = 0x00; // unknownByte2
        header[0xE] = config.qpdl_version; // 3
        header[0xF] = 0x01; // Colorplanes = 1 (Monokrom)
        header[0x10] = (config.resolution.dpi() / 100) as u8; // 6 (600 DPI)

        self.writer.write_all(&header)?;
        Ok(())
    }

    /// Raster şeridini Algo 0x11 RLE sıkıştırması, alt-başlık ve checksum ile yazar.
    ///
    /// `band_width_pixels`: Yazıcı DMA motorunun satır atlama genişliği.
    /// Satır kaymasını (zebra / merdiven deseni) önlemek için mutlaka tam bayt hizalı `bytes_per_line * 8` olmalıdır!
    pub fn write_compressed_band(
        &mut self,
        band_width_pixels: u16,
        band_height_lines: u16,
        raw_bitmap: &[u8],
    ) -> io::Result<()> {
        // Gerçek SpliX (compress.cpp compressPage/_compressBandedPage) hiçbir zaman
        // ham/sıkıştırmasız (0x00) bant göndermez: yalnızca 0x0D/0x0E/0x11/0x13/0x15
        // algoritmalarından biri kullanılır. Bu yazıcı ailesinde ham bant firmware
        // tarafından tanınmıyor ("INTERNAL ERROR - Please use the proper driver"
        // ile reddediliyor); bu yüzden burada da her zaman Algo 0x11 RLE kullanılır.
        let payload_bytes = Algo0x11::compress(raw_bitmap).unwrap_or_default();
        let compression_type = SplCompression::Rle;

        // Toplam veri boyutu: payload + 4 (sub-header sig) + 4 (checksum)
        let total_data_size = (payload_bytes.len() + 8) as u32;

        // 1. Şerit Başlığı (Record 0x0C - 11 Bayt)
        let mut band_header = [0u8; 11];
        band_header[0x0] = 0x0C; // Signature 12
        band_header[0x1] = self.current_band;
        band_header[0x2] = (band_width_pixels >> 8) as u8;
        band_header[0x3] = (band_width_pixels & 0xFF) as u8;
        band_header[0x4] = (band_height_lines >> 8) as u8;
        band_header[0x5] = (band_height_lines & 0xFF) as u8;
        band_header[0x6] = compression_type as u8; // 0x11 (Algo 0x11 RLE)
        band_header[0x7] = (total_data_size >> 24) as u8;
        band_header[0x8] = (total_data_size >> 16) as u8;
        band_header[0x9] = (total_data_size >> 8) as u8;
        band_header[0xA] = (total_data_size & 0xFF) as u8;

        self.writer.write_all(&band_header)?;

        // 2. Alt Başlık İmzası (Sub-header: 0x09ABCDEF LE)
        self.writer.write_all(&SUBHEADER_SIG_LE)?;

        // 3. Sıkıştırılmış (Algo 0x11 RLE) Şerit Verisi
        self.writer.write_all(&payload_bytes)?;

        // 4. Sağlama Toplamı (Checksum: Subheader + Payload toplamı)
        let mut checksum = Algo0x11::calculate_checksum(&SUBHEADER_SIG_LE);
        checksum = checksum.wrapping_add(Algo0x11::calculate_checksum(&payload_bytes));

        let checksum_bytes = checksum.to_be_bytes();
        self.writer.write_all(&checksum_bytes)?;

        self.current_band = self.current_band.wrapping_add(1);
        Ok(())
    }

    /// Sayfa Sonu (3 baytlık QPDL Page Footer: [0x01, copies_msb, copies_lsb])
    pub fn end_page(&mut self, copies: u16) -> io::Result<()> {
        let footer = [
            0x01, // Page Footer İmzası
            (copies >> 8) as u8,
            (copies & 0xFF) as u8,
        ];
        self.writer.write_all(&footer)?;
        self.writer.flush()?;
        Ok(())
    }

    /// İş Sonu (PJL UEL Kapanışı)
    ///
    /// Gerçek SpliX (printer.cpp sendPJLFooter) `_endPJL`'i olduğu gibi yazıp
    /// hemen flush eder; sona fazladan bir "\n" EKLEMEZ.
    pub fn end_job(&mut self) -> io::Result<()> {
        self.writer.write_all(PJL_END)?;
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_pjl_field_strips_injection_chars() {
        assert_eq!(sanitize_pjl_field("Normal Title"), "Normal Title");
        // Çift tırnak: alıntılanmış alanı erken kapatabilir.
        assert_eq!(sanitize_pjl_field("a\"b"), "ab");
        // CR/LF: yeni bir PJL komut satırı başlatabilir.
        assert_eq!(sanitize_pjl_field("a\r\nb"), "ab");
        // ESC: Universal Exit Language (@PJL zarfını erken bitirir).
        assert_eq!(sanitize_pjl_field("a\x1bb"), "ab");
        // Tam enjeksiyon denemesi: yeni bir @PJL komutu eklemeye çalışıyor.
        let injected = sanitize_pjl_field("x\"\n@PJL DEFAULT PASSWORD=1234\n");
        assert!(!injected.contains('"'));
        assert!(!injected.contains('\n'));
        assert!(!injected.contains('\r'));
    }

    #[test]
    fn test_begin_job_blocks_pjl_line_injection() {
        // Kötü niyetli girdi: tırnak + CR/LF ile alıntılanmış alandan kaçıp
        // yeni bir "@PJL DEFAULT PASSWORD=..." satırı enjekte etmeye,
        // ayrıca ESC (UEL) ile PJL zarfını erken bitirip yeni bir zarf/iş
        // başlatmaya çalışıyor.
        let malicious = JobConfig {
            job_name: "Evil\"\n@PJL DEFAULT PASSWORD=1234".to_string(),
            user_name: "attacker\x1b%-12345X@PJL SET JOBNAME=\"hijacked".to_string(),
            service_date: "20120101".to_string(),
            duplex: SplDuplex::Simplex,
        };
        let benign = JobConfig {
            job_name: "Benign".to_string(),
            user_name: "benign".to_string(),
            service_date: "20120101".to_string(),
            duplex: SplDuplex::Simplex,
        };

        let mut out_malicious: Vec<u8> = Vec::new();
        SplStreamWriter::new(&mut out_malicious)
            .begin_job(&malicious)
            .unwrap();
        let mut out_benign: Vec<u8> = Vec::new();
        SplStreamWriter::new(&mut out_benign).begin_job(&benign).unwrap();

        // Kötü niyetli girdi, zararsız girdiyle AYNI sayıda PJL satırı
        // üretmeli: sanitizasyon başarısız olsaydı gömülü "\n" ekstra
        // satır(lar) enjekte ederdi.
        let lines_malicious = out_malicious.iter().filter(|&&b| b == b'\n').count();
        let lines_benign = out_benign.iter().filter(|&&b| b == b'\n').count();
        assert_eq!(
            lines_malicious, lines_benign,
            "girdi ekstra PJL komut satırı enjekte edebildi"
        );

        // Akışta, baştaki tek UEL dışında başka bir UEL (yeni bir PJL
        // zarfı/iş başlangıcı) bulunmamalı.
        let uel_count = out_malicious
            .windows(PJL_UEL.len())
            .filter(|w| *w == PJL_UEL)
            .count();
        assert_eq!(uel_count, 1, "girdi yeni bir UEL zarfı enjekte edebildi");
    }

    #[test]
    fn test_algo0x11_compression() {
        let mut sample = vec![0x00u8; 620 * 64];
        for i in 100..500 {
            sample[i] = 0xAA;
        }
        let comp = Algo0x11::compress(&sample).unwrap();
        assert!(comp.len() < sample.len());
    }

    #[test]
    fn test_algo0x11_roundtrip_synthetic() {
        // Rastgele olmayan ama tekrarlı+değişken içerik: metin benzeri veri.
        let mut sample = vec![0u8; 620 * 128];
        for (i, b) in sample.iter_mut().enumerate() {
            *b = ((i * 37 + (i / 620) * 13) % 251) as u8;
        }
        for i in 0..sample.len() {
            if (i / 620) % 5 == 0 && (i % 620) > 50 && (i % 620) < 400 {
                sample[i] = 0xFF;
            }
        }
        let comp = Algo0x11::compress(&sample).unwrap();
        let decomp = Algo0x11::decompress(&comp);
        assert_eq!(decomp.len(), sample.len(), "decompressed length mismatch");
        assert_eq!(decomp, sample, "decompressed content mismatch");
    }

    #[test]
    fn test_algo0x11_roundtrip_real_band0() {
        use crate::raster::CupsRasterReader;
        use std::fs::File;
        use std::io::{BufReader, Read};

        let path = "target/test_output/test.raster";
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("SKIP: {} yok, test atlandı", path);
                return;
            }
        };
        let mut reader = CupsRasterReader::new(BufReader::new(file)).unwrap();
        let header = reader.next_page_header().unwrap().unwrap();

        let page_width_pixels =
            (((header.page_size_points[0] as f64 * header.hw_resolution[0] as f64 / 72.0).ceil())
                as u32
                + 7)
                & !7u32;
        let band_width_bytes = ((page_width_pixels + 7) / 8) as usize;
        let cups_line_bytes = header.bytes_per_line as usize;
        let margin_bytes = (band_width_bytes - cups_line_bytes) / 2;
        let bytes_to_copy = cups_line_bytes.min(band_width_bytes - margin_bytes);
        let band_height = 128usize;

        let stream = reader.stream_mut();
        let mut line_buffer = vec![0u8; cups_line_bytes];
        let mut band_data = vec![0u8; band_width_bytes * band_height];
        let mut found_band = None;

        // Sayfanın en üstü genelde boş kenar boşluğudur; gerçek (tekdüze
        // olmayan) içerik barındıran ilk bandı bulana kadar ilerle.
        for band_idx in 0..(header.height as usize / band_height + 1) {
            for b in band_data.iter_mut() {
                *b = 0;
            }
            let mut lines_read = 0;
            for y in 0..band_height {
                if stream.read_exact(&mut line_buffer).is_err() {
                    break;
                }
                lines_read += 1;
                let dst = y * band_width_bytes + margin_bytes;
                band_data[dst..dst + bytes_to_copy].copy_from_slice(&line_buffer[..bytes_to_copy]);
            }
            if lines_read == 0 {
                break;
            }
            let mut inverted = band_data.clone();
            for b in &mut inverted {
                *b = !*b;
            }
            if inverted.iter().any(|&b| b != inverted[0]) {
                found_band = Some((band_idx, inverted));
                break;
            }
        }

        let (band_idx, band_data) = found_band.expect("sayfada tekdüze olmayan bant bulunamadı");
        eprintln!("Test icin secilen band: {}", band_idx);

        let comp = Algo0x11::compress(&band_data).unwrap();
        let decomp = Algo0x11::decompress(&comp);
        assert_eq!(
            decomp.len(),
            band_data.len(),
            "decompressed length mismatch: {} != {}",
            decomp.len(),
            band_data.len()
        );
        let first_diff = decomp.iter().zip(band_data.iter()).position(|(a, b)| a != b);
        assert_eq!(
            first_diff, None,
            "decompressed content differs from original at byte offset {:?}",
            first_diff
        );
    }
}
