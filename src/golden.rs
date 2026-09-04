//! # Altın Dosya (Golden File) Koşum Takımı
//!
//! Bu modül, 1.x CUPS filtresinin ÜRETTİĞİ SPL2/QPDL baytlarını, kod PAPPL
//! Printer Application'a taşınmadan ÖNCE dondurur. Taşıma sırasındaki kabul
//! ölçütü bayt bayt aynılıktır (proje kuralı 6); o ölçütün karşılaştırılacak
//! bir referansa ihtiyacı var ve o referans yalnızca ŞİMDİ, 1.x kodu hâlâ
//! çalışırken yakalanabilir.
//!
//! ## Ne saklanıyor
//!
//! Her vaka için iki dosya:
//!
//! * `goldens/<vaka>.spl`  — filtrenin ürettiği ham SPL2/QPDL akışı.
//! * `goldens/<vaka>.json` — o akışı üreten KLASİK CUPS sayfa başlığının
//!   alanları, artı filtrenin o başlıktan türettiği QPDL yerleşim değerleri.
//!
//! JSON yan dosyası (sidecar) taşımanın ikinci yarısı için var: PAPPL
//! tarafındaki seçenek eşlemesi (medya boyutu, çözünürlük, tepsi, kağıt türü,
//! kenar boşluğu) bu dosyalara karşı doğrulanacak. Klasik başlık PAPPL
//! yolunda artık üretilmeyeceği için, bu değerler de yalnızca şimdi
//! yakalanabilir.
//!
//! ## Girdiler neden commit edilmiyor
//!
//! Raster girdileri (A4 @1200 DPI için ~16 MB) bu dosyadaki `build_raster`
//! tarafından DETERMİNİSTİK olarak üretiliyor, bu yüzden depoya konmuyor.
//! Üreticinin kendisi değişirse bu, JSON yan dosyasındaki başlık alanlarında
//! görünür — yani sapma sessiz kalmaz, incelemede fark edilir.
//!
//! ## Yenileme
//!
//! ```text
//! UPDATE_GOLDENS=1 cargo test golden
//! ```
//!
//! Altın dosyalar YALNIZCA kasıtlı bir davranış değişikliğiyle birlikte
//! yenilenmelidir; `.spl` dosyasındaki her diff, yazıcıya giden baytların
//! değiştiği anlamına gelir.

use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use crate::raster::{CupsRasterVersion, PageHeader};
use crate::spl::{current_service_date, SplPaperSize, SplPaperSource};
use crate::{
    band_height_for, band_placement, compute_page_width_pixels, hard_margin_bytes,
    process_cups_raster_to_spl, sanitize_copies, CupsFilterArgs,
};

/// Altın dosyalara yazılan SABİT servis tarihi.
///
/// `@PJL DEFAULT SERVICEDATE` satırı normalde bugünün tarihini taşır; sabit
/// bir değer verilmezse altın dosyalar her gece yarısı kırılırdı ve
/// karşılaştırmayı "SERVICEDATE satırını yok say" diye gevşetmek, o satırdaki
/// GERÇEK sapmaları da gizlerdi. Bu yüzden tarih gevşetilmiyor, sabitleniyor
/// (bkz. `process_cups_raster_to_spl` doc yorumu).
const GOLDEN_SERVICE_DATE: &str = "20260101";

/// Altın dosyaların iş adı/kullanıcı adı — PJL alanlarının da sabit kalması
/// için argv'den değil buradan geliyor.
const GOLDEN_TITLE: &str = "golden";
const GOLDEN_USER: &str = "tester";

/// PPD'den türetilmiş medya tanımı.
///
/// `dimension_pt` = `*PaperDimension`, `imageable_pt` = `*ImageableArea`
/// (sol, alt, sağ, üst). Değerler `ppd/samsung-ml2160.ppd` içindeki
/// karşılık gelen satırlardan birebir alındı; `test_golden_media_matches_ppd`
/// bağı bir yorum olmaktan çıkarıp doğruluyor.
#[derive(Debug, Clone, Copy)]
struct Media {
    /// PPD `*PageSize` anahtarı.
    ppd_key: &'static str,
    /// `*PaperDimension <key>: "W H"`.
    dimension_pt: (u32, u32),
    /// `*ImageableArea <key>: "L B R T"`.
    imageable_pt: (u32, u32, u32, u32),
}

impl Media {
    /// Basılabilir alanın genişliği/yüksekliği (pt).
    fn imageable_size_pt(&self) -> (u32, u32) {
        (
            self.imageable_pt.2 - self.imageable_pt.0,
            self.imageable_pt.3 - self.imageable_pt.1,
        )
    }
}

const A4: Media = Media {
    ppd_key: "A4",
    dimension_pt: (595, 842),
    imageable_pt: (12, 12, 583, 830),
};
const LETTER: Media = Media {
    ppd_key: "Letter",
    dimension_pt: (612, 792),
    imageable_pt: (12, 12, 600, 780),
};
const A5: Media = Media {
    ppd_key: "A5",
    dimension_pt: (420, 595),
    imageable_pt: (12, 12, 408, 583),
};
const ENV_C5: Media = Media {
    ppd_key: "EnvC5",
    dimension_pt: (459, 649),
    imageable_pt: (12, 12, 447, 637),
};

/// `cups-filters`'ın piksel sayısı yuvarlaması: `round(pt * dpi / 72)`.
///
/// Bu kural tahmin değil, ÖLÇÜM: `test_validate_page_header_accepts_real_cupsfilter_heights`
/// içindeki sekiz gerçek `cupsHeight` değerinin (A4 @300/600/1200, Letter
/// @600, Legal @600/1200, A6 @600, Folio @1200) hepsi bu formülle birebir
/// yeniden üretiliyor; `test_golden_geometry_matches_measured_cupsfilter_output`
/// bunu doğruluyor. `ceil` ya da `floor` sekiz değerin en az birinde şaşıyor.
fn px_from_pt(pt: u32, dpi: u32) -> u32 {
    ((pt as u64 * dpi as u64 * 2 + 72) / (72 * 2)) as u32
}

/// Sayfa içeriği.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Content {
    /// Tamamen boş sayfa.
    Blank,
    /// Basılabilir alanın DÖRT KÖŞESİNE birer piksellik hizalama işareti.
    ///
    /// Kenar boşluğu regresyonunun bayt düzeyinde yakalanmasını sağlayan
    /// vaka budur (bkz. `docs/MIGRATION-PLAN.md` R-1). Bir köşe işareti,
    /// bant tamponunda tam olarak tek bir bitin konumunu belirler; yatay
    /// yerleşim bir bayt kayarsa sıkıştırılmış payload değişir ve altın
    /// dosya karşılaştırması cetvele gerek kalmadan kırılır.
    ///
    /// DİKKAT: sol köşe işaretleri her zaman KAĞITA ULAŞMAZ. A4 @600 DPI'da
    /// sert kenar boşluğu 13 bayt, ortalama 12 bayttır; net `src_skip = 1`,
    /// yani CUPS satırının ilk baytı (dolayısıyla x=0 işareti) atılır. Bu bir
    /// hata değil, SpliX'in davranışıdır — ve altın dosyanın kaydettiği şey
    /// tam olarak budur. Yan dosyadaki `src_skip_bytes` alanı hangi vakada
    /// ne kadar atıldığını söyler.
    RegistrationMarks,
}

/// Girdi akışının CUPS Raster sürümü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    /// `RaS3` — sıkıştırmasız.
    V3,
    /// `RaS2` — satır-RLE (PWG Raster ile aynı gövde kodlaması).
    V2Rle,
}

/// Bir altın dosya vakası.
#[derive(Debug, Clone, Copy)]
struct Case {
    name: &'static str,
    media: Media,
    /// (x_dpi, y_dpi)
    resolution: (u32, u32),
    copies: u32,
    /// CUPS `MediaPosition` (PPD `*InputSlot`); 0 = seçilmemiş.
    media_position: u32,
    /// CUPS `MediaType` (PPD `*MediaType`); boş = seçilmemiş.
    media_type: &'static str,
    pages: u32,
    content: Content,
    encoding: Encoding,
}

impl Case {
    const fn new(name: &'static str, media: Media, resolution: (u32, u32)) -> Self {
        Self {
            name,
            media,
            resolution,
            copies: 1,
            media_position: 0,
            media_type: "",
            pages: 1,
            content: Content::RegistrationMarks,
            encoding: Encoding::V3,
        }
    }

    /// CUPS satırının piksel genişliği — basılabilir alandan türetilir.
    fn width_px(&self) -> u32 {
        px_from_pt(self.media.imageable_size_pt().0, self.resolution.0)
    }

    fn height_lines(&self) -> u32 {
        px_from_pt(self.media.imageable_size_pt().1, self.resolution.1)
    }

    fn bytes_per_line(&self) -> u32 {
        self.width_px().div_ceil(8)
    }
}

/// Altın dosya korpusu.
///
/// Kapsam gerekçesi:
///
/// * `*-marks` vakaları A4 ve Letter için DESTEKLENEN HER ÇÖZÜNÜRLÜKTE var
///   (300x300, 600x600, 1200x600, 1200x1200) — kenar boşluğu ve bant
///   yüksekliği kurallarının hepsini birden gerdiriyor. 1200x600 asimetrik
///   modu, çözünürlük eksenlerinin karışmasını (R-4) yakalayan tek vakadır.
/// * `a4-300-marks` ayrıca 64 satırlık bant kuralını (R-3) kapsar; diğer
///   bütün vakalar 128 satırlık bant üretir.
/// * `a5` ve `envc5` küçük medya ve zarf geometrisini kapsar.
/// * `*-3copies` kopya alanının iki yere birden (sayfa başlığı + sayfa sonu)
///   yazılmasını dondurur (R-5).
/// * `*-manual-env` tepsi ve kağıt türü eşlemesini dondurur.
/// * `*-v2rle` aynı sayfayı satır-RLE'li bir girdiden üretir; `*-600-marks`
///   ile BAYT BAYT aynı çıkmalıdır (`test_golden_v2_and_v3_agree`).
/// * `a4-600-blank` sıkıştırıcının tamamen boş bant davranışını dondurur.
const CASES: &[Case] = &[
    // --- A4, her çözünürlük ---
    Case::new("a4-300-marks", A4, (300, 300)),
    Case::new("a4-600-marks", A4, (600, 600)),
    Case::new("a4-1200x600-marks", A4, (1200, 600)),
    Case::new("a4-1200-marks", A4, (1200, 1200)),
    // --- Letter, her çözünürlük ---
    Case::new("letter-300-marks", LETTER, (300, 300)),
    Case::new("letter-600-marks", LETTER, (600, 600)),
    Case::new("letter-1200x600-marks", LETTER, (1200, 600)),
    Case::new("letter-1200-marks", LETTER, (1200, 1200)),
    // --- diğer medya ---
    Case::new("a5-600-marks", A5, (600, 600)),
    Case::new("envc5-600-marks", ENV_C5, (600, 600)),
    // --- boş sayfa ---
    Case {
        content: Content::Blank,
        ..Case::new("a4-600-blank", A4, (600, 600))
    },
    // --- kopyalar ---
    Case {
        copies: 3,
        ..Case::new("a4-600-marks-3copies", A4, (600, 600))
    },
    // --- tepsi + kağıt türü ---
    Case {
        media_position: 2,
        media_type: "ENV",
        ..Case::new("envc5-600-marks-manual-env", ENV_C5, (600, 600))
    },
    // --- çok sayfa ---
    Case {
        pages: 3,
        ..Case::new("a4-600-marks-3pages", A4, (600, 600))
    },
    // --- v2 satır-RLE girdisi ---
    Case {
        encoding: Encoding::V2Rle,
        ..Case::new("a4-600-marks-v2rle", A4, (600, 600))
    },
];

// ============================================================================
// Raster girdisi üreteci
// ============================================================================

/// Tek bir raster satırı üretir.
fn build_line(case: &Case, y: u32) -> Vec<u8> {
    let bpl = case.bytes_per_line() as usize;
    let mut line = vec![0u8; bpl];

    if case.content == Content::RegistrationMarks {
        let last_y = case.height_lines() - 1;
        if y == 0 || y == last_y {
            // Sol köşe: x = 0 -> 0. baytın en anlamlı biti.
            line[0] |= 0x80;
            // Sağ köşe: x = width - 1.
            let last_x = case.width_px() - 1;
            let byte = (last_x / 8) as usize;
            let bit = 7 - (last_x % 8);
            line[byte] |= 1 << bit;
        }
    }

    line
}

/// 1796 baytlık `cups_page_header2_t` (Big Endian) üretir.
fn build_page_header(case: &Case) -> Vec<u8> {
    let mut buf = vec![0u8; 1796];
    let mut put = |off: usize, val: u32| {
        buf[off..off + 4].copy_from_slice(&val.to_be_bytes());
    };

    let (img_l, img_b, img_r, img_t) = case.media.imageable_pt;

    put(276, case.resolution.0); // HWResolution[0]
    put(280, case.resolution.1); // HWResolution[1]
    put(284, img_l); // ImagingBoundingBox
    put(288, img_b);
    put(292, img_r);
    put(296, img_t);
    put(312, img_l); // Margins[0] — *ImageableArea sol kenarı
    put(316, img_b); // Margins[1]
    put(324, case.media_position);
    put(340, case.copies);
    put(352, case.media.dimension_pt.0); // PageSize[0]
    put(356, case.media.dimension_pt.1); // PageSize[1]
    put(372, case.width_px()); // cupsWidth
    put(376, case.height_lines()); // cupsHeight
    put(384, 1); // cupsBitsPerColor
    put(388, 1); // cupsBitsPerPixel
    put(392, case.bytes_per_line()); // cupsBytesPerLine
    put(396, 0); // cupsColorOrder = Chunked
    put(400, 3); // cupsColorSpace = K

    let media_type = case.media_type.as_bytes();
    buf[128..128 + media_type.len()].copy_from_slice(media_type);
    let name = case.media.ppd_key.as_bytes();
    buf[1732..1732 + name.len()].copy_from_slice(name); // cupsPageSizeName

    buf
}

/// Bir satırı CUPS Raster v2 satır-RLE'siyle kodlar.
///
/// Kodlama `raster.rs` `CupsLineDecoder`'ın çözdüğü biçimdir: satır başına bir
/// `repeat` baytı (burada hep 0 = satır bir kez), ardından satır dolana kadar
/// tekrar kayıtları. 1-bit veride piksel başına bayt 1'dir, bu yüzden eşit
/// baytları en çok 128'lik gruplara bölmek yeterli.
fn encode_v2_line(line: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8]; // repeat = 0 -> satır bir kez
    let mut i = 0usize;
    while i < line.len() {
        let byte = line[i];
        let mut run = 1usize;
        while i + run < line.len() && line[i + run] == byte && run < 128 {
            run += 1;
        }
        out.push((run - 1) as u8); // n < 128 -> sonraki piksel (n+1) kez
        out.push(byte);
        i += run;
    }
    out
}

/// Vakanın tam CUPS Raster akışını üretir.
fn build_raster(case: &Case) -> Vec<u8> {
    let mut stream = match case.encoding {
        Encoding::V3 => b"RaS3".to_vec(),
        Encoding::V2Rle => b"RaS2".to_vec(),
    };

    let header = build_page_header(case);
    let height = case.height_lines();

    for _ in 0..case.pages {
        stream.extend_from_slice(&header);
        for y in 0..height {
            let line = build_line(case, y);
            match case.encoding {
                Encoding::V3 => stream.extend_from_slice(&line),
                Encoding::V2Rle => stream.extend_from_slice(&encode_v2_line(&line)),
            }
        }
    }

    stream
}

// ============================================================================
// Yan dosya (sidecar) üretimi
// ============================================================================

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Vakanın klasik CUPS sayfa başlığını ve filtrenin ondan türettiği QPDL
/// yerleşim değerlerini JSON olarak yazar.
///
/// PAPPL tarafındaki seçenek eşlemesi bu dosyaya karşı doğrulanacak: klasik
/// başlık orada artık üretilmeyeceği için, referans yalnızca burada saklanır.
fn build_sidecar(case: &Case) -> String {
    let header_bytes = build_page_header(case);
    let header = PageHeader::parse(&header_bytes, CupsRasterVersion::V3Be)
        .expect("üretilen başlık ayrıştırılamadı");

    // Filtrenin türettiği yerleşim değerleri.
    let page_width_px =
        compute_page_width_pixels(header.page_size_points[0], header.hw_resolution[0]);
    let band_width_bytes = page_width_px.div_ceil(8);
    let hard_margin = hard_margin_bytes(header.margins[0], header.hw_resolution[0]);
    let placement = band_placement(
        band_width_bytes as usize,
        header.bytes_per_line as usize,
        hard_margin,
    )
    .expect("altın vaka geçerli bir yerleşim üretmeli");
    let band_height = band_height_for(&header);
    let paper_size = SplPaperSize::from_dimensions_pt_exact(
        header.page_size_points[0],
        header.page_size_points[1],
    )
    .expect("altın vaka tanınan bir kâğıt ölçüsü kullanmalı");
    let paper_source = SplPaperSource::from_media_position(header.media_position)
        .expect("altın vaka tanınan bir tepsi kullanmalı");

    let (img_l, img_b, img_r, img_t) = case.media.imageable_pt;

    format!(
        r#"{{
  "case": "{name}",
  "note": "Klasik CUPS sayfa başlığı ve filtrenin ondan türettiği QPDL yerleşimi. PAPPL tarafındaki seçenek eşlemesi buna karşı doğrulanacak.",
  "input": {{
    "encoding": "{encoding}",
    "pages": {pages},
    "content": "{content}"
  }},
  "cups_page_header": {{
    "MediaType": "{media_type}",
    "cupsPageSizeName": "{page_size_name}",
    "HWResolution": [{res_x}, {res_y}],
    "PageSize": [{page_w}, {page_h}],
    "ImagingBoundingBox": [{img_l}, {img_b}, {img_r}, {img_t}],
    "Margins": [{margin_l}, {margin_b}],
    "MediaPosition": {media_position},
    "NumCopies": {num_copies},
    "Duplex": {duplex},
    "Tumble": {tumble},
    "cupsWidth": {width},
    "cupsHeight": {height},
    "cupsBytesPerLine": {bpl},
    "cupsBitsPerColor": {bpc},
    "cupsBitsPerPixel": {bpp},
    "cupsColorOrder": {color_order},
    "cupsColorSpace": {color_space},
    "cupsCompression": {compression}
  }},
  "derived_qpdl": {{
    "page_width_pixels": {page_width_px},
    "band_width_bytes": {band_width_bytes},
    "band_height_lines": {band_height},
    "hard_margin_bytes": {hard_margin},
    "dst_offset_bytes": {dst_offset},
    "src_skip_bytes": {src_skip},
    "paper_size_code": {paper_code},
    "paper_source_code": {source_code},
    "copies_sent": {copies_sent}
  }},
  "ppd_source": {{
    "PageSize": "{ppd_key}",
    "PaperDimension": "{page_w} {page_h}",
    "ImageableArea": "{img_l} {img_b} {img_r} {img_t}"
  }}
}}
"#,
        name = json_escape(case.name),
        encoding = match case.encoding {
            Encoding::V3 => "RaS3",
            Encoding::V2Rle => "RaS2",
        },
        pages = case.pages,
        content = match case.content {
            Content::Blank => "blank",
            Content::RegistrationMarks => "registration-marks",
        },
        media_type = json_escape(&header.media_type),
        page_size_name = json_escape(header.page_size_name.as_deref().unwrap_or("")),
        res_x = header.hw_resolution[0],
        res_y = header.hw_resolution[1],
        page_w = header.page_size_points[0],
        page_h = header.page_size_points[1],
        img_l = img_l,
        img_b = img_b,
        img_r = img_r,
        img_t = img_t,
        margin_l = header.margins[0],
        margin_b = header.margins[1],
        media_position = header.media_position,
        num_copies = header.num_copies,
        duplex = header.duplex,
        tumble = header.tumble,
        width = header.width,
        height = header.height,
        bpl = header.bytes_per_line,
        bpc = header.bits_per_color,
        bpp = header.bits_per_pixel,
        color_order = 0,
        color_space = 3,
        compression = header.compression,
        page_width_px = page_width_px,
        band_width_bytes = band_width_bytes,
        band_height = band_height,
        hard_margin = hard_margin,
        dst_offset = placement.dst_offset,
        src_skip = placement.src_skip,
        paper_code = paper_size as u8,
        source_code = paper_source as u8,
        copies_sent = sanitize_copies(header.num_copies),
        ppd_key = json_escape(case.media.ppd_key),
    )
}

// ============================================================================
// Çalıştırma ve karşılaştırma
// ============================================================================

fn goldens_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/goldens"))
}

/// Vakayı filtreden geçirip üretilen SPL akışını döner.
fn run_case(case: &Case) -> Vec<u8> {
    let args = CupsFilterArgs {
        job_id: Some("1".to_string()),
        user: Some(GOLDEN_USER.to_string()),
        title: Some(GOLDEN_TITLE.to_string()),
        num_copies: None,
        options: None,
        filename: None,
    };

    let raster = build_raster(case);
    dump_raster_if_requested(case, &raster);

    let mut out: Vec<u8> = Vec::new();
    process_cups_raster_to_spl(
        &args,
        Box::new(Cursor::new(raster)),
        &mut out,
        GOLDEN_SERVICE_DATE,
    )
    .unwrap_or_else(|e| panic!("altın vaka '{}' işlenemedi: {}", case.name, e));
    out
}

fn update_requested() -> bool {
    std::env::var_os("UPDATE_GOLDENS").is_some()
}

/// `DUMP_GOLDEN_RASTER=<dizin>` ayarlıysa, vakanın ÜRETİLEN raster girdisini
/// o dizine yazar.
///
/// İki işe yarıyor: (1) altın dosyalar depoda dururken girdiler durmuyor, bu
/// yüzden bir sapmayı elle incelemek gerektiğinde girdiyi yeniden elde etmenin
/// yolu bu; (2) bu koşum takımı filtreyi SÜREÇ İÇİNDE çağırıyor, oysa CUPS
/// kurulmuş ikiliyi çalıştırır — dökülen girdi, ikisinin aynı baytları
/// ürettiğini doğrulamayı mümkün kılar:
///
/// ```text
/// DUMP_GOLDEN_RASTER=/tmp/r cargo test golden
/// ./target/release/rastertospl-rust 1 tester golden 1 '' /tmp/r/a4-600-marks.raster > /tmp/out.spl
/// ```
///
/// (İkilinin çıktısı yalnızca `SERVICEDATE` satırında ayrılır; o satır altın
/// dosyalarda sabitlenmiştir, ikilide ise bugünün tarihidir.)
fn dump_raster_if_requested(case: &Case, raster: &[u8]) {
    let Some(dir) = std::env::var_os("DUMP_GOLDEN_RASTER") else {
        return;
    };
    let dir = PathBuf::from(dir);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = fs::write(dir.join(format!("{}.raster", case.name)), raster);
}

/// Üretilen içeriği altın dosyayla karşılaştırır; `UPDATE_GOLDENS` ayarlıysa
/// dosyayı yeniler.
fn compare_or_update(path: PathBuf, produced: &[u8], case_name: &str) {
    if update_requested() {
        fs::create_dir_all(goldens_dir()).expect("goldens/ oluşturulamadı");
        fs::write(&path, produced)
            .unwrap_or_else(|e| panic!("{} yazılamadı: {}", path.display(), e));
        return;
    }

    let expected = fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "Altın dosya okunamadı: {} ({}). İlk üretim için: UPDATE_GOLDENS=1 cargo test golden",
            path.display(),
            e
        )
    });

    if expected == produced {
        return;
    }

    // Farkın NEREDE başladığını söyle: 28 KB'lık iki akışı gözle karşılaştırmak
    // mümkün değil, ama ilk farklı bayt genelde hangi aşamanın (PJL başlığı,
    // sayfa başlığı, bant kaydı) değiştiğini doğrudan gösterir.
    let first_diff = expected
        .iter()
        .zip(produced.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected.len().min(produced.len()));

    let ctx = |data: &[u8]| {
        let start = first_diff.saturating_sub(8);
        let end = (first_diff + 8).min(data.len());
        data[start..end]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    };

    panic!(
        "ALTIN DOSYA SAPMASI — vaka '{}'\n\
         dosya      : {}\n\
         beklenen   : {} bayt\n\
         üretilen   : {} bayt\n\
         ilk fark   : ofset {}\n\
         beklenen   : {}\n\
         üretilen   : {}\n\
         \n\
         Yazıcıya giden baytlar değişti. Bu KASITLI bir davranış değişikliğiyse\n\
         `UPDATE_GOLDENS=1 cargo test golden` ile yenileyin ve diff'i incelemeye\n\
         dahil edin; değilse bir regresyondur.",
        case_name,
        path.display(),
        expected.len(),
        produced.len(),
        first_diff,
        ctx(&expected),
        ctx(produced),
    );
}

// ============================================================================
// Testler
// ============================================================================

/// Korpustaki her vaka için üretilen SPL akışı ve yan dosya, commit edilmiş
/// altın dosyalarla BAYT BAYT aynı olmalıdır.
#[test]
fn test_goldens_match() {
    let dir = goldens_dir();
    for case in CASES {
        let produced = run_case(case);
        compare_or_update(dir.join(format!("{}.spl", case.name)), &produced, case.name);

        let sidecar = build_sidecar(case);
        compare_or_update(
            dir.join(format!("{}.json", case.name)),
            sidecar.as_bytes(),
            case.name,
        );
    }
}

/// Aynı sayfa, satır-RLE'li (`RaS2`) ve sıkıştırmasız (`RaS3`) girdiden
/// üretildiğinde BAYT BAYT aynı SPL akışını vermelidir.
///
/// Bu, PAPPL'e geçişte özellikle önemli: PWG Raster'ın gövde kodlaması
/// `RaS2` ile aynıdır, yani bu eşitlik bozulursa çözücü tarafında bir sapma
/// var demektir.
#[test]
fn test_golden_v2_and_v3_agree() {
    let v3 = CASES
        .iter()
        .find(|c| c.name == "a4-600-marks")
        .expect("a4-600-marks vakası korpusta olmalı");
    let v2 = CASES
        .iter()
        .find(|c| c.name == "a4-600-marks-v2rle")
        .expect("a4-600-marks-v2rle vakası korpusta olmalı");

    assert_eq!(
        run_case(v3),
        run_case(v2),
        "RaS2 ve RaS3 girdileri aynı SPL akışını üretmeli"
    );
}

/// `px_from_pt` gerçek `cups-filters` çıktısını yeniden üretmelidir.
///
/// Ölçümler `test_validate_page_header_accepts_real_cupsfilter_heights`
/// içindeki tablodan alındı; oradaki `cupsHeight` değerleri gerçek
/// çalıştırmalardan geliyor. Yuvarlama kuralı (`round`, `ceil`/`floor` değil)
/// altın korpusun geometrisinin gerçekçi olmasını sağlar.
#[test]
fn test_golden_geometry_matches_measured_cupsfilter_output() {
    // (basılabilir yükseklik pt, y_dpi, ölçülen cupsHeight)
    let measured = [
        (818u32, 300u32, 3408u32), // A4   (830 - 12)
        (818, 600, 6817),          // A4
        (818, 1200, 13633),        // A4
        (768, 600, 6400),          // Letter (780 - 12)
        (984, 600, 8200),          // Legal  (996 - 12)
        (984, 1200, 16400),        // Legal
        (396, 600, 3300),          // A6     (408 - 12)
        (911, 1200, 15183),        // Folio  (923 - 12)
    ];
    for (imageable_pt, dpi, expected) in measured {
        assert_eq!(
            px_from_pt(imageable_pt, dpi),
            expected,
            "{} pt @ {} DPI",
            imageable_pt,
            dpi
        );
    }
}

/// Korpustaki medya tanımları PPD'nin kendisiyle uyuşmalıdır.
///
/// Altın dosyaların geometrisi PPD'den türetiliyor; PPD değişip bu tablo
/// değişmezse altın dosyalar sessizce gerçek dışı bir geometriyi dondurmaya
/// devam ederdi.
#[test]
fn test_golden_media_matches_ppd() {
    let ppd = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/ppd/samsung-ml2160.ppd"
    ))
    .expect("PPD okunamadı");

    for media in [A4, LETTER, A5, ENV_C5] {
        let dim_line = format!("*PaperDimension {}/", media.ppd_key);
        let dim = ppd
            .lines()
            .find(|l| l.starts_with(&dim_line))
            .unwrap_or_else(|| panic!("PPD'de *PaperDimension {} yok", media.ppd_key));
        let dim_values: Vec<u32> = dim
            .split('"')
            .nth(1)
            .expect("*PaperDimension değeri tırnak içinde olmalı")
            .split_whitespace()
            .map(|v| v.parse().expect("*PaperDimension sayısal olmalı"))
            .collect();
        assert_eq!(
            dim_values,
            vec![media.dimension_pt.0, media.dimension_pt.1],
            "{} *PaperDimension uyuşmuyor",
            media.ppd_key
        );

        let area_line = format!("*ImageableArea {}/", media.ppd_key);
        let area = ppd
            .lines()
            .find(|l| l.starts_with(&area_line))
            .unwrap_or_else(|| panic!("PPD'de *ImageableArea {} yok", media.ppd_key));
        let area_values: Vec<u32> = area
            .split('"')
            .nth(1)
            .expect("*ImageableArea değeri tırnak içinde olmalı")
            .split_whitespace()
            .map(|v| v.parse().expect("*ImageableArea sayısal olmalı"))
            .collect();
        let (l, b, r, t) = media.imageable_pt;
        assert_eq!(
            area_values,
            vec![l, b, r, t],
            "{} *ImageableArea uyuşmuyor",
            media.ppd_key
        );
    }
}

/// Korpus, kullanıcının istediği kapsamı gerçekten sağlamalı: A4 ve Letter
/// için DESTEKLENEN HER ÇÖZÜNÜRLÜKTE bir hizalama işareti vakası.
///
/// Bu test korpusun kendisini denetler; bir vaka yanlışlıkla silinirse ya da
/// PPD'ye yeni bir çözünürlük eklenirse kırılır.
#[test]
fn test_golden_corpus_covers_marks_on_a4_and_letter_at_every_resolution() {
    let resolutions = [(300, 300), (600, 600), (1200, 600), (1200, 1200)];
    for media in [A4, LETTER] {
        for res in resolutions {
            let found = CASES.iter().any(|c| {
                c.media.ppd_key == media.ppd_key
                    && c.resolution == res
                    && c.content == Content::RegistrationMarks
                    && c.encoding == Encoding::V3
            });
            assert!(
                found,
                "korpusta {} @ {}x{} için hizalama işareti vakası yok",
                media.ppd_key, res.0, res.1
            );
        }
    }
}

/// Sabit servis tarihi gerçekten etkili olmalı: altın akış, bugünün tarihini
/// DEĞİL `GOLDEN_SERVICE_DATE`'i taşımalıdır.
///
/// Aksi hâlde altın dosyalar gece yarısı kırılır ve karşılaştırmayı
/// gevşetme baskısı doğar — ki bu, gerçek sapmaları da gizlerdi.
#[test]
fn test_golden_service_date_is_pinned() {
    let case = CASES
        .iter()
        .find(|c| c.name == "a4-600-blank")
        .expect("a4-600-blank vakası korpusta olmalı");
    let out = run_case(case);

    let expected = format!("@PJL DEFAULT SERVICEDATE={}\n", GOLDEN_SERVICE_DATE);
    assert!(
        out.windows(expected.len())
            .any(|w| w == expected.as_bytes()),
        "altın akış sabit servis tarihini taşımalı"
    );

    let today = format!("@PJL DEFAULT SERVICEDATE={}\n", current_service_date());
    if today != expected {
        assert!(
            !out.windows(today.len()).any(|w| w == today.as_bytes()),
            "altın akış bugünün tarihini taşımamalı"
        );
    }
}
