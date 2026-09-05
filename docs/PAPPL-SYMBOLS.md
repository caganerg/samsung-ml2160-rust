# PAPPL Symbol Table — nothing bound is newer than 1.3

Decision Q-1 targets Debian trixie's PAPPL **1.3.1-2.1+b2** and requires a
table asserting that no symbol `pappl-sys` binds is newer than 1.3. This is
that table, plus the argument behind it.

## Why the table looks like this

The obvious form would be a column giving the PAPPL version that introduced
each symbol. That column cannot be filled honestly from what is installed
here: the 1.3.1 headers contain exactly **one** `@since` annotation in total —
`PAPPL_SOPTIONS_NO_TLS`, `@since PAPPL 1.1@` in `system.h` — so an
"introduced in" column would be invented for the other 44 entries, which
project rule 1 forbids in spirit and rule 2 in letter.

What can be established mechanically is stronger for the purpose the
requirement serves. The requirement exists so the binary never references
something 1.3 lacks. Two checks together guarantee exactly that:

1. **`tests/symbols.rs` asserts that every declared symbol is exported by the
   installed `libpappl.so.1`**, which *is* 1.3.1. A symbol introduced after
   1.3 would not be there, and the test would fail. This is a direct
   observation of the target library, not a claim about release history.
2. **`build.rs` refuses to build outside `>= 1.3, < 2.0`**, so the library the
   test observes is always in range.

Every declaration is additionally transcribed from a header shipped by
`libpappl-dev` 1.3.1-2.1+b2 — a symbol newer than 1.3 could not have been
copied from them in the first place.

If a 1.4-only symbol ever turns out to be genuinely necessary, decision Q-1
says stop and ask rather than raising the floor.

## The table

Verified on 2026-09-05 against `libpappl-dev` / `libpappl1t64` 1.3.1-2.1+b2,
`pkg-config --modversion pappl` = 1.3.1. All 45 symbols are exported by the
installed library; `cargo test -p pappl-sys` re-checks this on every run.

| Symbol | Declared in | Exported by installed 1.3.1 |
|---|---|---|
| `papplMainloop` | `mainloop.h` | yes |
| `papplMainloopShutdown` | `mainloop.h` | yes |
| `papplSystemCreate` | `system.h` | yes |
| `papplSystemDelete` | `system.h` | yes |
| `papplSystemRun` | `system.h` | yes |
| `papplSystemShutdown` | `system.h` | yes |
| `papplSystemIsRunning` | `system.h` | yes |
| `papplSystemAddListeners` | `system.h` | yes |
| `papplSystemSetPrinterDrivers` | `system.h` | yes |
| `papplSystemSetLogLevel` | `system.h` | yes |
| `papplSystemGetLogLevel` | `system.h` | yes |
| `papplSystemLoadState` | `system.h` | yes |
| `papplSystemSaveState` | `system.h` | yes |
| `papplPrinterCreate` | `printer.h` | yes |
| `papplPrinterDelete` | `printer.h` | yes |
| `papplPrinterSetDriverData` | `printer.h` | yes |
| `papplPrinterGetDriverData` | `printer.h` | yes |
| `papplPrinterSetReadyMedia` | `printer.h` | yes |
| `papplPrinterGetName` | `printer.h` | yes |
| `papplPrinterGetID` | `printer.h` | yes |
| `papplPrinterOpenDevice` | `printer.h` | yes |
| `papplPrinterCloseDevice` | `printer.h` | yes |
| `papplPrinterGetReasons` | `printer.h` | yes |
| `papplPrinterSetReasons` | `printer.h` | yes |
| `papplJobGetName` | `job.h` | yes |
| `papplJobGetUsername` | `job.h` | yes |
| `papplJobGetID` | `job.h` | yes |
| `papplJobGetFilename` | `job.h` | yes |
| `papplJobGetFormat` | `job.h` | yes |
| `papplJobGetImpressions` | `job.h` | yes |
| `papplJobSetImpressionsCompleted` | `job.h` | yes |
| `papplJobIsCanceled` | `job.h` | yes |
| `papplJobGetData` | `job.h` | yes |
| `papplJobSetData` | `job.h` | yes |
| `papplJobSetReasons` | `job.h` | yes |
| `papplJobGetPrinter` | `job.h` | yes |
| `papplDeviceWrite` | `device.h` | yes |
| `papplDevicePuts` | `device.h` | yes |
| `papplDeviceFlush` | `device.h` | yes |
| `papplDeviceRead` | `device.h` | yes |
| `papplDeviceGetStatus` | `device.h` | yes |
| `papplLog` | `log.h` | yes |
| `papplLogJob` | `log.h` | yes |
| `papplLogPrinter` | `log.h` | yes |
| `papplCopyString` | `base.h` | yes |

## Types and constants

The same discipline covers data, where the risk is worse: a wrong field offset
corrupts memory silently instead of failing to link. `probe/layout_probe.c`
prints the size and alignment of all 8 types this crate declares, the offset
and size of all 128 fields, and the value of all 69 constants and limits;
`tests/layout.rs` checks every one of those 205 records against the Rust
declarations and fails if any record is left unchecked.

The `cups_page_header2_t` embedded in `pappl_pr_options_t` is held as opaque
storage of the probed size (1796 bytes) for now. Its individual raster fields
are a CUPS header rather than a PAPPL one and get the same treatment —
transcription plus probe entries — when the raster callbacks need them.
