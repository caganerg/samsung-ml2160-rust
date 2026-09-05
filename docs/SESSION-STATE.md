# Session State — Handover Note

*Last updated 2026-09-05 (second session).*

**Raise the hard-margin question first: it is open, it blocks the
driver-capability table, and only the maintainer's hardware settles it.**
Upstream SpliX gives the ML-2160 `HWMargins 10.75 15 10.75 15` and the ML-2165
`12.5 12.5 12.5 12.5`, while this driver's PPD uses 12 pt on every edge — a
value that traces to a defs file for different hardware and matches neither
model. At 600 dpi those three values give 12, 14 and 13 hard-margin bytes
respectively, so the difference is a whole byte column, about 0.34 mm, on a
page that would look correct. Nothing has been changed:
[`docs/MARGINS.md`](MARGINS.md) has the full trace, the byte table at every
resolution and the three candidate resolutions, `docs/DECISIONS.md` carries it
as an open Q-2 follow-up, and release gate G-1 now requires the measurement to
name the exact model, per model rather than per family.

Plan steps P1 to P4 are complete and pushed on `migration/pappl`; P5 is next
and was deliberately not started. P2's golden corpus is validated rather than
merely built — 32 cases, reproduced byte-for-byte from the untouched
`v1.x-final` binary, with each of the five encoder risks R-1 to R-5 injected as a
defect to prove the suite goes red, and R-5 caught by the corpus alone. (R-6,
the libcups ABI risk, is not of that kind: it cannot be mutation-tested, and
its mitigations are build-time, packaging and a runtime backstop.) P3 is
`crates/pappl-sys`: hand-written FFI whose 8 types, 128 fields, 69 constants
and 45 symbols are all checked against the installed headers and library by a
C probe, with the check failing if any probed record is left unchecked. P4 is
`crates/pappl`: the `guard` shim every `extern "C"` callback body passes
through so no panic can unwind into C, borrowed `Device`/`Job` handles, and the
`io::Write` implementation that lets the unchanged SPL2 encoder write to a
PAPPL device. P5 covers the driver-capability table, the mainloop and the
printable-area experiment, and carries two requirements written down where they
will be seen: the hard-margin table cannot be finalised until the measurement
above, and R-6/H requires the option-struct sanity check to **fail the job, not
clamp**, because a clamp would delete the only runtime signal that the libcups
ABI moved. Also outstanding, none of it blocking: SPDX headers on the GPL
files and the PPD (Q-8b), and the Q-4 toner-save evidence that turned up in the
SpliX source and is recorded in `docs/NON-GOALS.md`.

## Step numbering

Two numbering schemes have been in use: the plan's own, and the one in the
prompt series the work is driven from. **The plan's numbering is authoritative
from here on**, in the documents and in every report. The mapping below covers
the steps where the two are known to differ or to coincide; blanks are steps
the prompt series has not named, and are left blank rather than guessed.

| Plan | What it covers | Prompt series |
|---|---|---|
| P1 | Repository audit and migration plan (`docs/MIGRATION-PLAN.md`) | — |
| P2 | Golden-file harness, and its validation | P2 |
| P3 | `pappl-sys`: hand-written FFI **and** the size/offset/enum layout harness | P3 + P6 |
| P4 | `pappl`: the safe wrapper and the `catch_unwind` callback shim | P7 |
| P5 | Minimal PAPPL app; the printable-area vs full-media experiment (`docs/MARGINS.md`) | — |
| P9 | Raster-type decision, and the dithering-exposure question | — |
| P11 | The gate after which the frozen 1.x filter may be removed (Q-5) | — |
| P12 | Hardware bring-up; release gate G-1, the physical margin measurement | P12 |

The prompt series splits P3 into the bindings (its P3) and their layout tests
(its P6); the plan keeps them in one step because the harness had to exist
before the first declaration. Where older documents in this repository say
P11 or P12, they mean the rows above.

**Reading order for a fresh agent:** this file, then
[`docs/MARGINS.md`](MARGINS.md) and `docs/DECISIONS.md`, then
`docs/GOLDEN-VALIDATION.md` and `CONTRIBUTING.md` for how output is verified
and what blessing a golden requires, then `docs/MIGRATION-PLAN.md` §7 and §9
for the target layout and the six corruption risks. The byte-for-byte
behaviour itself lives in `src/main.rs` around `compute_page_width_pixels`,
`hard_margin_bytes` and `band_placement`, and in `src/spl.rs` around
`begin_job`, `begin_page`, `write_compressed_band`, `end_page` and `end_job`.
The `v1.x-final` recovery anchor is an annotated tag: object `7c0cf2c`,
commit `33d4ff2`, both present on `origin`.
