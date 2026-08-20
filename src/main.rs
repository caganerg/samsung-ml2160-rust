pub mod raster;
pub mod spl;

use std::env;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::process;

use raster::{CupsColorSpace, CupsRasterReader, PageHeader};
use spl::{
    JobConfig, PageConfig, SplDuplex, SplPaperSize, SplPaperSource, SplResolution, SplStreamWriter,
};

/// CUPS Filtre Argümanları
/// Standart çağrı: `filter job-id user title num-copies options [filename]`
#[allow(dead_code)]
struct CupsFilterArgs {
    pub job_id: Option<String>,
    pub user: Option<String>,
    pub title: Option<String>,
    pub num_copies: Option<String>,
    pub options: Option<String>,
    pub filename: Option<String>,
}

impl CupsFilterArgs {
    fn parse() -> Self {
        let args: Vec<String> = env::args().collect();
        if args.len() >= 6 {
            Self {
                job_id: Some(args[1].clone()),
                user: Some(args[2].clone()),
                title: Some(args[3].clone()),
                num_copies: Some(args[4].clone()),
                options: Some(args[5].clone()),
                filename: args.get(6).cloned(),
            }
        } else if args.len() == 2 && !args[1].starts_with('-') {
            // Doğrudan dosya modu: `cargo run -- dosya.raster`
            Self {
                job_id: None,
                user: None,
                title: None,
                num_copies: None,
                options: None,
                filename: Some(args[1].clone()),
            }
        } else {
            Self {
                job_id: None,
                user: None,
                title: None,
                num_copies: None,
                options: None,
                filename: None,
            }
        }
    }
}

fn main() {
    // CUPS filtreleri normalde `filter job-id user title copies options [file]`
    // (5-6 argüman) ile çağrılır. Programın hiç argümansız (yalnızca ikili
    // dosya adıyla) çalıştırılması gerçek bir `cupsd` çağrısı değildir — ya
    // yanlış kurulumdur ya da elle/keşif amaçlı bir çalıştırmadır. Bu araca
    // en yakın mimari eşdeğerler olan `/usr/lib/cups/filter/rastertopwg` ve
    // `pstops` çalıştırılarak doğrulandı: ikisi de bu durumda bir
    // "Usage: ..." mesajı basıp 1 koduyla çıkıyor; CUPS *backend*'lerine
    // özgü olan "desteklenen MIME türlerini listeleyip 0 ile çık" davranışını
    // UYGULAMIYORLAR (o davranış filtreler için değil, aygıt keşfi yapan
    // backend'ler içindir). Bu yüzden burada da aynı yaklaşım izleniyor: boş
    // stdin okumayı denemeden önce net bir kullanım mesajıyla erken çıkılıyor.
    if env::args().count() <= 1 {
        let prog = env::args()
            .next()
            .unwrap_or_else(|| "rastertospl-rust".to_string());
        eprintln!("Usage: {} job-id user title copies options [file]", prog);
        process::exit(1);
    }

    let args = CupsFilterArgs::parse();

    // `user`, `title` ve `job_id`, işi gönderen istemciden (CUPS üzerinden)
    // geldiği için güvenilmez kabul edilmeli: `{}` yerine `{:?}` (Debug)
    // kullanmak, gömülü ANSI/terminal kaçış dizilerini ve kontrol
    // karakterlerini (ör. ESC, CR) `\u{1b}` gibi kaçırılmış (escaped)
    // biçimde yazdırarak sahte log satırı enjeksiyonunu ya da terminal
    // emülatörü zafiyetlerinin tetiklenmesini önler. Normal akışta `job_id`
    // sayısal bir dize olsa da, filtre manipüle edilmiş argümanlarla elle
    // çağrıldığında bu garanti yoktur.
    if let (Some(job), Some(user)) = (&args.job_id, &args.user) {
        eprintln!("DEBUG: CUPS Job ID: {:?}, User: {:?}", job, user);
    }
    if let Some(title) = &args.title {
        eprintln!("DEBUG: CUPS Title: {:?}", title);
    }

    let input_reader: Box<dyn Read> = match &args.filename {
        Some(path) => {
            eprintln!("DEBUG: CUPS Raster dosyadan okunuyor: {}", path);
            match File::open(path) {
                Ok(file) => Box::new(BufReader::new(file)),
                Err(err) => {
                    eprintln!("ERROR: Raster dosyası açılamadı '{}': {}", path, err);
                    process::exit(1);
                }
            }
        }
        None => {
            eprintln!("DEBUG: CUPS Raster standart girdiden (stdin) okunuyor");
            Box::new(BufReader::new(io::stdin()))
        }
    };

    if let Err(err) = process_cups_raster_to_spl(&args, input_reader) {
        eprintln!("ERROR: Raster işleme hatası: {}", err);
        process::exit(1);
    }
}

/// Sayfa başlığı alanlarının makul sınırlar içinde olduğunu doğrular.
///
/// Bozuk ya da kötü niyetli bir CUPS Raster akışı aşırı büyük `bytesPerLine`,
/// yükseklik veya çözünürlük değerleri bildirebilir; bu değerler doğrudan
/// tampon boyutu hesaplarında kullanıldığından, doğrulanmadan geçirilmeleri
/// devasa/aşırı bellek tahsisine (OOM) ya da sessizce taşan hesaplamalara yol
/// açabilir. Bu sınırlar gerçekçi yazıcı donanımının çok üzerinde, sadece
/// açıkça saçma değerleri elemek için var.
fn validate_page_header(header: &PageHeader) -> io::Result<()> {
    // Sınırlar keyfi değil: ppd/samsung-ml2160.ppd'nin tanımladığı en yüksek
    // çözünürlükten (1200 DPI) ve en büyük kağıttan (Legal, 612x1008 pt) türetildi,
    // makul bir pay (yaklaşık %25) bırakıldı. Eski sınırlar (1.000.000 bayt/satır,
    // 10.000 DPI, 100.000 pt) bu donanımın fiziksel olarak üretebileceğinin
    // ~100 katı üzerindeydi; bozuk/kötü niyetli bir başlık bu boşluğu kullanıp
    // devasa bant tamponları (bkz. main.rs stream_page_bands) tahsis ettirebilirdi.
    const MAX_DPI: u32 = 1200; // PPD'deki en yüksek *Resolution seçeneği
    const MAX_POINTS: u32 = 1300; // En büyük *PageSize (Legal: 1008 pt) + pay
    // ~1300pt * 1200dpi / 72 / 8 ≈ 2709 B (1-bit); yuvarlanıp pay bırakıldı.
    const MAX_BYTES_PER_LINE: u32 = 4096;
    // ~1300pt * 1200dpi / 72 ≈ 21667 satır; yuvarlanıp pay bırakıldı.
    const MAX_LINES: u32 = 24_000;

    let invalid = |msg: String| Err(io::Error::new(io::ErrorKind::InvalidData, msg));

    if header.bytes_per_line == 0 || header.bytes_per_line > MAX_BYTES_PER_LINE {
        return invalid(format!(
            "Geçersiz cupsBytesPerLine değeri: {}",
            header.bytes_per_line
        ));
    }
    if header.height == 0 || header.height > MAX_LINES {
        return invalid(format!(
            "Geçersiz sayfa yüksekliği (satır sayısı): {}",
            header.height
        ));
    }
    if header.hw_resolution[0] == 0
        || header.hw_resolution[0] > MAX_DPI
        || header.hw_resolution[1] == 0
        || header.hw_resolution[1] > MAX_DPI
    {
        return invalid(format!("Geçersiz çözünürlük: {:?}", header.hw_resolution));
    }
    if header.page_size_points[0] == 0
        || header.page_size_points[0] > MAX_POINTS
        || header.page_size_points[1] == 0
        || header.page_size_points[1] > MAX_POINTS
    {
        return invalid(format!(
            "Geçersiz sayfa boyutu (pt): {:?}",
            header.page_size_points
        ));
    }

    // ML-2160 serisi QPDL motoru tek düzlemli, 1-bit monokrom (K) raster
    // bekler: stream_page_bands her baytı doğrudan tek bir siyah/beyaz
    // düzlem olarak yorumlayıp koşulsuz tersliyor (bkz. o fonksiyondaki
    // polarite açıklaması). Bu varsayımla uyuşmayan bir akış (ör. 24-bit RGB
    // ya da 32-bit CMYK) sessizce 1-bit monokrom sanılıp yazıcıya
    // gönderilirse şerit hizalaması bozulur, firmware senkronizasyonu
    // kaybolur ve gereksiz toner tüketimine yol açar; bu yüzden erken
    // reddediyoruz.
    if header.color_space != CupsColorSpace::K {
        return invalid(format!(
            "Desteklenmeyen renk uzayı: {} (yalnızca 1-bit K/monokrom destekleniyor)",
            header.color_space
        ));
    }
    if header.bits_per_color != 1 || header.bits_per_pixel != 1 {
        return invalid(format!(
            "Desteklenmeyen bit derinliği: bitsPerColor={}, bitsPerPixel={} (yalnızca 1-bit monokrom destekleniyor)",
            header.bits_per_color, header.bits_per_pixel
        ));
    }
    let expected_bytes_per_line =
        (header.width as u64 * header.bits_per_pixel as u64).div_ceil(8);
    if expected_bytes_per_line != header.bytes_per_line as u64 {
        return invalid(format!(
            "cupsBytesPerLine ({}) cupsWidth ({}) ile tutarsız (beklenen: {})",
            header.bytes_per_line, header.width, expected_bytes_per_line
        ));
    }

    Ok(())
}

/// SpliX document.cpp'deki `pageWidth` hesaplamasının Rust karşılığı.
///
/// SpliX kaynak kodu:
///   pageWidth = ((unsigned long)ceil(convertToXResolution(
///       request.printer()->pageWidth())) + 7) & ~7;
///
/// page_size_pt: Sayfa genişliği (1/72 inç, CUPS header.PageSize[0])
/// x_dpi: Yatay çözünürlük (CUPS header.HWResolution[0])
fn compute_page_width_pixels(page_size_pt: u32, x_dpi: u32) -> u32 {
    let px = (page_size_pt as f64 * x_dpi as f64 / 72.0).ceil() as u32;
    (px + 7) & !7u32
}

/// CUPS Raster başlığındaki `num_copies` (u32) alanını QPDL'nin 16-bit kopya
/// sayısı alanına güvenle sığacak şekilde normalize eder.
///
/// Önceki `header.num_copies.max(1) as u16` ifadesi, 65536 (2^16) ve katları
/// gibi değerlerde sessizce 0'a taşıyordu (`u16::MAX + 1 == 0`); bu da
/// yazıcıya fiilen "0 kopya bas" komutu gönderilmesine yol açardı.
/// `clamp(1, u16::MAX)` hem alt hem üst sınırı aynı anda garanti eder: 0
/// asla geçmez, aşırı büyük değerler ise sessizce taşmak yerine 16-bit
/// alanın sığdırabildiği en büyük değere sabitlenir.
fn sanitize_copies(num_copies: u32) -> u16 {
    num_copies.clamp(1, u16::MAX as u32) as u16
}

/// Standart CUPS Raster akışını okur ve Samsung QPDL/SPL2 formatına dönüştürür.
fn process_cups_raster_to_spl(args: &CupsFilterArgs, reader: Box<dyn Read>) -> io::Result<()> {
    // 1. CUPS Raster başlık/magic kontrolü (RaSt, RaS2, RaS3 vb.)
    let mut raster_reader = CupsRasterReader::new(reader)?;

    eprintln!(
        "INFO: Geçerli CUPS Raster akışı tespit edildi (Sürüm: {:?}, Endian: {})",
        raster_reader.version(),
        if raster_reader.version().is_big_endian() {
            "Big Endian"
        } else {
            "Little Endian"
        }
    );

    let mut spl_writer = SplStreamWriter::new(io::stdout());

    // İlk sayfa başlığını, işi başlatmadan (begin_job) önce oku: CUPS Raster
    // duplex bilgisini SAYFA başlığında taşır, ama PJL iş başlığı duplex'i
    // İŞ seviyesinde bildirmek zorunda. Bu yüzden ilk başlığı "peek" edip iş
    // yapılandırmasını buna göre kuruyoruz; döngüde tekrar okumuyoruz.
    let mut next_header = raster_reader.next_page_header()?;

    let job_duplex = match &next_header {
        Some(h) if h.duplex => SplDuplex::LongEdge,
        _ => SplDuplex::Simplex,
    };

    // 2. Samsung ML-2160 serisi PJL Başlığı (@PJL ENTER LANGUAGE = QPDL)
    let job_config = JobConfig {
        job_name: args.title.clone().unwrap_or_else(|| "CUPS Document".to_string()),
        user_name: args.user.clone().unwrap_or_else(|| "guest".to_string()),
        service_date: "20120101".to_string(),
        duplex: job_duplex,
    };
    spl_writer.begin_job(&job_config)?;

    let mut page_number = 0;

    // 3. Sayfa Döngüsü
    while let Some(header) = next_header.take() {
        validate_page_header(&header)?;

        page_number += 1;

        eprintln!("PAGE: {} {}", page_number, header.num_copies.max(1));
        eprintln!("INFO: Sayfa {} başlatılıyor...", page_number);

        print_header_info(page_number, &header);

        // SpliX pageWidth hesabı: fiziksel sayfa genişliğini DPI ile piksele çevir, 8'e hizala
        // SpliX document.cpp: pageWidth = ((ceil(pageSizePt * dpi / 72) + 7) & ~7)
        let page_width_pixels = compute_page_width_pixels(
            header.page_size_points[0],
            header.hw_resolution[0],
        );

        // SpliX compress.cpp (M2026 öncesi orijinal mantık):
        //   bandWidthInB = lineWidthInB = (pageWidth + 7) / 8
        //   bandWidth = bandWidthInB * 8
        // ML-2160 serisi için 256-hizalama kullanılmıyor.
        let band_width_bytes = (page_width_pixels + 7) / 8;
        let band_width_pixels = band_width_bytes * 8;

        // QPDL bant kaydındaki genişlik/yükseklik alanları 16-bit'tir (spl.rs
        // write_compressed_band). Bu sınırı aşan bir değeri sessizce `as u16`
        // ile kırpmak, yazıcıya bildirilen genişlik ile gerçek payload'ın
        // uyuşmamasına (DMA/RLE çözme senkron kaybı) yol açar; bunun yerine
        // erken ve net bir hata döndürüyoruz.
        if band_width_pixels > u16::MAX as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Bant genişliği QPDL'nin 16-bit alanına sığmıyor: {} px",
                    band_width_pixels
                ),
            ));
        }

        // CUPS raster verisi (596B) ile bant genişliği (620B) farkı = margin
        let cups_line_bytes = header.bytes_per_line as usize;
        let margin_bytes = if (band_width_bytes as usize) > cups_line_bytes {
            ((band_width_bytes as usize) - cups_line_bytes) / 2
        } else {
            if (band_width_bytes as usize) < cups_line_bytes {
                eprintln!(
                    "WARNING: Hesaplanan bant genişliği ({} B) CUPS satır genişliğinden ({} B) dar; \
                     satırların sağ kenarı kırpılacak.",
                    band_width_bytes, cups_line_bytes
                );
            }
            0
        };

        eprintln!(
            "DEBUG: QPDL Genişlik: cupsWidth={}, pageWidthPx={}, bandWidthPx={}, bandWidthB={}, marginB={}",
            header.width, page_width_pixels, band_width_pixels, band_width_bytes, margin_bytes
        );

        let page_config = PageConfig {
            paper_size: SplPaperSize::from_dimensions_pt(
                header.page_size_points[0],
                header.page_size_points[1],
            ),
            paper_source: SplPaperSource::Auto,
            resolution: SplResolution::from_dpi(header.hw_resolution[0]),
            duplex: if header.duplex {
                SplDuplex::LongEdge
            } else {
                SplDuplex::Simplex
            },
            copies: sanitize_copies(header.num_copies),
            // SpliX qpdl.cpp renderPage: width = page->width() = pageWidth
            width_pixels: band_width_pixels,
            height_pixels: header.height,
            qpdl_version: 3,
        };

        // 17-Baytlık QPDL Sayfa Başlığı
        spl_writer.begin_page(&page_config)?;

        // Sayfa şeritlerini SpliX uyumlu stride ile aktar
        stream_page_bands(
            &mut raster_reader,
            &mut spl_writer,
            &header,
            band_width_pixels,
            band_width_bytes,
            margin_bytes,
        )?;

        // 3-Baytlık QPDL Sayfa Sonu
        spl_writer.end_page(sanitize_copies(header.num_copies))?;

        eprintln!("INFO: Sayfa {} tamamlandı.\n", page_number);

        next_header = raster_reader.next_page_header()?;
    }

    if page_number == 0 {
        eprintln!("WARNING: CUPS Raster akışında sayfa bulunamadı.");
    } else {
        // İş Sonu (PJL UEL)
        spl_writer.end_job()?;
        eprintln!(
            "INFO: Toplam {} sayfa başarıyla SPL/QPDL formatına dönüştürüldü.",
            page_number
        );
    }

    io::stdout().flush()?;
    Ok(())
}

/// Raster verisini SpliX uyumlu şeritler (bands) halinde yazar.
///
/// SpliX document.cpp + compress.cpp akışını takip eder:
/// 1. CUPS raster satırını okur (cupsBytesPerLine bayt)
/// 2. bandWidthInB genişliğinde bir şerit tampona, margin ile ortalayarak kopyalar
/// 3. Şeridi Algo 0x11 RLE ile sıkıştırır
/// 4. QPDL Record 0x0C + Subheader 0x09ABCDEF + Payload + Checksum olarak yazar
fn stream_page_bands<R: Read, W: Write>(
    raster_reader: &mut CupsRasterReader<R>,
    spl_writer: &mut SplStreamWriter<W>,
    header: &PageHeader,
    band_width_pixels: u32,
    band_width_bytes: u32,
    margin_bytes: usize,
) -> io::Result<()> {
    let cups_bytes_per_line = header.bytes_per_line as usize;
    let total_lines = header.height as usize;
    let bw_bytes = band_width_bytes as usize;

    // SpliX spl2basic.defs: BandSize = 128
    let band_height: usize = 128;

    // CUPS raster satır okuma tamponu
    let mut line_buffer = vec![0u8; cups_bytes_per_line];

    // Kopyalanacak bayt sayısı: CUPS satırı bant genişliğine sığmalı
    let bytes_to_copy = cups_bytes_per_line.min(bw_bytes - margin_bytes);

    let mut current_line = 0;
    let stream = raster_reader.stream_mut();

    while current_line < total_lines {
        let lines_in_this_band = (total_lines - current_line).min(band_height);

        // Band tamponu: bandWidthInB × bandHeight bayt, sıfır ile başlat
        let band_size = bw_bytes * band_height;
        let mut band_data = vec![0u8; band_size];

        for y in 0..lines_in_this_band {
            stream.read_exact(&mut line_buffer)?;

            // SpliX algo0x11.h: Algo0x11::reverseLineColumn() == true, yani
            // compress.cpp'deki _compressBandedPage bant tamponunu SÜTUN-ÖNCELİKLİ
            // (transpoze) doldurur: band[x * bandHeight + y] = kaynak[x + margin].
            // Satır-öncelikli (row-major) doldurma, sıkıştırma kendisi doğru
            // çalışsa bile yazıcının transpoze edilmiş/gürültülü bir görüntü
            // çözmesine yol açar.
            for c in 0..bytes_to_copy {
                let col = margin_bytes + c;
                band_data[col * band_height + y] = line_buffer[c];
            }
        }

        // Samsung ML-2160 serisi QPDL lazer motoru, CUPS K renk uzayının TERSİ polariteyle çalışır:
        //   CUPS K:     0 = beyaz (toner yok),  1 = siyah (toner var)
        //   Samsung:    0 = siyah (toner bas),   1 = beyaz (toner yok)
        // Empirik test: tersleme olmadan sayfa simsiyah çıkıyor.
        for b in &mut band_data {
            *b = !*b;
        }

        // QPDL Şerit Kaydı (SpliX uyumlu)
        spl_writer.write_compressed_band(
            band_width_pixels as u16,
            band_height as u16,
            &band_data,
        )?;

        current_line += lines_in_this_band;
    }

    Ok(())
}

/// CUPS Raster sayfa başlığından elde edilen meta verileri formatlayıp stderr'e basar.
fn print_header_info(page_num: u32, header: &PageHeader) {
    eprintln!("--------------------------------------------------");
    eprintln!(" [CUPS RASTER SAYFA {} META VERİLERİ]", page_num);
    eprintln!(
        "  Çözünürlük (DPI): {} x {}",
        header.hw_resolution[0], header.hw_resolution[1]
    );
    eprintln!(
        "  Boyutlar (px)   : {} x {} (Genişlik x Yükseklik)",
        header.width, header.height
    );
    eprintln!(
        "  Sayfa Boyutu(pt): {} x {} pt",
        header.page_size_points[0], header.page_size_points[1]
    );
    if let Some(name) = &header.page_size_name {
        eprintln!("  Medya Adı       : {}", name);
    }
    eprintln!("  Renk Uzayı      : {}", header.color_space);
    eprintln!("  Renk Dizilimi   : {:?}", header.color_order);
    eprintln!("  Kanal Bit Derin.: {}", header.bits_per_color);
    eprintln!("  Piksel Bit Der. : {}", header.bits_per_pixel);
    eprintln!("  Satır Bayt Say. : {} bayt", header.bytes_per_line);
    eprintln!(
        "  Ham Raster Boy. : {} bayt ({:.2} MB)",
        header.total_raster_bytes(),
        header.total_raster_bytes() as f64 / (1024.0 * 1024.0)
    );
    eprintln!(
        "  Çift Taraflı    : {}",
        if header.duplex { "Açık" } else { "Kapalı" }
    );
    eprintln!("  Kopya Sayısı    : {}", header.num_copies);
    eprintln!("--------------------------------------------------");
}

#[cfg(test)]
mod tests {
    use super::*;
    use raster::CupsRasterVersion;

    #[test]
    fn test_sanitize_copies_never_returns_zero() {
        assert_eq!(sanitize_copies(0), 1);
        assert_eq!(sanitize_copies(1), 1);
        assert_eq!(sanitize_copies(5), 5);
        // Eski davranış: `.max(1) as u16` burada 0 döndürüyordu.
        assert_eq!(sanitize_copies(65536), u16::MAX);
        assert_eq!(sanitize_copies(131072), u16::MAX);
        assert_eq!(sanitize_copies(u32::MAX), u16::MAX);
    }

    /// A4, 600 DPI, 1-bit monokrom (K), tutarlı `bytesPerLine`'a sahip
    /// geçerli bir başlık üretir; testler bunu temel alıp tek bir alanı
    /// bozarak `validate_page_header`'ın onu reddettiğini doğrular.
    fn valid_header() -> PageHeader {
        let mut buf = vec![0u8; 1796];
        buf[276..280].copy_from_slice(&600u32.to_be_bytes()); // hw_resolution[0]
        buf[280..284].copy_from_slice(&600u32.to_be_bytes()); // hw_resolution[1]
        buf[352..356].copy_from_slice(&595u32.to_be_bytes()); // page_size_points[0] (A4)
        buf[356..360].copy_from_slice(&842u32.to_be_bytes()); // page_size_points[1]
        buf[372..376].copy_from_slice(&8u32.to_be_bytes()); // width
        buf[376..380].copy_from_slice(&8u32.to_be_bytes()); // height
        buf[384..388].copy_from_slice(&1u32.to_be_bytes()); // bits_per_color
        buf[388..392].copy_from_slice(&1u32.to_be_bytes()); // bits_per_pixel
        buf[392..396].copy_from_slice(&1u32.to_be_bytes()); // bytes_per_line = ceil(8*1/8)
        buf[400..404].copy_from_slice(&3u32.to_be_bytes()); // color_space = K
        PageHeader::parse(&buf, CupsRasterVersion::V2Be).unwrap()
    }

    #[test]
    fn test_validate_page_header_accepts_valid_mono_header() {
        assert!(validate_page_header(&valid_header()).is_ok());
    }

    #[test]
    fn test_validate_page_header_rejects_non_k_color_space() {
        let mut header = valid_header();
        header.color_space = CupsColorSpace::Rgb;
        assert!(validate_page_header(&header).is_err());
    }

    #[test]
    fn test_validate_page_header_rejects_wrong_bit_depth() {
        // 24-bit RGB / 32-bit CMYK gibi çok bitli bir akış simüle edilir.
        let mut header = valid_header();
        header.bits_per_color = 8;
        header.bits_per_pixel = 24;
        assert!(validate_page_header(&header).is_err());
    }

    #[test]
    fn test_validate_page_header_rejects_inconsistent_bytes_per_line() {
        let mut header = valid_header();
        header.bytes_per_line = 999; // width=8, bits_per_pixel=1 ile uyuşmuyor
        assert!(validate_page_header(&header).is_err());
    }

    #[test]
    fn test_validate_page_header_rejects_oversized_fields() {
        let mut over_dpi = valid_header();
        over_dpi.hw_resolution = [9999, 9999];
        assert!(validate_page_header(&over_dpi).is_err());

        let mut over_points = valid_header();
        over_points.page_size_points = [999_999, 999_999];
        assert!(validate_page_header(&over_points).is_err());

        let mut over_lines = valid_header();
        over_lines.height = 9_999_999;
        assert!(validate_page_header(&over_lines).is_err());

        let mut over_bpl = valid_header();
        over_bpl.width = 9_999_999;
        over_bpl.bytes_per_line = 9_999_999;
        assert!(validate_page_header(&over_bpl).is_err());
    }
}
