# Session State — Handover Note

*Last updated 2026-09-05.*

**Where things stand.** The repository has been audited end to end and the
migration from the 1.x CUPS raster filter to a PAPPL Printer Application is
planned but not started — no PAPPL code exists yet, and `libpappl-dev` is not
even installed on the development machine (install
`pkg-config libpappl-dev`; trixie has 1.3.1-2.1+b2, which is the agreed
target). Three things are done. First, the audit itself:
[`docs/MIGRATION-PLAN.md`](MIGRATION-PLAN.md) covers the current module layout,
the SPL2 engine surface, hidden state, every printer option and where it is
read, the proposed `spl2-core` / `pappl-sys` / `pappl` / `ml216x-printer-app`
workspace split, licensing, and the top five ways this migration could silently
corrupt output. Second, all eleven open questions have been answered and
recorded in [`docs/DECISIONS.md`](DECISIONS.md), with the two exclusions
written up in [`docs/NON-GOALS.md`](NON-GOALS.md); only **Q-8a**, the licence
for the safe `pappl` wrapper, is still open, and it is blocked on a decision
rather than on work — the SPDX audit it was waiting for is already in the
decision log and recommends MIT. Third, and most important for what comes next,
**P2 (the golden-file harness) is already complete** on branch
`p2-golden-harness`: `src/golden.rs` plus 15 cases in `goldens/`, each with a
`.spl` stream and a `.json` sidecar of the classic CUPS page header, including
registration-mark cases on A4 and Letter at every supported resolution. It was
verified by running the installed release binary over all 15 generated rasters;
every one reproduces its golden exactly apart from the two pinned
`SERVICEDATE` digits. 107 tests pass, `clippy --all-targets -D warnings` and
`fmt --check` are clean. The tree before any of this is tagged `v1.x-final`.

**What to do next, and what to read first.** The next task is **not** P2 — it
is done and awaiting review; do not rebuild it. The immediate work is P3, the
`pappl-sys` FFI bindings, which is blocked until `libpappl-dev` is installed
and cannot begin before the headers are read (project rule 2 forbids inventing
PAPPL signatures, so quote the real declaration from `/usr/include/pappl/*.h`
for every symbol bound, and produce a table asserting no bound symbol is newer
than 1.3). Two smaller items should be cleared first because they are cheap and
already agreed: `src/golden.rs` and `goldens/README.md` were written in Turkish
before the English-everywhere decision (Q-11) and need converting, and
`ppd/samsung-ml2160.ppd` needs its GPL-2 header (Q-8b). A fresh agent should
read, in order: this file, then `docs/DECISIONS.md` for what was settled and
why, then `docs/MIGRATION-PLAN.md` sections 7 and 9 for the target layout and
the corruption risks, then `goldens/README.md` and `src/golden.rs` to
understand how output is verified, and finally `src/main.rs` around lines
367–460 (`compute_page_width_pixels`, `hard_margin_bytes`, `band_placement`)
and `src/spl.rs` around lines 820–1080 (`begin_job`, `begin_page`,
`write_compressed_band`, `end_page`, `end_job`) — that is where the
byte-for-byte behaviour actually lives. The three deferred investigations, none
blocking, are listed at the end of `docs/MIGRATION-PLAN.md`: the
printable-area-vs-full-media margin experiment for `docs/MARGINS.md`, the
dithering-exposure question for `docs/SECURITY-REVIEW.md`, and confirming the
unpatched overflow lines in PAPPL 1.3.1's source before filing a Debian bug.
