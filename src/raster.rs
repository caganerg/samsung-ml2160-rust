//! # CUPS Raster Parser
//!
//! Bu modül, Linux CUPS filtre zincirinden (`cups-filters` / `libcupsfilters`)
//! gelen standart CUPS Raster akışını (V1, V2, V3) ayrıştırır.
//!
//! Not: PWG Raster (`PwgR` magic) yerine klasik CUPS Raster (`RaSt`, `RaS2`, `RaS3`)
//! ve `cups_page_header2_t` (1796 bayt) veri yapısı esas alınmıştır.

use std::fmt;
use std::io::{self, Read};

/// CUPS Raster spesifikasyonuna ait senkronizasyon (magic) baytları.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CupsRasterVersion {
    /// CUPS Raster Sürüm 1 - Big Endian (`RaSt`)
    V1Be,
    /// CUPS Raster Sürüm 1 - Little Endian (`tSaR`)
    V1Le,
    /// CUPS Raster Sürüm 2 - Big Endian (`RaS2`)
    V2Be,
    /// CUPS Raster Sürüm 2 - Little Endian (`2SaR`)
    V2Le,
    /// CUPS Raster Sürüm 3 - Big Endian (`RaS3`)
    V3Be,
    /// CUPS Raster Sürüm 3 - Little Endian (`3SaR`)
    V3Le,
}

impl CupsRasterVersion {
    /// Akışın Big-Endian olup olmadığını döner.
    #[inline]
    pub fn is_big_endian(&self) -> bool {
        matches!(
            self,
            CupsRasterVersion::V1Be | CupsRasterVersion::V2Be | CupsRasterVersion::V3Be
        )
    }

    /// Başlık yapısının beklenen bayt boyutu (V1 = 436 bayt, V2/V3 = 1796 bayt).
    #[inline]
    pub fn header_size(&self) -> usize {
        match self {
            CupsRasterVersion::V1Be | CupsRasterVersion::V1Le => 436,
            _ => 1796,
        }
    }
}

/// CUPS Renk Uzayları (`cups_cspace_e`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CupsColorSpace {
    W,          // 0: Luminance / Gri tonlama (0 = Beyaz)
    Rgb,        // 1: Standart RGB
    Rgba,       // 2: RGB + Alpha
    K,          // 3: Siyah-Beyaz / Siyah-Tonlama (0 = Siyah, Samsung lazer için ideal)
    Cmy,        // 4: Cyan, Magenta, Yellow
    Cmyk,       // 5: Cyan, Magenta, Yellow, Black
    Ymc,        // 6
    Ymck,       // 7
    Kcmy,       // 8
    Kcmycm,     // 9
    Gmck,       // 10
    Gmcs,       // 11
    White,      // 12
    Gold,       // 13
    Silver,     // 14
    CieXyz,     // 15
    CieLab,     // 16
    Rgbw,       // 17
    Sw,         // 18: sRGB Tabanlı Gri tonlama (DeviceGray)
    Srgb,       // 19: sRGB Renkli
    AdobeRgb,   // 20: Adobe RGB
    Device1,    // 32..47: Cihaza özel renk kanalları
    Device2,
    Device3,
    Device4,
    Device5,
    Device6,
    Device7,
    Device8,
    Device9,
    Device10,
    Device11,
    Device12,
    Device13,
    Device14,
    Device15,
    Device16,
    Unknown(u32),
}

impl From<u32> for CupsColorSpace {
    fn from(val: u32) -> Self {
        match val {
            0 => CupsColorSpace::W,
            1 => CupsColorSpace::Rgb,
            2 => CupsColorSpace::Rgba,
            3 => CupsColorSpace::K,
            4 => CupsColorSpace::Cmy,
            5 => CupsColorSpace::Cmyk,
            6 => CupsColorSpace::Ymc,
            7 => CupsColorSpace::Ymck,
            8 => CupsColorSpace::Kcmy,
            9 => CupsColorSpace::Kcmycm,
            10 => CupsColorSpace::Gmck,
            11 => CupsColorSpace::Gmcs,
            12 => CupsColorSpace::White,
            13 => CupsColorSpace::Gold,
            14 => CupsColorSpace::Silver,
            15 => CupsColorSpace::CieXyz,
            16 => CupsColorSpace::CieLab,
            17 => CupsColorSpace::Rgbw,
            18 => CupsColorSpace::Sw,
            19 => CupsColorSpace::Srgb,
            20 => CupsColorSpace::AdobeRgb,
            32 => CupsColorSpace::Device1,
            33 => CupsColorSpace::Device2,
            34 => CupsColorSpace::Device3,
            35 => CupsColorSpace::Device4,
            36 => CupsColorSpace::Device5,
            37 => CupsColorSpace::Device6,
            38 => CupsColorSpace::Device7,
            39 => CupsColorSpace::Device8,
            40 => CupsColorSpace::Device9,
            41 => CupsColorSpace::Device10,
            42 => CupsColorSpace::Device11,
            43 => CupsColorSpace::Device12,
            44 => CupsColorSpace::Device13,
            45 => CupsColorSpace::Device14,
            46 => CupsColorSpace::Device15,
            47 => CupsColorSpace::Device16,
            other => CupsColorSpace::Unknown(other),
        }
    }
}

impl fmt::Display for CupsColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CupsColorSpace::W => write!(f, "W (White=0 Grayscale)"),
            CupsColorSpace::Rgb => write!(f, "RGB"),
            CupsColorSpace::Rgba => write!(f, "RGBA"),
            CupsColorSpace::K => write!(f, "K (Black=0 Grayscale)"),
            CupsColorSpace::Cmy => write!(f, "CMY"),
            CupsColorSpace::Cmyk => write!(f, "CMYK"),
            CupsColorSpace::Sw => write!(f, "sGray (sRGB Grayscale)"),
            CupsColorSpace::Srgb => write!(f, "sRGB"),
            CupsColorSpace::AdobeRgb => write!(f, "AdobeRGB"),
            CupsColorSpace::Unknown(id) => write!(f, "Unknown({})", id),
            other => write!(f, "{:?}", other),
        }
    }
}

/// CUPS Renk Dizilimi (`cups_order_e`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CupsColorOrder {
    /// Piksel baytları ardışık dizilir (Örn: RGBRGB... veya KKKK...)
    Chunked,
    /// Renk düzlemleri her satırda ayrı şeritler halindedir (RR... GG... BB...)
    Banded,
    /// Her renk düzlemi tüm sayfa boyunca ayrı bir sayfadır
    Planar,
    Unknown(u32),
}

impl From<u32> for CupsColorOrder {
    fn from(val: u32) -> Self {
        match val {
            0 => CupsColorOrder::Chunked,
            1 => CupsColorOrder::Banded,
            2 => CupsColorOrder::Planar,
            other => CupsColorOrder::Unknown(other),
        }
    }
}

/// `cups_page_header2_t` (CUPS V2 / V3) ve `cups_page_header_t` (CUPS V1)
/// sayfa başlığı yapısının Rust modellemesi.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PageHeader {
    // CUPS V1 Başlık Alanları (0..420 bayt)
    pub media_class: String,
    pub media_color: String,
    pub media_type: String,
    pub output_type: String,

    pub advance_distance: u32,
    pub advance_media: u32,
    pub collate: bool,
    pub cut_media: u32,
    pub duplex: bool,
    pub hw_resolution: [u32; 2],       // [X DPI, Y DPI]
    pub imaging_bounding_box: [u32; 4], // [Left, Bottom, Right, Top] (pt)
    pub insert_sheet: bool,
    pub jog: u32,
    pub leading_edge: u32,
    pub margins: [u32; 2],             // [Left, Bottom] (pt)
    pub manual_feed: bool,
    pub media_position: u32,
    pub media_weight: u32,
    pub mirror_print: bool,
    pub negative_print: bool,
    pub num_copies: u32,
    pub orientation: u32,
    pub output_face_up: bool,
    pub page_size_points: [u32; 2],    // [Width, Length] (1/72 inch points)
    pub separations: bool,
    pub tray_switch: bool,
    pub turn_off: bool,

    pub width: u32,                    // cupsWidth (piksel)
    pub height: u32,                   // cupsHeight (piksel)
    pub cups_media_type: u32,
    pub bits_per_color: u32,           // cupsBitsPerColor (1, 8, 16)
    pub bits_per_pixel: u32,           // cupsBitsPerPixel (1, 8, 24, 32)
    pub bytes_per_line: u32,           // cupsBytesPerLine
    pub color_order: CupsColorOrder,   // cupsColorOrder
    pub color_space: CupsColorSpace,   // cupsColorSpace
    pub compression: u32,              // cupsCompression (0 = uncompressed)
    pub row_count: u32,
    pub row_feed: u32,
    pub row_step: u32,

    // CUPS V2 / V3 Genişletilmiş Alanlar (420..1796 bayt)
    pub num_colors: u32,
    pub page_size_f: [f32; 2],
    pub rendering_intent: Option<String>,
    pub page_size_name: Option<String>,
}

impl PageHeader {
    fn parse_c_string(bytes: &[u8]) -> String {
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..len]).trim().to_string()
    }

    #[inline]
    fn read_u32(buf: &[u8], offset: usize, is_be: bool) -> u32 {
        let slice: [u8; 4] = buf[offset..offset + 4]
            .try_into()
            .expect("Hatalı dilim uzunluğu");
        if is_be {
            u32::from_be_bytes(slice)
        } else {
            u32::from_le_bytes(slice)
        }
    }

    #[inline]
    fn read_f32(buf: &[u8], offset: usize, is_be: bool) -> f32 {
        let slice: [u8; 4] = buf[offset..offset + 4]
            .try_into()
            .expect("Hatalı dilim uzunluğu");
        if is_be {
            f32::from_be_bytes(slice)
        } else {
            f32::from_le_bytes(slice)
        }
    }

    /// CUPS Raster sayfa başlığını ayrıştırır.
    pub fn parse(buf: &[u8], version: CupsRasterVersion) -> io::Result<Self> {
        let is_be = version.is_big_endian();
        let expected_size = version.header_size();

        if buf.len() < expected_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "CUPS Raster başlık boyutu yetersiz (beklenen: {} bayt, gelen: {} bayt)",
                    expected_size,
                    buf.len()
                ),
            ));
        }

        // C dize alanları (0..256)
        let media_class = Self::parse_c_string(&buf[0..64]);
        let media_color = Self::parse_c_string(&buf[64..128]);
        let media_type = Self::parse_c_string(&buf[128..192]);
        let output_type = Self::parse_c_string(&buf[192..256]);

        let advance_distance = Self::read_u32(buf, 256, is_be);
        let advance_media = Self::read_u32(buf, 260, is_be);
        let collate = Self::read_u32(buf, 264, is_be) != 0;
        let cut_media = Self::read_u32(buf, 268, is_be);
        let duplex = Self::read_u32(buf, 272, is_be) != 0;

        let hw_res_x = Self::read_u32(buf, 276, is_be);
        let hw_res_y = Self::read_u32(buf, 280, is_be);

        let img_bbox = [
            Self::read_u32(buf, 284, is_be),
            Self::read_u32(buf, 288, is_be),
            Self::read_u32(buf, 292, is_be),
            Self::read_u32(buf, 296, is_be),
        ];

        let insert_sheet = Self::read_u32(buf, 300, is_be) != 0;
        let jog = Self::read_u32(buf, 304, is_be);
        let leading_edge = Self::read_u32(buf, 308, is_be);
        let margin_x = Self::read_u32(buf, 312, is_be);
        let margin_y = Self::read_u32(buf, 316, is_be);
        let manual_feed = Self::read_u32(buf, 320, is_be) != 0;
        let media_position = Self::read_u32(buf, 324, is_be);
        let media_weight = Self::read_u32(buf, 328, is_be);
        let mirror_print = Self::read_u32(buf, 332, is_be) != 0;
        let negative_print = Self::read_u32(buf, 336, is_be) != 0;
        let num_copies = Self::read_u32(buf, 340, is_be);
        let orientation = Self::read_u32(buf, 344, is_be);
        let output_face_up = Self::read_u32(buf, 348, is_be) != 0;
        let page_sz_w = Self::read_u32(buf, 352, is_be);
        let page_sz_h = Self::read_u32(buf, 356, is_be);
        let separations = Self::read_u32(buf, 360, is_be) != 0;
        let tray_switch = Self::read_u32(buf, 364, is_be) != 0;
        let turn_off = Self::read_u32(buf, 368, is_be) != 0;

        let width = Self::read_u32(buf, 372, is_be);
        let height = Self::read_u32(buf, 376, is_be);
        let cups_media_type = Self::read_u32(buf, 380, is_be);
        let bits_per_color = Self::read_u32(buf, 384, is_be);
        let bits_per_pixel = Self::read_u32(buf, 388, is_be);
        let bytes_per_line = Self::read_u32(buf, 392, is_be);
        let color_order_val = Self::read_u32(buf, 396, is_be);
        let color_space_val = Self::read_u32(buf, 400, is_be);
        let compression = Self::read_u32(buf, 404, is_be);
        let row_count = Self::read_u32(buf, 408, is_be);
        let row_feed = Self::read_u32(buf, 412, is_be);
        let row_step = Self::read_u32(buf, 416, is_be);

        // V2 / V3 Genişletilmiş Alanlar
        let (num_colors, page_size_f, rendering_intent, page_size_name) = if expected_size >= 1796 {
            let num_colors = Self::read_u32(buf, 420, is_be);
            let ps_w_f = Self::read_f32(buf, 428, is_be);
            let ps_h_f = Self::read_f32(buf, 432, is_be);

            // cupsRenderingIntent: 1668..1732
            let intent_raw = Self::parse_c_string(&buf[1668..1732]);
            let intent = if intent_raw.is_empty() { None } else { Some(intent_raw) };

            // cupsPageSizeName: 1732..1796
            let name_raw = Self::parse_c_string(&buf[1732..1796]);
            let name = if name_raw.is_empty() { None } else { Some(name_raw) };

            (num_colors, [ps_w_f, ps_h_f], intent, name)
        } else {
            (0, [page_sz_w as f32, page_sz_h as f32], None, None)
        };

        Ok(Self {
            media_class,
            media_color,
            media_type,
            output_type,
            advance_distance,
            advance_media,
            collate,
            cut_media,
            duplex,
            hw_resolution: [hw_res_x, hw_res_y],
            imaging_bounding_box: img_bbox,
            insert_sheet,
            jog,
            leading_edge,
            margins: [margin_x, margin_y],
            manual_feed,
            media_position,
            media_weight,
            mirror_print,
            negative_print,
            num_copies,
            orientation,
            output_face_up,
            page_size_points: [page_sz_w, page_sz_h],
            separations,
            tray_switch,
            turn_off,
            width,
            height,
            cups_media_type,
            bits_per_color,
            bits_per_pixel,
            bytes_per_line,
            color_order: CupsColorOrder::from(color_order_val),
            color_space: CupsColorSpace::from(color_space_val),
            compression,
            row_count,
            row_feed,
            row_step,
            num_colors,
            page_size_f,
            rendering_intent,
            page_size_name,
        })
    }

    /// Sayfaya ait ham (sıkıştırılmamış) toplam piksel verisi boyutu.
    #[inline]
    pub fn total_raster_bytes(&self) -> u64 {
        (self.bytes_per_line as u64) * (self.height as u64)
    }
}

/// Akıştan CUPS Raster verisini okuyan ayrıştırıcı.
pub struct CupsRasterReader<R: Read> {
    reader: R,
    version: CupsRasterVersion,
    page_count: u32,
}

impl<R: Read> CupsRasterReader<R> {
    /// 4 baytlık CUPS Raster senkronizasyon kodunu doğrulayarak okuyucuyu başlatır.
    pub fn new(mut reader: R) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut magic) {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Girdi akışı boş (0 bayt). cupsfilter veya önceki filtre aşamasının başarıyla raster ürettiğinden emin olun.",
                ));
            }
            return Err(e);
        }

        let version = match &magic {
            b"RaSt" => CupsRasterVersion::V1Be,
            b"tSaR" => CupsRasterVersion::V1Le,
            b"RaS2" => CupsRasterVersion::V2Be,
            b"2SaR" => CupsRasterVersion::V2Le,
            b"RaS3" => CupsRasterVersion::V3Be,
            b"3SaR" => CupsRasterVersion::V3Le,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Geçersiz CUPS Raster formatı: {:?} (Beklenen: 'RaSt', 'tSaR', 'RaS2', '2SaR', 'RaS3', '3SaR')",
                        String::from_utf8_lossy(other)
                    ),
                ));
            }
        };

        Ok(Self {
            reader,
            version,
            page_count: 0,
        })
    }

    #[inline]
    pub fn version(&self) -> CupsRasterVersion {
        self.version
    }

    #[allow(dead_code)]
    #[inline]
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// Sonraki sayfa başlığını okur. Akış, sayfalar arasında temiz bir şekilde
    /// (0 bayt okunarak) sona ererse `Ok(None)` döner.
    ///
    /// `read_exact` tek başına, akışın başlık ortasında kesildiği (bozuk/yarım
    /// veri) durumu ile iki sayfa arasındaki normal akış sonunu AYIRT EDEMEZ;
    /// ikisi de aynı `UnexpectedEof` hatasını üretir. Bu, gerçek bir bozulmayı
    /// sessizce "işi normal bitir" olarak yorumlamamak için baytları elle,
    /// sayarak okur.
    pub fn next_page_header(&mut self) -> io::Result<Option<PageHeader>> {
        let header_len = self.version.header_size();
        let mut buf = vec![0u8; header_len];

        let mut total_read = 0usize;
        while total_read < header_len {
            match self.reader.read(&mut buf[total_read..]) {
                Ok(0) => break,
                Ok(n) => total_read += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }

        if total_read == 0 {
            return Ok(None);
        }
        if total_read < header_len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "CUPS Raster akışı sayfa başlığının ortasında kesildi \
                     ({} / {} bayt okundu). Önceki filtre aşaması yarıda kesilmiş olabilir.",
                    total_read, header_len
                ),
            ));
        }

        self.page_count += 1;
        let header = PageHeader::parse(&buf, self.version)?;
        Ok(Some(header))
    }

    /// Sayfa raster verisini okumak için temel akışa erişim.
    #[inline]
    pub fn stream_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_cups_sync_words() {
        let v2_be = b"RaS2";
        let reader = CupsRasterReader::new(Cursor::new(v2_be)).unwrap();
        assert_eq!(reader.version(), CupsRasterVersion::V2Be);
        assert!(reader.version().is_big_endian());
        assert_eq!(reader.version().header_size(), 1796);

        let v1_le = b"tSaR";
        let reader_v1 = CupsRasterReader::new(Cursor::new(v1_le)).unwrap();
        assert_eq!(reader_v1.version(), CupsRasterVersion::V1Le);
        assert!(!reader_v1.version().is_big_endian());
        assert_eq!(reader_v1.version().header_size(), 436);
    }

    #[test]
    fn test_cups_header_parse_canonical() {
        let mut header_buf = vec![0u8; 1796];

        // HWResolution: [600, 600]
        header_buf[276..280].copy_from_slice(&600u32.to_be_bytes());
        header_buf[280..284].copy_from_slice(&600u32.to_be_bytes());

        // PageSize points: [595, 842] (A4)
        header_buf[352..356].copy_from_slice(&595u32.to_be_bytes());
        header_buf[356..360].copy_from_slice(&842u32.to_be_bytes());

        // width: 4960, height: 7016
        header_buf[372..376].copy_from_slice(&4960u32.to_be_bytes());
        header_buf[376..380].copy_from_slice(&7016u32.to_be_bytes());

        // bits_per_color: 1, bits_per_pixel: 1, bytes_per_line: 620
        header_buf[384..388].copy_from_slice(&1u32.to_be_bytes());
        header_buf[388..392].copy_from_slice(&1u32.to_be_bytes());
        header_buf[392..396].copy_from_slice(&620u32.to_be_bytes());

        // color_space: 3 (K/Black)
        header_buf[400..404].copy_from_slice(&3u32.to_be_bytes());

        // cupsPageSizeName at offset 1732: "A4"
        header_buf[1732..1734].copy_from_slice(b"A4");

        let header = PageHeader::parse(&header_buf, CupsRasterVersion::V2Be).unwrap();
        assert_eq!(header.hw_resolution, [600, 600]);
        assert_eq!(header.page_size_points, [595, 842]);
        assert_eq!(header.width, 4960);
        assert_eq!(header.height, 7016);
        assert_eq!(header.bits_per_color, 1);
        assert_eq!(header.bits_per_pixel, 1);
        assert_eq!(header.bytes_per_line, 620);
        assert_eq!(header.color_space, CupsColorSpace::K);
        assert_eq!(header.page_size_name.as_deref(), Some("A4"));
        assert_eq!(header.total_raster_bytes(), 620 * 7016);
    }
}
