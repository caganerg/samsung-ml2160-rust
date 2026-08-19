pub mod raster;
pub mod spl;

use std::env;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::process;

use raster::{CupsRasterReader, PageHeader};
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
    let args = CupsFilterArgs::parse();

    if let (Some(job), Some(user)) = (&args.job_id, &args.user) {
        eprintln!("DEBUG: CUPS Job ID: {}, User: {}", job, user);
    }
    if let Some(title) = &args.title {
        eprintln!("DEBUG: CUPS Title: {}", title);
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

/// SpliX compress.cpp'deki `bufferWidth` hesaplamasının Rust karşılığı.
///
/// SpliX kaynak kodu:
///   bufferWidth = page->width() & ~255;
///   if ((bufferWidth + 128) < page->width())
///       bufferWidth += 256;
///
/// Bu, 256-piksel hizalamalı (nearest-256 yuvarlama) bir genişlik üretir.
/// Samsung QPDL DMA motoru bu hizalamayı bekler.
fn compute_qpdl_buffer_width(page_width_pixels: u32) -> u32 {
    let mut bw = page_width_pixels & !255u32;
    if (bw + 128) < page_width_pixels {
        bw += 256;
    }
    bw
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

    // 2. Samsung ML-2165 PJL Başlığı (@PJL ENTER LANGUAGE = QPDL)
    let job_config = JobConfig {
        job_name: args.title.clone().unwrap_or_else(|| "CUPS Document".to_string()),
        user_name: args.user.clone().unwrap_or_else(|| "guest".to_string()),
        service_date: "20120101".to_string(),
        duplex: SplDuplex::Simplex,
    };
    spl_writer.begin_job(&job_config)?;

    let mut page_number = 0;

    // 3. Sayfa Döngüsü
    while let Some(header) = raster_reader.next_page_header()? {
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
        // ML-2165 için 256-hizalama kullanılmıyor.
        let band_width_bytes = (page_width_pixels + 7) / 8;
        let band_width_pixels = band_width_bytes * 8;

        // CUPS raster verisi (596B) ile bant genişliği (620B) farkı = margin
        let cups_line_bytes = header.bytes_per_line as usize;
        let margin_bytes = if (band_width_bytes as usize) > cups_line_bytes {
            ((band_width_bytes as usize) - cups_line_bytes) / 2
        } else {
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
            copies: header.num_copies.max(1) as u16,
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
        spl_writer.end_page(header.num_copies.max(1) as u16)?;

        eprintln!("INFO: Sayfa {} tamamlandı.\n", page_number);
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

            // SpliX document.cpp'deki kopyalama:
            //   memcpy(planes[i] + index + marginWidthInB, line + clippingX, bytesToCopy)
            // CUPS verisini margin ile ortalayarak bant tamponuna kopyala
            let dst_offset = y * bw_bytes + margin_bytes;
            band_data[dst_offset..dst_offset + bytes_to_copy]
                .copy_from_slice(&line_buffer[..bytes_to_copy]);
        }

        // Samsung ML-2165 QPDL lazer motoru, CUPS K renk uzayının TERSİ polariteyle çalışır:
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
