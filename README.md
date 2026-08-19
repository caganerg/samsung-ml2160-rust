# samsung-ml2160-rust

Samsung ML-2160 serisi monokrom lazer yazıcılar için Rust ile yazılmış bir CUPS raster filtresi (`rastertospl-rust`). CUPS'un ürettiği standart raster akışını (`RaSt`/`RaS2`/`RaS3`), yazıcının anladığı ikili **SPL2 / QPDL v3** formatına dönüştürür: PJL iş zarfı, 17 baytlık sayfa başlığı, Algo 0x11 RLE ile sıkıştırılmış şerit (band) kayıtları ve sağlama toplamları.

Protokol detayları [OpenPrinting SpliX](https://github.com/OpenPrinting/splix) sürücüsünün gerçek kaynak koduyla (`document.cpp`, `compress.cpp`, `qpdl.cpp`, `algo0x11.cpp`, `printer.cpp`) karşılaştırılarak doğrulanmış ve gerçek donanımda test edilmiştir.

## Desteklenen Modeller

ML-2160, ML-2165, ML-2165W, ML-2168 (aynı QPDL v3 protokol ailesi).

## Gereksinimler

- Rust toolchain (`cargo`)
- CUPS (`lpadmin`, `lpinfo`, `cupstestppd`)
- Yazıcı USB ile bağlı ve açık

## Kurulum

```sh
./install.sh [kuyruk-adı] [device-uri]
```

Betik sırasıyla: projeyi derler (`cargo build --release`), PPD dosyasını doğrular (`cupstestppd`), filtre ikilisini `/usr/lib/cups/filter/`'a kurar ve bağlı Samsung ML-2160 serisi USB yazıcıyı otomatik bulup bir CUPS kuyruğu (varsayılan adı `ML2160_Rust`) olarak tanımlar. Yalnızca sistem dosyalarına yazan adımlarda `sudo` parolası ister; betiğin tamamını `sudo` ile çalıştırmayın.

USB dışında bir bağlantı (örn. ağ yazıcısı) kullanıyorsanız aygıt URI'sini elle verin:

```sh
./install.sh MyPrinter "ipp://192.168.1.50/ipp/print"
```

Kurulumdan sonra test baskısı:

```sh
lp -d ML2160_Rust dosya.pdf
```

### Manuel Kurulum

`install.sh` kullanmak istemezseniz aynı adımları elle çalıştırabilirsiniz:

```sh
cargo build --release
sudo install -m 755 -o root -g root target/release/rastertospl-rust /usr/lib/cups/filter/rastertospl-rust
sudo lpadmin -p ML2160_Rust -E -v <device-uri> -P ppd/samsung-ml2160.ppd
```

## Test

`test_pipeline.sh`, örnek bir PDF'i (Ghostscript ile üretir), `cupsfilter` ile CUPS raster akışına çevirir, filtreden geçirir ve üretilen SPL2 dosyasının PJL/QPDL kayıt yapısını (sayfa başlığı, band kayıtları, checksum'lar, iş sonu) ayrıştırıp doğrular:

```sh
./test_pipeline.sh                 # örnek PDF otomatik üretilir
./test_pipeline.sh benim.pdf       # kendi PDF'inizle
```

Birim testleri (Algo 0x11 RLE round-trip testi dahil):

```sh
cargo test
```

## Proje Yapısı

- `src/main.rs` — CUPS filtre giriş noktası: argüman ayrıştırma, sayfa/band döngüsü
- `src/raster.rs` — CUPS Raster (V1/V2/V3) başlık ayrıştırıcı
- `src/spl.rs` — SPL2/QPDL protokolü: PJL zarfı, sayfa/band kayıtları, Algo 0x11 RLE
- `ppd/samsung-ml2160.ppd` — CUPS PPD dosyası
- `install.sh` — derleme + sistem kurulumu
- `test_pipeline.sh` — uçtan uca pipeline testi ve SPL2 format doğrulayıcı

## Lisans

GPLv2 (yalnızca v2) — bkz. [LICENSE](LICENSE). Protokol uygulaması GPLv2 lisanslı OpenPrinting SpliX projesinden türetildiği için bu lisansla uyumludur.
