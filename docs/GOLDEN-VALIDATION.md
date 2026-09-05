# Golden Corpus Validation — 2026-09-05

The 15-case golden corpus in `goldens/` is the reference every later step of
the PAPPL migration compares against: ground rule 6 makes byte-for-byte
identity the acceptance criterion, so if these bytes are wrong, every "no
change in behaviour" claim built on them is wrong too. This document records
the validation the corpus was put through before any PAPPL code was written,
so the result does not have to be taken on trust.

## 1. Reproduction from the untouched pre-migration tree

**Result: all 15 cases reproduce exactly.**

The literal check — "regenerate the goldens from `v1.x-final`" — is not
possible, because the generator does not exist at that tag: `v1.x-final`
(`33d4ff2`) predates both `4a5552a` (which made the PJL service date
injectable) and `1cf2d0b` (which added `src/golden.rs`), and
`process_cups_raster_to_spl` there has no `service_date` parameter. The
equivalent check that *is* possible drives the untouched binary instead of the
untouched generator:

```sh
git worktree add /tmp/v1x v1.x-final && (cd /tmp/v1x && cargo build --release)
DUMP_GOLDEN_RASTER=/tmp/rasters cargo test --release golden
for r in /tmp/rasters/*.raster; do
    /tmp/v1x/target/release/rastertospl-rust 1 tester golden 1 '' "$r" > out.spl
done
```

Every one of the 15 streams produced by the `v1.x-final` binary is identical in
length to its golden and differs in exactly **two bytes** — the day and the
last digit of `@PJL DEFAULT SERVICEDATE=20260101`, which the corpus pins and
the 1.x binary reads from the clock. With that line normalised, the streams are
byte-for-byte identical. The raster inputs are generated deterministically by
`build_raster` in `src/golden.rs`; the filter's argv `num_copies` is
deliberately ignored (`src/main.rs:22`), so copies come from the raster header
in both paths.

The corpus therefore does **not** depend on the one encoder-adjacent change
that was made before it was captured: `4a5552a` injects the service date but
does not alter any other byte.

`goldens/SHA256SUMS` pins the validated bytes so this check has a fixed
reference next time; verify with `cd goldens && sha256sum -c SHA256SUMS`.

## 2. Proof that the harness can fail

A harness that never goes red is not a safety net. Each of the five migration
risks in `docs/MIGRATION-PLAN.md` §9 was injected as a deliberate minimal
defect, one at a time, into a scratch worktree, and the full suite was run:

| Risk | Injected defect | Suite | Caught by `golden::test_goldens_match` | Other tests that caught it |
|---|---|---|---|---|
| R-1 | `hard_margin_bytes` returns one byte too many | RED | yes | 5 |
| R-2 | band polarity inversion removed | RED | yes | 3 |
| R-3 | band height forced to 128 at 300 dpi | RED | yes | 3 |
| R-4 | resolution axes swapped in the 17-byte page header | RED | yes | 1 |
| R-5 | two job-level PJL lines reordered | RED | yes | **0** |

All five are caught, and the golden corpus catches all five on its own. R-5 is
caught by **nothing else**: reordering `@PJL SET ALTITUDE=LOW` and `@PJL SET
DENSITY=3` leaves all 106 other tests green. That is the case for keeping the
corpus in CI rather than relying on unit tests.

## 3. Coverage, and what is not covered

Covered by the 15 cases:

| Axis | Covered |
|---|---|
| Media | A4, Letter, A5, EnvC5 |
| Resolution | 300×300, 600×600, 1200×600, 1200×1200 (A4 and Letter at all four) |
| Band height | 64 (the two 300 dpi cases) and 128 (all others) |
| Input slot | unset (→ Auto) and `MediaPosition` 2 (Manual) |
| Media type | unset and `ENV` |
| Copies | 1 and 3 |
| Pages | 1 and 3 |
| Input encoding | `RaS3` uncompressed and `RaS2` line-RLE (identical output asserted) |
| Content | four 1-pixel corner registration marks, and one blank page |

Not covered, in rough order of how much it matters:

* **`band_placement`'s `dst_offset > 0` branch.** Every PPD medium has a 12 pt
  margin, so `dst_offset` is 0 in all 15 cases and only `src_skip` varies (0 at
  1200 dpi, 1 at 300/600 dpi). The positive-offset branch — a hard margin
  smaller than the centring offset — is exercised by unit tests but frozen by
  no golden.
* **The other seven PPD media**: A6, B5, Env10, EnvDL, Executive, Folio, Legal.
  Their `SplPaperSize` codes are covered by unit tests, not by golden bytes.
* **Thirteen of the fifteen PPD media types** (`NORMAL`, `THICK`, `THIN`,
  `BOND`, `CARD`, `COLOR`, `COTTON`, `LABEL`, `OHP`, `RECYCLED`, `ARCHIVE`,
  `USED`, `OFF`) and the explicit `MediaPosition` 1 (Auto).
* **Duplex page-header bytes.** All cases are Simplex, so the `0xB`/`0xC`
  duplex and tumble bytes are frozen in one state only. Duplex is a non-goal
  for 2.0 (`docs/NON-GOALS.md`), so this is a deliberate gap.
* **Combinations**: multi-page × multi-copy together, and the
  `sanitize_copies` clamp boundary.
* **Compressor stress**: only corner marks and blank pages, so Algo 0x11's
  literal/repeat paths run on very sparse data. The tallest case (A4 @1200 dpi)
  is 107 bands, so the `u8` band-index ceiling at 256 bands is not approached.
* **Untrusted job metadata**: job name and user are fixed ASCII, so
  `quote_untrusted` is not exercised through this path.

## 4. Physical measurement — NOT DONE

**R-1 remains unverified.** No printer is attached to this machine (`lpstat -p`
reports no destinations; no Samsung device on USB), so no registration-mark
golden has been printed and measured against the PPD's `*ImageableArea`.

This matters because the corpus can be self-consistently wrong: every case
derives its margin from the same 12 pt `*ImageableArea` value, so byte-for-byte
agreement proves internal consistency and nothing about where toner lands on
paper. Until one `*-marks` case is printed through the 1.x path and measured
with a ruler, **R-1 is validated only against itself**.
