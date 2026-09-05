# Session State — Handover Note

*Last updated 2026-09-05 (second session).*

**Where things stand.** The migration plan, the decision log and the non-goals
are complete and all eleven questions are answered — Q-8a, the last open one,
was decided this session: `pappl-sys` and `pappl` are `Apache-2.0 OR MIT`,
everything else `GPL-2.0-only`. `libpappl-dev` **is** installed
(1.3.1-2.1+b2), so the previous note's claim that it is missing is out of
date. Branch `migration/pappl` is pushed to `origin`, and so is the
`v1.x-final` tag, which is the recovery anchor for the working 1.x driver. It
is an **annotated** tag: the tag object is `7c0cf2c` and it points at commit
`33d4ff2`. `git ls-remote --tags origin` shows both, so the remote agrees.

**P2 is validated, not merely done.** The golden corpus was audited rather
than trusted: all 15 original streams are reproduced byte-for-byte by the
untouched `v1.x-final` binary (only the two pinned `SERVICEDATE` digits
differ), and each of the five migration risks was injected as a one-line
defect to confirm the suite goes red. R-5 — a reordered job-level PJL line —
is caught by the golden corpus and by nothing else in the repository. The full
record is `docs/GOLDEN-VALIDATION.md`, and `CONTRIBUTING.md` states the bless
discipline that follows from it.

**The corpus is now 32 cases**, closing the gaps that could only be captured
while the 1.x encoder runs: `dst_offset > 0` (two synthetic cases, the branch
that sits directly on R-1), all eleven PPD media sizes, Legal and Folio at
every resolution, multi-page combined with multi-copy and the copy clamp, and
a non-ASCII job title. `goldens/SHA256SUMS` pins the validated bytes. The
band-order ceiling is proved across the whole matrix (worst case Legal
@1200x1200, 129 of 256 bands) with a compile-time assertion bounding it from
the validator's limits.

**P3 has started.** `crates/pappl-sys` holds hand-written FFI for PAPPL 1.3:
8 types, 128 fields, 69 constants and 45 functions, every declaration quoting
the real prototype. The layout harness came first and is the thing to keep
green — `probe/layout_probe.c` plus `tests/layout.rs` check every field
offset, and `tests/symbols.rs` checks every symbol against the installed
library. `docs/PAPPL-SYMBOLS.md` carries the Q-1 symbol table.

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

**What to do next.** Continue P3 into P4: the `pappl` safe wrapper, whose
entire reason for existing is to own the `unsafe` surface and the
`catch_unwind` shim every callback must pass through (rule 5). Two things are
deliberately left for when they are needed: the fields of
`cups_page_header2_t` (bound as opaque storage today) and `cups_option_t`
(opaque pointer) — both are CUPS headers and get the same transcription plus
probe treatment. Also outstanding: SPDX headers and `license` fields for the
FFI crates as they are created, the GPL-2 header on the PPD (Q-8b), and the
exact SpliX copyright notice in `debian/copyright`, which needs a copy of the
upstream source to transcribe.

**Release gate G-1 is open and blocks 2.0.** R-1 — the hard margin — is
validated only against itself until a registration-mark page is printed on
real hardware and measured against the PPD's `*ImageableArea`. No printer is
attached to this machine. The gate lands in P12; see
`docs/GOLDEN-VALIDATION.md` §4.

**Reading order for a fresh agent:** this file, then `docs/DECISIONS.md`,
`docs/GOLDEN-VALIDATION.md` and `CONTRIBUTING.md`, then
`docs/MIGRATION-PLAN.md` §7 and §9 for the target layout and the corruption
risks, then `goldens/README.md` and `src/golden.rs`. The byte-for-byte
behaviour itself lives in `src/main.rs` around `compute_page_width_pixels`,
`hard_margin_bytes` and `band_placement`, and in `src/spl.rs` around
`begin_job`, `begin_page`, `write_compressed_band`, `end_page` and `end_job`.
