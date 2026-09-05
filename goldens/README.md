# Golden Files

This directory freezes **the SPL2/QPDL bytes the 1.x CUPS filter produces**
before the move to a PAPPL Printer Application. The acceptance criterion is
byte-for-byte identity (project rule 6), and that criterion needs a reference
to compare against.

The code that produces and compares them: [`../src/golden.rs`](../src/golden.rs).

## Files

| File | Content |
|---|---|
| `<case>.spl` | The raw SPL2/QPDL stream the filter produced |
| `<case>.json` | The classic CUPS page header that produced that stream, plus the QPDL placement values the filter derived from it |
| `SHA256SUMS` | Checksums of every file above, so a validated corpus has a fixed reference |

The raster **inputs** are not kept in the repository; `build_raster` in
`src/golden.rs` generates them deterministically (the A4 @1200 DPI input alone
is ~16 MB).

## Usage

```sh
# Compare (the default; on every `cargo test` run)
cargo test golden

# Refresh AFTER a deliberate behaviour change
UPDATE_GOLDENS=1 cargo test golden

# Verify the committed bytes
sha256sum -c SHA256SUMS

# Export the inputs (for manual inspection / comparison with the installed binary)
DUMP_GOLDEN_RASTER=/tmp/r cargo test golden
```

Every diff in a `.spl` file means **the bytes going to the printer changed**.
Refresh only together with a deliberate change, and include the diff in review.

## Corpus coverage

* Registration-mark (`*-marks`) cases for A4 and Letter at **every supported
  resolution** (300x300, 600x600, 1200x600, 1200x1200).
* `a4-300-marks` / `letter-300-marks` cover the 64-line band rule; everything
  else produces 128-line bands.
* `a5-600-marks`, `envc5-600-marks` cover small media and envelope geometry.
* `a4-600-marks-3copies` covers the copy field, `a4-600-marks-3pages` covers
  multiple pages, `envc5-600-marks-manual-env` covers the tray + media-type
  mapping.
* `a4-600-marks-v2rle` uses a line-RLE (`RaS2`) input; it must come out
  byte-for-byte identical to `a4-600-marks`.
* `a4-600-blank` is a completely blank page.

**Known gap:** because every medium in this PPD has a 12 pt margin, the
`dst_offset > 0` branch of `band_placement` is never exercised by the corpus
(`dst_offset` is 0 in every case). Until a geometry whose hard margin is
smaller than the centring offset is added, that branch is not protected by any
golden file.
