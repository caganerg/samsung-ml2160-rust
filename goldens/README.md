# Altın Dosyalar (Golden Files)

Bu dizin, **1.x CUPS filtresinin ürettiği SPL2/QPDL baytlarını** PAPPL
Printer Application'a geçişten önce dondurur. Kabul ölçütü bayt bayt
aynılıktır (proje kuralı 6) ve bu ölçütün karşılaştırılacak bir referansa
ihtiyacı var.

Üreten ve karşılaştıran kod: [`../src/golden.rs`](../src/golden.rs).

## Dosyalar

| Dosya | İçerik |
|---|---|
| `<vaka>.spl` | Filtrenin ürettiği ham SPL2/QPDL akışı |
| `<vaka>.json` | O akışı üreten klasik CUPS sayfa başlığı + filtrenin ondan türettiği QPDL yerleşim değerleri |

Raster **girdileri** depoda tutulmaz; `src/golden.rs` içindeki `build_raster`
onları deterministik olarak üretir (A4 @1200 DPI girdisi tek başına ~16 MB).

## Kullanım

```sh
# Karşılaştır (varsayılan; her `cargo test` çalıştırmasında)
cargo test golden

# Kasıtlı bir davranış değişikliğinden SONRA yenile
UPDATE_GOLDENS=1 cargo test golden

# Girdileri dışa aktar (elle inceleme / kurulu ikiliyle karşılaştırma için)
DUMP_GOLDEN_RASTER=/tmp/r cargo test golden
```

`.spl` dosyasındaki her diff, **yazıcıya giden baytların değiştiği** anlamına
gelir. Yenileme yalnızca kasıtlı bir değişiklikle birlikte ve diff incelemeye
dahil edilerek yapılmalıdır.

## Korpus kapsamı

* Hizalama işareti (`*-marks`) vakaları A4 ve Letter için **desteklenen her
  çözünürlükte** (300x300, 600x600, 1200x600, 1200x1200).
* `a4-300-marks` / `letter-300-marks` 64 satırlık bant kuralını kapsar;
  diğerleri 128 satırlık bant üretir.
* `a5-600-marks`, `envc5-600-marks` küçük medya ve zarf geometrisi.
* `a4-600-marks-3copies` kopya alanı, `a4-600-marks-3pages` çok sayfa,
  `envc5-600-marks-manual-env` tepsi + kağıt türü eşlemesi.
* `a4-600-marks-v2rle` satır-RLE'li (`RaS2`) girdi; `a4-600-marks` ile bayt
  bayt aynı olmalıdır.
* `a4-600-blank` tamamen boş sayfa.

**Bilinen boşluk:** bu PPD'nin her medyada 12 pt kenar boşluğu olduğu için
`band_placement`'ın `dst_offset > 0` dalı korpusta hiç tetiklenmiyor
(`dst_offset` her vakada 0). Sert kenar boşluğu ortalamadan küçük olan bir
geometri eklenmedikçe o dal altın dosyalarla korunmuyor.
