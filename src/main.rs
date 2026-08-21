pub mod raster;
pub mod spl;

use std::env;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::process;

use raster::{CupsColorOrder, CupsColorSpace, CupsRasterReader, PageHeader};
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
    fn parse(args: &[String]) -> Self {
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
    // `env::args()` argv'de geçersiz UTF-8 baytı bulunca panic atar; ama bu
    // argümanlar (`job-id user title copies options [file]`) `cupsd`
    // tarafından işi gönderen istemcinin verdiği alanlardan (ör. job-name)
    // türetiliyor ve güvenilmez kabul edilmeli. Bozuk/kötü niyetli bir başlık
    // filtreyi daha ilk argümanı okurken çökertip DoS'a yol açmasın diye
    // `env::args_os()` + kayıplı (lossy) UTF-8 dönüşümü kullanılıyor: geçersiz
    // baytlar sessizce `U+FFFD` ile değiştirilir, panic olmaz.
    let raw_args: Vec<String> = env::args_os()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

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
    if raw_args.len() <= 1 {
        let prog = raw_args
            .first()
            .cloned()
            .unwrap_or_else(|| "rastertospl-rust".to_string());
        eprintln!("Usage: {} job-id user title copies options [file]", prog);
        process::exit(1);
    }

    let args = CupsFilterArgs::parse(&raw_args);

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
            eprintln!(
                "DEBUG: CUPS Raster dosyadan okunuyor: {}",
                quote_untrusted(path)
            );
            match File::open(path) {
                Ok(file) => Box::new(BufReader::new(file)),
                Err(err) => {
                    eprintln!(
                        "ERROR: Raster dosyası açılamadı {}: {}",
                        quote_untrusted(path),
                        err
                    );
                    process::exit(1);
                }
            }
        }
        None => {
            eprintln!("DEBUG: CUPS Raster standart girdiden (stdin) okunuyor");
            Box::new(BufReader::new(io::stdin()))
        }
    };

    // `process_cups_raster_to_spl` `SplStreamWriter`'ı YEREL olarak tutar:
    // hata `?` ile döndüğünde writer bu satıra gelmeden düşer ve `Drop`
    // uygulaması kapanış UEL'ini yazar. `process::exit` `Drop` çalıştırmadığı
    // için sıralama önemlidir — hata burada, writer çoktan düştükten sonra
    // raporlanır.
    if let Err(err) = process_cups_raster_to_spl(&args, input_reader, io::stdout()) {
        eprintln!("ERROR: Raster işleme hatası: {}", err);
        process::exit(1);
    }
}

/// Güvenilmez bir dizeyi, log satırına gömülmeye hazır hâle getirir.
///
/// `{:?}` (Debug) biçimi dizeyi tırnak içine alır ve kontrol karakterlerini
/// kaçırır (`\n`, `\u{1b}` gibi). Bu, iki saldırıyı birden kapatır:
/// gömülü bir CR/LF ile `/var/log/cups/error_log`'a sahte bir günlük satırı
/// enjekte etmek, ve gömülü ANSI/OSC dizileriyle logu izleyen yöneticinin
/// terminalini (renk, pencere başlığı) manipüle etmek.
///
/// Bu, `main`'in argv'den gelen `title`/`user` alanları için zaten uyguladığı
/// kalıbın aynısıdır; buradaki yardımcı, aynı politikayı raster başlığından
/// gelen dizeler ve dosya yolları için de tek bir yerde toplar.
fn quote_untrusted(value: &str) -> String {
    format!("{:?}", value)
}

/// Sayfa başlığı alanlarının makul sınırlar içinde olduğunu doğrular.
///
/// Bozuk ya da kötü niyetli bir CUPS Raster akışı aşırı büyük `bytesPerLine`,
/// yükseklik veya çözünürlük değerleri bildirebilir; bu değerler doğrudan
/// tampon boyutu hesaplarında kullanıldığından, doğrulanmadan geçirilmeleri
/// devasa/aşırı bellek tahsisine (OOM) ya da sessizce taşan hesaplamalara yol
/// açabilir. Bu sınırlar gerçekçi yazıcı donanımının çok üzerinde, sadece
/// açıkça saçma değerleri elemek için var.
/// PPD'deki en yüksek `*Resolution` seçeneği (1200 DPI).
pub const MAX_DPI: u32 = 1200;
/// En büyük `*PaperDimension` (Legal: 1008 pt) + makul pay.
pub const MAX_POINTS: u32 = 1300;
/// ~1300 pt * 1200 dpi / 72 / 8 ≈ 2709 B (1-bit); yuvarlanıp pay bırakıldı.
pub const MAX_BYTES_PER_LINE: u32 = 4096;
/// ~1300 pt * 1200 dpi / 72 ≈ 21.667 satır; yuvarlanıp pay bırakıldı.
pub const MAX_LINES: u32 = 24_000;

fn validate_page_header(header: &PageHeader) -> io::Result<()> {
    // Sınırlar keyfi değil: ppd/samsung-ml2160.ppd'nin tanımladığı en yüksek
    // çözünürlükten ve en büyük kağıttan türetildi, makul bir pay bırakıldı.
    // Eski sınırlar (1.000.000 bayt/satır, 10.000 DPI, 100.000 pt) bu
    // donanımın fiziksel olarak üretebileceğinin ~100 katı üzerindeydi;
    // bozuk/kötü niyetli bir başlık bu boşluğu kullanıp devasa bant tamponları
    // (bkz. stream_page_bands) tahsis ettirebilirdi.
    //
    // PPD ile bu sabitler arasındaki bağ artık bir yorum değil, bir test:
    // `test_limits_cover_every_ppd_option` PPD'yi ayrıştırıp her `*Resolution`
    // ve `*PaperDimension` seçeneğinin sınırlar içinde kaldığını doğruluyor.
    // PPD'ye daha büyük bir kağıt ya da daha yüksek çözünürlük eklenirse test
    // kırılır ve sabitlerin birlikte güncellenmesi gerektiğini söyler.
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

    // `cupsColorOrder` bugüne kadar hiç denetlenmiyordu. 1-bit tek kanallı
    // veride Chunked/Banded/Planar dizilimleri BİRBİRİNİN AYNIsıdır (tek
    // düzlem, tek kanal), bu yüzden üçü de kabul edilir; ama tanınmayan bir
    // değer, akışı üreten tarafın bu filtrenin varsaydığından farklı bir
    // düzen kullandığının işaretidir ve sessizce yanlış yorumlanmamalıdır.
    if let CupsColorOrder::Unknown(order) = header.color_order {
        return invalid(format!(
            "Tanınmayan cupsColorOrder değeri: {} (beklenen: 0=Chunked, 1=Banded, 2=Planar)",
            order
        ));
    }

    // D-01: `cupsBytesPerLine`, sayfanın FİZİKSEL genişliğine sığmalı.
    //
    // Bant genişliği `page_size_points` ve `hw_resolution`'dan hesaplanır
    // (bkz. compute_page_width_pixels); satır bundan genişse fazlalık
    // sessizce kırpılıyordu. Yukarıdaki `cupsWidth` tutarlılık kontrolü bunu
    // yakalamaz, çünkü kendi içinde tutarlı ama sayfayla tutarsız bir başlık
    // (ör. `PageSize = 1 pt` + `cupsBytesPerLine = 620`) bantı 1 bayta
    // düşürüp satırların %99,8'ini attırabiliyordu.
    //
    // Payın 1 bayt olmasının nedeni: `page_width_pixels` yukarı doğru 8'e
    // hizalandığı için normalde `bytes_per_line <= band_width_bytes` zaten
    // sağlanır; 1 baytlık pay yalnızca üretici tarafın farklı yuvarlaması
    // ihtimalini karşılar. Bu payın içinde kalan sapma, aşağıdaki sayfa
    // döngüsünde uyarıyla birlikte kırpılmaya devam eder.
    const LINE_OVERSHOOT_SLACK_BYTES: u32 = 1;
    let band_width_bytes =
        compute_page_width_pixels(header.page_size_points[0], header.hw_resolution[0]).div_ceil(8);
    if header.bytes_per_line > band_width_bytes + LINE_OVERSHOOT_SLACK_BYTES {
        return invalid(format!(
            "cupsBytesPerLine ({}) sayfa genişliğine sığmıyor: {} pt @ {} DPI => en fazla {} bayt/satır",
            header.bytes_per_line,
            header.page_size_points[0],
            header.hw_resolution[0],
            band_width_bytes
        ));
    }

    // D-02: `cupsHeight`, sayfanın FİZİKSEL yüksekliğine sığmalı.
    //
    // D-01'in dikey karşılığı. Yükseklik bugüne kadar yalnızca global
    // `MAX_LINES` sınırına karşı denetleniyordu; sayfanın kendi boyutuyla hiç
    // karşılaştırılmıyordu. Kendi içinde tutarlı ama sayfayla tutarsız bir
    // başlık (ör. `PageSize = 595 x 1 pt` + `cupsHeight = 24000`) böylece
    // kabul ediliyor, QPDL sayfa başlığına 24000 satırlık bir yükseklik
    // yazılıyor ve sayfaya sığandan 3-4 kat fazla bant gönderiliyordu.
    // Yazıcıya bildirilen boyut ile gerçekte gönderilen veri miktarının
    // ayrışması, D-01'de olduğu gibi yazıcı tarafında hizalama/senkron kaybı
    // ve gereksiz kâğıt/toner tüketimi anlamına gelir.
    //
    // Payın 8 satır olmasının nedeni: genişlikten farklı olarak burada 8'e
    // hizalama YOK, yani `compute_page_height_lines` fazladan bir baş boşluk
    // bırakmıyor; pay yalnızca üretici tarafın farklı yuvarlaması (ceil yerine
    // round, ya da küçük bir bloğa hizalama) ihtimalini karşılıyor. Gerçek
    // cups-filters çıktısı bu sınırın çok altında kalır, çünkü PPD'nin
    // `*ImageableArea` kenar boşluklarını düşer: A4 @600 DPI'da fiziksel
    // 7017 satıra karşılık üretilen `cupsHeight` 6816'dır (12 pt üst + 12 pt
    // alt = 200 satır eksik); aynı fark 300 DPI'da 100, 1200 DPI'da 400
    // satırdır. Yani meşru hiçbir iş bu kontrole takılmaz.
    const HEIGHT_OVERSHOOT_SLACK_LINES: u32 = 8;
    let page_height_lines =
        compute_page_height_lines(header.page_size_points[1], header.hw_resolution[1]);
    if header.height > page_height_lines + HEIGHT_OVERSHOOT_SLACK_LINES {
        return invalid(format!(
            "cupsHeight ({}) sayfa yüksekliğine sığmıyor: {} pt @ {} DPI => en fazla {} satır",
            header.height,
            header.page_size_points[1],
            header.hw_resolution[1],
            page_height_lines
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

/// Sayfanın fiziksel yüksekliğinin kaç raster satırına karşılık geldiği.
///
/// `compute_page_width_pixels`'in dikey karşılığı, iki farkla: dikey eksende
/// bant/DMA hizalaması gerekmediği için 8'e yuvarlama YOKTUR, ve dikey
/// çözünürlük `hw_resolution[1]`'dir — PPD `1200x600dpi` gibi asimetrik bir
/// seçenek sunduğu için bu ayrım gerçekten tetiklenebilir.
///
/// Yalnızca `validate_page_header`'ın D-02 kontrolünde bir ÜST SINIR olarak
/// kullanılır; sayfa döngüsü satır sayısını (D-01'deki genişlik gibi) buna
/// göre yeniden ölçeklemez, çünkü gönderilecek satır sayısı `cupsHeight`
/// tarafından belirlenir.
fn compute_page_height_lines(page_size_pt: u32, y_dpi: u32) -> u32 {
    (page_size_pt as f64 * y_dpi as f64 / 72.0).ceil() as u32
}

/// Tek bir baskı işinde işlenecek azami sayfa sayısı.
///
/// Sayfa döngüsünün üst sınırı yoktu: akış ne kadar uzunsa o kadar sayfa
/// üretiliyordu. Bu hem doğrudan kâğıt/toner tüketimini sınırsız bırakıyor
/// (`MAX_REALISTIC_COPIES` ile çarpıldığında daha da fazlası), hem de
/// sıkıştırıcının sayfa başına maliyetini (ölçülen en kötü durum: sıkıştırılamaz
/// gürültüde ~7,4 MB/s, yani tam boy bir sayfa için ~6 s CPU) toplamda
/// sınırsız kılıyordu. Filtre CUPS kuyruğunu tek iş parçacığıyla işlediği için
/// uzun bir iş sıradaki tüm işleri bekletir.
///
/// 5.000 sayfa, gerçek bir belge için fazlasıyla cömert (bir kutu kâğıdın on
/// katı) ama sınırsız değil.
const MAX_PAGES_PER_JOB: u32 = 5_000;

/// Gerçekçi bir baskı işi için makul kabul edilen azami kopya sayısı.
///
/// QPDL'nin kopya alanı 16-bit'tir (teorik üst sınır 65535), ama hiçbir
/// gerçek iş bu sınıra yakın bir değer istemez; 999, kağıt/toner israfına
/// veya yazıcının fiziksel olarak saatlerce durmadan basmasına yol açacak
/// bozuk/aşırı bir başlığa karşı ek bir güvenlik payı bırakır.
const MAX_REALISTIC_COPIES: u16 = 999;

/// CUPS Raster başlığındaki `num_copies` (u32) alanını QPDL'nin 16-bit kopya
/// sayısı alanına güvenle sığacak şekilde normalize eder.
///
/// Önceki `header.num_copies.max(1) as u16` ifadesi, 65536 (2^16) ve katları
/// gibi değerlerde sessizce 0'a taşıyordu (`u16::MAX + 1 == 0`); bu da
/// yazıcıya fiilen "0 kopya bas" komutu gönderilmesine yol açardı.
/// `clamp(1, MAX_REALISTIC_COPIES)` hem alt hem üst sınırı aynı anda
/// garanti eder: 0 asla geçmez, aşırı büyük değerler ise sessizce taşmak
/// yerine (16-bit alana teknik olarak sığsa bile) gerçekçi bir üst sınıra
/// sabitlenir.
fn sanitize_copies(num_copies: u32) -> u16 {
    num_copies.clamp(1, MAX_REALISTIC_COPIES as u32) as u16
}

/// Standart CUPS Raster akışını okur ve Samsung QPDL/SPL2 formatına dönüştürür.
///
/// `writer` parametre olarak alınır (doğrudan `io::stdout()` kullanılmaz) ki
/// testler üretilen SPL akışını inceleyebilsin; özellikle hata yollarında
/// kapanış UEL'inin yazıldığını doğrulamak için gerekli.
fn process_cups_raster_to_spl<W: Write>(
    args: &CupsFilterArgs,
    reader: Box<dyn Read>,
    writer: W,
) -> io::Result<()> {
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

    let mut spl_writer = SplStreamWriter::new(writer);

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
        if page_number > MAX_PAGES_PER_JOB {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "İş, sayfa sınırını aştı: {} sayfadan fazlası işlenmiyor. \
                     Belge gerçekten bu kadar uzunsa işi parçalara bölün.",
                    MAX_PAGES_PER_JOB
                ),
            ));
        }

        // `sanitize_copies` ile aynı değer loglanır: aksi halde bu satır,
        // yazıcıya fiilen gönderilen (aşağıda begin_page/end_page ile
        // sanitize_copies() üzerinden yazılan) kopya sayısından farklı,
        // ham/sınırsız `header.num_copies` değerini gösterip yanıltıcı
        // teşhis bilgisi üretebilirdi (ör. 65536 istenirse burada "65536"
        // yazılır ama yazıcıya 999 gönderilirdi).
        eprintln!("PAGE: {} {}", page_number, sanitize_copies(header.num_copies));
        eprintln!("INFO: Sayfa {} başlatılıyor...", page_number);

        print_header_info(page_number, &header);

        // `cupsCompression`, CUPS Raster'da AKIŞ sıkıştırması değil, sürücüye
        // özel bir "cihaz sıkıştırması" ipucudur (akış sıkıştırması sync
        // sözcüğüyle belirlenir; bkz. raster.rs is_compressed). SpliX bu alanı
        // kullanmaz, bu filtre de bant sıkıştırmasını her zaman Algo 0x11 ile
        // yapar; dolayısıyla alan bilinçli olarak yok sayılıyor. Sıfırdan
        // farklıysa, PPD ile bu filtrenin varsayımları arasında bir uyuşmazlık
        // olabileceği için teşhis amaçlı bir kez bildiriyoruz.
        if header.compression != 0 {
            eprintln!(
                "WARNING: cupsCompression={} yok sayıldı; bant sıkıştırması her zaman Algo 0x11 RLE.",
                header.compression
            );
        }

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

        // QPDL sayfa/bant kayıtlarındaki genişlik ve yükseklik alanları
        // 16-bit'tir. Değerleri `as u16` ile sessizce kırpmak, yazıcıya
        // bildirilen boyut ile gerçek payload'ın uyuşmamasına (DMA/RLE çözme
        // senkron kaybı) yol açar; bunun yerine `try_into` ile erken ve net
        // bir hata döndürüyoruz. `PageConfig` alanları da `u16` olduğu için
        // dönüşümü atlamak mümkün değil (bkz. spl.rs PageConfig).
        //
        // Her ikisi de pratikte `validate_page_header`'ın sınırları sayesinde
        // zaten sığıyor; bu dönüşümler o dolaylı korumayı doğrudan ve
        // sınırlar değişse bile geçerli kalan bir garantiye çeviriyor.
        let to_u16 = |value: u32, alan: &str| -> io::Result<u16> {
            u16::try_from(value).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{} QPDL'nin 16-bit alanına sığmıyor: {} px",
                        alan, value
                    ),
                )
            })
        };
        let band_width_u16 = to_u16(band_width_pixels, "Bant genişliği")?;
        let page_height_u16 = to_u16(header.height, "Sayfa yüksekliği")?;

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
            // Eksenler AYRI: QPDL `header[0x1]` dikey, `header[0x10]` yatay
            // çözünürlüğü taşır (bkz. spl.rs PageConfig). `1200x600dpi` bu
            // motor ailesinde gerçek bir moddur.
            resolution_x: SplResolution::from_dpi(header.hw_resolution[0]),
            resolution_y: SplResolution::from_dpi(header.hw_resolution[1]),
            duplex: if header.duplex {
                SplDuplex::LongEdge
            } else {
                SplDuplex::Simplex
            },
            copies: sanitize_copies(header.num_copies),
            // SpliX qpdl.cpp renderPage: width = page->width() = pageWidth
            width_pixels: band_width_u16,
            height_pixels: page_height_u16,
            qpdl_version: 3,
        };

        // 17-Baytlık QPDL Sayfa Başlığı
        spl_writer.begin_page(&page_config)?;

        // Sayfa şeritlerini SpliX uyumlu stride ile aktar
        stream_page_bands(
            &mut raster_reader,
            &mut spl_writer,
            &header,
            band_width_u16,
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
        eprintln!(
            "INFO: Toplam {} sayfa başarıyla SPL/QPDL formatına dönüştürüldü.",
            page_number
        );
    }

    // İş Sonu (PJL UEL). Sayfa bulunamamış olsa bile çağrılır: `begin_job`
    // yazıcıyı çoktan QPDL diline soktuğu için akış her hâlükârda kapanış
    // UEL'i ile bitmelidir. `end_job` içeride flush eder.
    spl_writer.end_job()?;
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
    band_width_pixels: u16,
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

    while current_line < total_lines {
        let lines_in_this_band = (total_lines - current_line).min(band_height);

        // Band tamponu: bandWidthInB × bandHeight bayt, sıfır ile başlat
        let band_size = bw_bytes * band_height;
        let mut band_data = vec![0u8; band_size];

        for y in 0..lines_in_this_band {
            raster_reader.read_line(&mut line_buffer)?;

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
        spl_writer.write_compressed_band(band_width_pixels, band_height as u16, &band_data)?;

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
        // `cupsPageSizeName` raster başlığındaki 64 baytlık bir C dizesidir ve
        // işi gönderen istemciden gelir — argv'deki `title`/`user` kadar
        // güvenilmezdir, bu yüzden ham değil kaçırılmış olarak basılır.
        eprintln!("  Medya Adı       : {}", quote_untrusted(name));
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
    use std::io::Cursor;

    #[test]
    fn test_sanitize_copies_never_returns_zero() {
        assert_eq!(sanitize_copies(0), 1);
        assert_eq!(sanitize_copies(1), 1);
        assert_eq!(sanitize_copies(5), 5);
        assert_eq!(sanitize_copies(MAX_REALISTIC_COPIES as u32), MAX_REALISTIC_COPIES);
        // Eski davranış: `.max(1) as u16` burada 0 döndürüyordu.
        assert_eq!(sanitize_copies(65536), MAX_REALISTIC_COPIES);
        assert_eq!(sanitize_copies(131072), MAX_REALISTIC_COPIES);
        assert_eq!(sanitize_copies(u32::MAX), MAX_REALISTIC_COPIES);
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

    /// Geçerli, tek sayfalık, sıkıştırmasız bir V3 Big-Endian raster akışı
    /// üretir. `pixel_bytes`, sayfa verisinin kaç baytının yazılacağını
    /// belirler; başlıkta bildirilenden az vermek kısa okuma (hata) yolunu
    /// tetikler.
    fn v3_stream(pixel_bytes: usize) -> Vec<u8> {
        let mut buf = vec![0u8; 1796];
        let mut put = |off: usize, val: u32| {
            buf[off..off + 4].copy_from_slice(&val.to_be_bytes());
        };
        put(276, 600); // hw_resolution[0]
        put(280, 600); // hw_resolution[1]
        put(352, 595); // page_size_points[0] (A4)
        put(356, 842); // page_size_points[1]
        put(372, 32); // width
        put(376, 4); // height
        put(384, 1); // bits_per_color
        put(388, 1); // bits_per_pixel
        put(392, 4); // bytes_per_line = ceil(32 * 1 / 8)
        put(400, 3); // color_space = K

        let mut stream = b"RaS3".to_vec();
        stream.extend_from_slice(&buf);
        stream.extend_from_slice(&vec![0u8; pixel_bytes]);
        stream
    }

    fn no_args() -> CupsFilterArgs {
        CupsFilterArgs {
            job_id: None,
            user: None,
            title: None,
            num_copies: None,
            options: None,
            filename: None,
        }
    }

    fn count_uel(stream: &[u8]) -> usize {
        stream
            .windows(spl::PJL_UEL.len())
            .filter(|w| *w == spl::PJL_UEL)
            .count()
    }

    /// Y-03 regresyonu (uçtan uca): sayfa verisi yarıda kesilirse dönüşüm
    /// hata döndürmeli, AMA yazıcıya giden akış yine de kapanış UEL'i ile
    /// bitmeli. Aksi hâlde yazıcı QPDL dilinde, yarım bir bant kaydını
    /// bekler hâlde asılı kalır.
    #[test]
    fn test_closing_uel_written_on_truncated_page_data() {
        let stream = v3_stream(2); // 4 satır x 4 bayt = 16 bayt gerekiyordu
        let mut out: Vec<u8> = Vec::new();
        let err = process_cups_raster_to_spl(&no_args(), Box::new(Cursor::new(stream)), &mut out)
            .expect_err("kısa sayfa verisi hata döndürmeliydi");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(
            out.ends_with(spl::PJL_END),
            "hata yolu akışı kapanış UEL'i olmadan bıraktı"
        );
        assert_eq!(count_uel(&out), 2);
    }

    /// Y-03 regresyonu: sayfa başlığı doğrulamadan geçemezse de aynı garanti.
    #[test]
    fn test_closing_uel_written_on_invalid_page_header() {
        let mut stream = v3_stream(16);
        // bytes_per_line'ı 0 yap: validate_page_header reddedecek.
        stream[4 + 392..4 + 396].copy_from_slice(&0u32.to_be_bytes());

        let mut out: Vec<u8> = Vec::new();
        let err = process_cups_raster_to_spl(&no_args(), Box::new(Cursor::new(stream)), &mut out)
            .expect_err("geçersiz başlık hata döndürmeliydi");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(out.ends_with(spl::PJL_END));
    }

    /// Akışta hiç sayfa yoksa da iş kapatılmalı: `begin_job` yazıcıyı çoktan
    /// QPDL diline sokmuştur.
    #[test]
    fn test_closing_uel_written_when_stream_has_no_pages() {
        let mut out: Vec<u8> = Vec::new();
        process_cups_raster_to_spl(&no_args(), Box::new(Cursor::new(b"RaS3".to_vec())), &mut out)
            .expect("sayfasız akış hata değil, uyarı üretmeli");
        assert!(out.ends_with(spl::PJL_END));
        assert_eq!(count_uel(&out), 2);
    }

    /// Başarılı akış tam olarak iki UEL içermeli: `Drop`, açıkça kapatılmış
    /// bir işe üçüncü bir UEL eklememelidir.
    #[test]
    fn test_successful_stream_has_exactly_one_uel_pair() {
        let mut out: Vec<u8> = Vec::new();
        process_cups_raster_to_spl(&no_args(), Box::new(Cursor::new(v3_stream(16))), &mut out)
            .expect("geçerli akış başarılı olmalı");
        assert!(out.starts_with(spl::PJL_UEL));
        assert!(out.ends_with(spl::PJL_END));
        assert_eq!(count_uel(&out), 2, "fazladan UEL yazıldı");
    }

    /// Y-01 regresyonu (uçtan uca): sıkıştırılmış v2 akışı, aynı içeriğin
    /// sıkıştırmasız v3 hâliyle BİRE BİR aynı SPL çıktısını üretmeli.
    ///
    /// Eskiden v2 sayfa verisi ham piksel sanılıyordu ve yazıcıya çöp bir
    /// sayfa gidiyordu. Bu test, `CupsLineDecoder`'ın araya girdiğini ve
    /// akışın sürümünün çıktıyı hiç etkilemediğini sabitler.
    #[test]
    fn test_v2_and_v3_streams_produce_identical_output() {
        // 4 bayt/satır, 3 satır: 0xAA 0xAA 0xAA 0x55 / 0x00 x4 / 0xFF x4
        let pixels: [&[u8]; 3] = [
            &[0xAA, 0xAA, 0xAA, 0x55],
            &[0x00, 0x00, 0x00, 0x00],
            &[0xFF, 0xFF, 0xFF, 0xFF],
        ];

        let mut hdr = vec![0u8; 1796];
        {
            let mut put = |off: usize, val: u32| {
                hdr[off..off + 4].copy_from_slice(&val.to_be_bytes());
            };
            put(276, 600);
            put(280, 600);
            put(352, 595);
            put(356, 842);
            put(372, 32);
            put(376, 3);
            put(384, 1);
            put(388, 1);
            put(392, 4);
            put(400, 3);
        }

        let mut v3 = b"RaS3".to_vec();
        v3.extend_from_slice(&hdr);
        for line in pixels {
            v3.extend_from_slice(line);
        }

        // Aynı içeriğin CUPS satır-RLE karşılığı.
        let mut v2 = b"RaS2".to_vec();
        v2.extend_from_slice(&hdr);
        v2.extend_from_slice(&[0x00, 0x02, 0xAA, 0x00, 0x55]); // 0xAA x3, 0x55 x1
        v2.extend_from_slice(&[0x00, 0x80]); // satır sonuna kadar boş (K => 0x00)
        v2.extend_from_slice(&[0x00, 0x03, 0xFF]); // 0xFF x4

        let mut out_v3: Vec<u8> = Vec::new();
        process_cups_raster_to_spl(&no_args(), Box::new(Cursor::new(v3)), &mut out_v3)
            .expect("v3 akışı işlenmeli");
        let mut out_v2: Vec<u8> = Vec::new();
        process_cups_raster_to_spl(&no_args(), Box::new(Cursor::new(v2)), &mut out_v2)
            .expect("v2 akışı işlenmeli");

        assert!(!out_v3.is_empty());
        assert_eq!(out_v2, out_v3, "v2 ve v3 çıktıları ayrıştı");
    }

    /// `validate_page_header` sınırları, PPD'nin sunduğu HER seçeneği
    /// kapsamalı.
    ///
    /// Sabitler PPD'den elle türetilmişti ve aradaki bağ yalnızca bir
    /// yorumdu: PPD'ye daha büyük bir kağıt ya da daha yüksek bir çözünürlük
    /// eklenirse, sabitleri güncellemeyi unutmak meşru işlerin sessizce
    /// reddedilmesine yol açardı. Bu test o bağı zorunlu kılar.
    #[test]
    fn test_limits_cover_every_ppd_option() {
        let ppd = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ppd/samsung-ml2160.ppd"
        ))
        .expect("PPD okunamadı");

        let mut resolutions = 0;
        let mut papers = 0;

        for line in ppd.lines() {
            // *Resolution 1200x600dpi/...  ya da  *Resolution 600dpi/...
            if let Some(rest) = line.strip_prefix("*Resolution ") {
                let name = rest.split('/').next().unwrap_or("");
                let digits = name.trim_end_matches("dpi");
                for part in digits.split('x') {
                    let dpi: u32 = part.parse().unwrap_or_else(|_| {
                        panic!("PPD çözünürlüğü ayrıştırılamadı: {}", line)
                    });
                    assert!(
                        dpi <= MAX_DPI,
                        "PPD {} DPI sunuyor ama MAX_DPI = {}; sabiti güncelleyin",
                        dpi,
                        MAX_DPI
                    );
                    resolutions += 1;
                }
            }

            // *PaperDimension A4/A4: "595 842"
            if let Some(rest) = line.strip_prefix("*PaperDimension ") {
                let dims = rest.split('"').nth(1).unwrap_or("");
                let mut it = dims.split_whitespace();
                let (w, h) = match (it.next(), it.next()) {
                    (Some(w), Some(h)) => (w, h),
                    _ => panic!("PPD kağıt boyutu ayrıştırılamadı: {}", line),
                };
                for value in [w, h] {
                    // Bazı PPD'ler ondalık yazar; tavana yuvarla.
                    let pt = value.parse::<f64>().expect("boyut sayı değil").ceil() as u32;
                    assert!(
                        pt <= MAX_POINTS,
                        "PPD {} pt kağıt sunuyor ama MAX_POINTS = {}; sabiti güncelleyin",
                        pt,
                        MAX_POINTS
                    );
                    // En büyük kağıt en yüksek çözünürlükte satır/sütun
                    // sınırlarına da sığmalı.
                    let pixels = (pt as u64 * MAX_DPI as u64).div_ceil(72);
                    assert!(
                        pixels <= MAX_LINES as u64,
                        "{} pt @ {} DPI = {} satır, MAX_LINES = {}",
                        pt,
                        MAX_DPI,
                        pixels,
                        MAX_LINES
                    );
                    assert!(
                        pixels.div_ceil(8) <= MAX_BYTES_PER_LINE as u64,
                        "{} pt @ {} DPI = {} bayt/satır, MAX_BYTES_PER_LINE = {}",
                        pt,
                        MAX_DPI,
                        pixels.div_ceil(8),
                        MAX_BYTES_PER_LINE
                    );
                }
                papers += 1;
            }
        }

        // PPD gerçekten ayrıştırıldı mı? (Dosya yeniden düzenlenirse sessizce
        // hiçbir şey doğrulamayan bir test kalmasın.)
        assert!(resolutions >= 4, "PPD'den çözünürlük okunamadı: {}", resolutions);
        assert!(papers >= 10, "PPD'den kağıt boyutu okunamadı: {}", papers);
    }

    #[test]
    fn test_validate_page_header_rejects_line_wider_than_page() {
        let mut header = valid_header();
        header.page_size_points = [1, 1];
        header.width = 4960;
        header.bytes_per_line = 620; // cupsWidth ile tutarlı, sayfayla değil
        let err = validate_page_header(&header).expect_err("dar sayfa reddedilmeliydi");
        assert!(
            err.to_string().contains("sayfa genişliğine sığmıyor"),
            "hata nedeni açıklanmalı: {}",
            err
        );
    }

    /// Gerçek bir tam genişlik A4 sayfası (595 pt @ 600 DPI = 620 B/satır)
    /// reddedilmemeli — aşırı düzeltme kontrolü.
    #[test]
    fn test_validate_page_header_accepts_full_width_a4() {
        let mut header = valid_header();
        header.width = 4960;
        header.bytes_per_line = 620;
        assert!(validate_page_header(&header).is_ok());
    }

    /// Yuvarlama payı: 1 baytlık aşım hoş görülür, 2 bayt reddedilir.
    #[test]
    fn test_validate_page_header_line_width_slack_is_one_byte() {
        // 595 pt @ 600 DPI => 4960 px => 620 bayt bant genişliği.
        let mut ok = valid_header();
        ok.width = 4968; // 621 bayt
        ok.bytes_per_line = 621;
        assert!(validate_page_header(&ok).is_ok(), "1 baytlık pay kabul edilmeli");

        let mut too_wide = valid_header();
        too_wide.width = 4976; // 622 bayt
        too_wide.bytes_per_line = 622;
        assert!(validate_page_header(&too_wide).is_err(), "2 bayt aşım reddedilmeli");
    }

    /// D-02: `cupsHeight` sayfanın fiziksel yüksekliğine sığmalı — D-01'in
    /// dikey karşılığı. Yamadan önce bu başlık kabul ediliyordu.
    #[test]
    fn test_validate_page_header_rejects_page_taller_than_paper() {
        let mut header = valid_header();
        header.page_size_points = [595, 1]; // 1 pt yüksek "sayfa"
        header.height = 24_000; // MAX_LINES içinde, ama sayfaya sığmıyor
        let err = validate_page_header(&header).expect_err("uzun sayfa reddedilmeliydi");
        assert!(
            err.to_string().contains("sayfa yüksekliğine sığmıyor"),
            "hata nedeni açıklanmalı: {}",
            err
        );

        // Normal boyutlu bir A4 sayfasında da aşım yakalanmalı: 842 pt @ 600
        // DPI = 7017 satır; gerçek cups-filters çıktısı 6816'dır.
        let mut a4 = valid_header();
        a4.height = 24_000;
        assert!(
            validate_page_header(&a4).is_err(),
            "A4'e sığmayan yükseklik reddedilmeli"
        );
    }

    /// Aşırı düzeltme kontrolü: gerçek `cupsfilter` çıktısının ürettiği
    /// yükseklikler reddedilmemeli. Değerler, ppd/samsung-ml2160.ppd ile
    /// `cupsfilter -m application/vnd.cups-raster` çalıştırılarak ölçüldü;
    /// hepsi fiziksel sınırın altında kalır çünkü `*ImageableArea` kenar
    /// boşlukları (12 pt üst + 12 pt alt) düşülür.
    #[test]
    fn test_validate_page_header_accepts_real_cupsfilter_heights() {
        // (sayfa_yüksekliği_pt, y_dpi, ölçülen cupsHeight)
        let measured = [
            (842u32, 300u32, 3408u32), // A4
            (842, 600, 6816),
            (842, 1200, 13632),
            (792, 600, 6400), // Letter
            (1008, 600, 8200), // Legal
            (1008, 1200, 16400),
            (420, 600, 3296), // A6
            (936, 1200, 15200), // Folio
        ];
        for (page_pt, ydpi, cups_height) in measured {
            let mut h = valid_header();
            h.page_size_points = [595, page_pt];
            // D-03 gereği eksenler eşit; ölçümler zaten simetrik
            // çözünürlüklerden alındı.
            h.hw_resolution = [ydpi, ydpi];
            h.height = cups_height;
            assert!(
                validate_page_header(&h).is_ok(),
                "gerçek cupsfilter çıktısı reddedildi: {} pt @ {} DPI => {} satır",
                page_pt,
                ydpi,
                cups_height
            );
        }
    }

    /// Yuvarlama payı: 8 satırlık aşım hoş görülür, fazlası reddedilir.
    #[test]
    fn test_validate_page_header_height_slack_is_eight_lines() {
        // 842 pt @ 600 DPI => ceil(842 * 600 / 72) = 7017 satır.
        let exact = compute_page_height_lines(842, 600);
        assert_eq!(exact, 7017);

        let mut ok = valid_header();
        ok.height = exact + 8;
        assert!(validate_page_header(&ok).is_ok(), "8 satırlık pay kabul edilmeli");

        let mut too_tall = valid_header();
        too_tall.height = exact + 9;
        assert!(validate_page_header(&too_tall).is_err(), "9 satır aşım reddedilmeli");
    }

    /// Yükseklik sınırı dikey çözünürlüğe bağlı olmalı: `compute_page_height_lines`
    /// `hw_resolution[1]`'i alır, `[0]`'ı değil. D-03 asimetrik akışları zaten
    /// reddettiği için bu, yardımcının kendisi üzerinden doğrulanıyor.
    #[test]
    fn test_page_height_lines_uses_vertical_resolution() {
        assert_eq!(compute_page_height_lines(842, 600), 7017);
        assert_eq!(compute_page_height_lines(842, 300), 3509);
        assert_eq!(compute_page_height_lines(842, 1200), 14034);

        // Sınır gerçekten çözünürlükle ölçekleniyor: 600 DPI'da geçerli olan
        // bir yükseklik, aynı kağıtta 300 DPI'da reddedilmeli.
        let mut h300 = valid_header();
        h300.hw_resolution = [300, 300];
        h300.width = 2480;
        h300.bytes_per_line = 310;
        h300.height = 6816; // 600 DPI'nın satır sayısı
        assert!(
            validate_page_header(&h300).is_err(),
            "300 DPI'da 600 DPI'nın satır sayısı kabul edilmemeli"
        );

        h300.height = 3408; // 300 DPI için gerçek cupsfilter değeri
        assert!(validate_page_header(&h300).is_ok());
    }

    /// Asimetrik çözünürlük (`1200x600dpi`) DESTEKLENEN gerçek bir QPDL
    /// modudur — SpliX de aynı seçeneği ml2010/ml2015/ml1640/ml2510/ml2525
    /// PPD'lerinde sunar — ve reddedilmemelidir.
    #[test]
    fn test_validate_page_header_accepts_asymmetric_resolution() {
        // A4 @ 1200x600: cupsfilter'ın gerçekte ürettiği değerler.
        let mut h = valid_header();
        h.hw_resolution = [1200, 600];
        h.width = 9522;
        h.bytes_per_line = 1191;
        h.height = 6816;
        assert!(
            validate_page_header(&h).is_ok(),
            "1200x600dpi gerçek bir QPDL modu, reddedilmemeli: {:?}",
            validate_page_header(&h).err()
        );
    }

    /// PPD'nin sunduğu her çözünürlük, filtre tarafından da kabul edilmeli:
    /// aksi hâlde kullanıcı sebebi belirsiz bir "filter failed" görür.
    #[test]
    fn test_filter_accepts_every_ppd_resolution() {
        let ppd = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ppd/samsung-ml2160.ppd"
        ))
        .expect("PPD okunamadı");

        let mut checked = 0;
        for line in ppd.lines() {
            // *Resolution 1200x600dpi/...  ->  HWResolution[1200 600]
            let Some(rest) = line.strip_prefix("*Resolution ") else {
                continue;
            };
            let name = rest.split('/').next().unwrap_or("").trim_end_matches("dpi");
            let (x, y) = match name.split_once('x') {
                Some((a, b)) => (a.parse::<u32>().unwrap(), b.parse::<u32>().unwrap()),
                None => {
                    let v = name.parse::<u32>().unwrap();
                    (v, v)
                }
            };

            // O çözünürlükte A4 için tutarlı bir başlık kur.
            let mut h = valid_header();
            h.hw_resolution = [x, y];
            h.width = (595 * x).div_ceil(72);
            h.bytes_per_line = h.width.div_ceil(8);
            h.width = h.bytes_per_line * 8;
            h.height = (842 * y).div_ceil(72);
            assert!(
                validate_page_header(&h).is_ok(),
                "PPD {}x{} DPI sunuyor ama filtre reddediyor: {:?}",
                x,
                y,
                validate_page_header(&h).err()
            );
            checked += 1;
        }
        assert!(checked >= 4, "PPD'den çözünürlük okunamadı: {}", checked);
    }

    /// D-04 regresyonu: `cupsColorOrder` artık denetleniyor.
    #[test]
    fn test_validate_page_header_checks_color_order() {
        // 1-bit tek kanallı veride üç dizilim de eşdeğerdir, üçü de kabul.
        for order in [
            CupsColorOrder::Chunked,
            CupsColorOrder::Banded,
            CupsColorOrder::Planar,
        ] {
            let mut header = valid_header();
            header.color_order = order;
            assert!(validate_page_header(&header).is_ok(), "{:?} kabul edilmeliydi", order);
        }

        let mut unknown = valid_header();
        unknown.color_order = CupsColorOrder::Unknown(99);
        assert!(validate_page_header(&unknown).is_err(), "tanınmayan dizilim reddedilmeli");
    }

    /// Belirtilen sayıda küçük, geçerli sayfadan oluşan bir V3 akışı üretir.
    fn v3_multipage_stream(pages: u32) -> Vec<u8> {
        let mut page = vec![0u8; 1796];
        {
            let mut put = |off: usize, val: u32| {
                page[off..off + 4].copy_from_slice(&val.to_be_bytes());
            };
            // Mümkün olan en küçük geçerli sayfa: 1 pt @ 72 DPI => 8 px => 1
            // baytlık bant. Sayfa başına bant tamponu 128 bayta iner, böylece
            // 5.000 sayfalık sınır testi saniyeler değil milisaniyeler sürer.
            put(276, 72); // hw_resolution
            put(280, 72);
            put(352, 1); // page_size_points
            put(356, 1);
            put(372, 8); // width
            put(376, 1); // height
            put(384, 1); // bits_per_color
            put(388, 1); // bits_per_pixel
            put(392, 1); // bytes_per_line
            put(400, 3); // color_space = K
        }
        let mut stream = b"RaS3".to_vec();
        for _ in 0..pages {
            stream.extend_from_slice(&page);
            stream.push(0u8); // 1 satır x 1 bayt
        }
        stream
    }

    /// D-04 regresyonu: sayfa sayısının bir üst sınırı olmalı.
    #[test]
    fn test_page_count_is_capped() {
        let mut out: Vec<u8> = Vec::new();
        let err = process_cups_raster_to_spl(
            &no_args(),
            Box::new(Cursor::new(v3_multipage_stream(MAX_PAGES_PER_JOB + 1))),
            &mut out,
        )
        .expect_err("sayfa sınırı aşılınca hata beklenir");
        assert!(err.to_string().contains("sayfa sınırını aştı"), "{}", err);
        // Sınır aşılsa bile iş düzgün kapatılmalı (Y-03 garantisi).
        assert!(out.ends_with(spl::PJL_END));
    }

    /// Sınırın tam üstündeki bir iş sorunsuz işlenmeli.
    #[test]
    fn test_page_count_limit_is_not_off_by_one() {
        let mut out: Vec<u8> = Vec::new();
        process_cups_raster_to_spl(
            &no_args(),
            Box::new(Cursor::new(v3_multipage_stream(MAX_PAGES_PER_JOB))),
            &mut out,
        )
        .expect("tam sınır kadar sayfa kabul edilmeli");
        assert!(out.ends_with(spl::PJL_END));
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
