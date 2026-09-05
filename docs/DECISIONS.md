# Decision Log — PAPPL Migration

Decisions taken for the 2.0 line (CUPS filter → PAPPL Printer Application).
Each entry records the question, the decision, and the reasoning, so the state
can be reconstructed from the tree alone.

Questions are numbered as they were raised in `docs/MIGRATION-PLAN.md`.

**On the numbering.** Eleven questions were raised, Q-1 to Q-11, and none was
skipped. There are twelve entries below because Q-8 asked two unrelated things
in one paragraph — the licence for the FFI crates and the missing licence
header on the PPD — and was split into **Q-8a** and **Q-8b** when it was
answered. **Q-7 exists and is decided:** it asked whether the .deb should link
libpappl statically or dynamically, and it is answered under Q-7 below (and
folded into Q-1, which settled the same matter). Counting the decided entries
as ten and treating Q-7 as unaccounted for is the arithmetic slip this note
exists to prevent.

---

## 2026-09-05

### The two blocking answers

These were the decisions that unblocked the migration; everything else follows
from them.

**PAPPL 1.3.1 is the target, with a `>= 1.3, < 2.0` version guard.**
Build against the `libpappl-dev` Debian trixie actually ships (1.3.1-2.1+b2).
Do not build 1.4.x from source, do not vendor libpappl, do not link it
statically. Nothing 1.4 added is needed by a monochrome raster driver — the
raster callbacks, driver data, device API and mainloop have been stable since
1.0 — while vendoring a C library earns the lintian `embedded-library` tag,
removes the package from apt security updates, and makes us the CVE response
path. Linking the archive's library gives `${shlibs:Depends}` for free.

**The hard margin is a driver constant derived from the PPD, with no zero
fallback.** It is never read from a raster header, in either the classic or the
PAPPL path; `Margins[]` arriving as zero is correct behaviour under PAPPL, not
a bug to work around. Per-media margins are derived from
`ppd/samsung-ml2160.ppd`'s `*ImageableArea` and `*PaperDimension`, held in one
committed table that cites the PPD lines it came from, declared in the PAPPL
driver data, and cross-checked at page start against that table. A mismatch, a
zero, or an absent margin **fails the job** with a specific error and a clear
log line. A page that looks fine until measured is worse than a refused job;
see the regression recorded at `src/main.rs:427`.

---

### Q-1 — Which PAPPL version to target
**Decision: Debian trixie's 1.3.1, dynamically linked. Guard `>= 1.3, < 2.0`.**

Reasoning as above. Debian has 1.3.1-2.1 in bookworm, trixie, forky *and* sid,
so there is no newer packaged version to move to; the tracker itself notes that
upstream 1.4.12 is available and unpackaged.

Consequences:
- `pappl-sys` binds only symbols present in the 1.3.1 headers. After bindings
  are generated, produce a table of every bound symbol against the PAPPL
  version that introduced it and assert none is newer than 1.3. If a 1.4-only
  symbol turns out to be genuinely necessary — stop and ask.
- `build.rs` uses pkg-config as the **primary and only** path: `libpappl-dev`
  installs `/usr/lib/x86_64-linux-gnu/pkgconfig/pappl.pc` and ships no
  `pappl-config` script.
- `packaging/debian/control` loses `cups-filters` and gains `libpappl1t64` via
  `${shlibs:Depends}`; the musl static build goes away.

#### Q-1 follow-up — the unpatched dependency
Trixie's 1.3.1 predates upstream's two 2026 overflow fixes
(`4587888f50`, dithering in `pappl/job-process.c`; `44327aaac3`, ready-media in
`pappl/printer-ipp.c`; both 2026-08-04, both two-line bounds clamps). Upstream
released them in 1.4.12 with placeholder CVE IDs (`CVE-2026-NNNNN`), so no CVE
has been published and Debian's security tracker has no pappl entry at all.

Agreed actions, in order:
1. **Read the 1.3.1 source first** and confirm the vulnerable lines are present
   before filing anything. A Debian bug asserting an unconfirmed CVE gets
   closed; one citing the two upstream commits and the corresponding lines in
   1.3.1 does not. Report findings, then file.
2. **Determine whether we are exposed to the dithering issue at all.** If the
   driver declares only 1-bit black raster and never accepts 8-bit grayscale,
   PAPPL's dithering path may be unreachable for us. Answer this as part of the
   P9 raster-type decision and record the conclusion in
   `docs/SECURITY-REVIEW.md`. If declaring only `BLACK_1` removes the exposure
   that is an argument for doing so, but it must not override what the hardware
   and the existing engine actually need.
3. **Default the systemd unit to loopback only.** Network exposure is a
   deliberate opt-in via configuration, documented in the README.
4. **Record the unpatched dependency as a known issue** in
   `docs/SECURITY-REVIEW.md` and in the README, with the Debian bug number once
   filed.

### Q-2 — Where the hard margin comes from
**Decision: a driver constant derived from the PPD; no zero fallback.**

Reasoning as above. Still to determine empirically, deferred to P5: whether
PAPPL delivers `cupsWidth`/`cupsHeight` and scanlines for the *printable area*
or the *full media*. To be answered by experiment, not documentation, and
written up in `docs/MARGINS.md`.

### Q-3 — Duplex
**Decision: out of scope for 2.0, but advertise `sides-supported` explicitly as
one-sided only rather than omitting the attribute.**

IPP clients handle a present-but-limited attribute better than a missing one.
The exclusion and its reasoning are recorded in `docs/NON-GOALS.md` so it is
not re-litigated.

### Q-4 — Toner save / density
**Decision: deferred. No invented PJL values.**

Replace the hardcoded `@PJL SET DENSITY=3` with a named constant carrying a
comment citing where the value came from, so a future capture can be wired in
without archaeology. Expose no vendor option for it now. Recorded in
`docs/NON-GOALS.md` with a note on exactly what evidence would unblock it.

### Q-5 — Clean break: what we ship vs what we keep
**Decision: approved for what we SHIP, not approved for what we DELETE.**

- The 1.x filter code **stays in the tree** until P11 passes green, because the
  golden corpus can only be regenerated while it runs. Freeze it: exclude it
  from the built artifact and mark it clearly as frozen. Do not remove it.
- `ppd/samsung-ml2160.ppd` **stays permanently**. Under Q-2 it is the source of
  truth for the hard-margin table, so it is now project *data*, not a shipped
  artifact. It must not be installed by the .deb, but it must remain in the
  source package and be listed in `debian/copyright`.
- The tree is tagged `v1.x-final` before migration work so the working 1.x
  driver stays trivially recoverable.
- A concrete list of files to **stop shipping** versus **stop keeping** must be
  written out and approved separately before anything is deleted.

### Q-6 — Does `spl2-core` keep the CUPS raster parser
**Decision: keep `raster.rs`, behind a non-default Cargo feature named
`golden-replay` — not `#[cfg(test)]`.**

`#[cfg(test)]` items are compiled only for their own crate's unit tests and are
invisible to an integration test in another crate's `tests/` directory, which
is where the golden harness will live once the workspace is split. The harness
enables the feature explicitly, and CI builds both with and without it.

### Q-7 — Static or dynamic linking for the .deb
**Decision: dynamic, against the archive's `libpappl1t64`.**

Answered together with Q-1; it is not an open question. Statically linking
libpappl would put Apache-2.0 code inside a GPL-2.0-only binary and rest the
whole package on PAPPL's linking exception, on top of the packaging costs
listed under Q-1. The 1.x musl static build does not carry over to 2.0.

### Q-8a — Licence for the `pappl` safe wrapper
**Decision (2026-09-05): both FFI crates are licensed `Apache-2.0 OR MIT`.**
`pappl-sys` and `pappl` both carry the standard Rust dual licence;
`spl2-core` and `ml216x-printer-app` stay `GPL-2.0-only`.

The MIT arm is what makes the arrangement work: plain Apache-2.0 on either
crate would create the same internal incompatibility, because an Apache-2.0
crate linked into a GPL-2.0-only binary imposes the patent-termination and
notice terms that GPLv2 section 6 treats as "further restrictions", and
PAPPL's linking exception covers PAPPL's own code, not ours. A GPL-2.0-only
consumer — this project's binary — takes the MIT arm and the conflict
disappears; the Apache-2.0 arm preserves the "match upstream" intent for
anyone reusing the bindings elsewhere. Dual-licensing is also what the wider
Rust ecosystem expects of a `-sys` crate, so it costs nothing in reusability.

This is a practical licensing convention, not legal advice. Note also that it
is only a question because the project is GPL-2.0-**only**: relicensing the
repository as `GPL-2.0-or-later` would make the Apache-2.0 incompatibility
moot, since Apache-2.0 is compatible with GPLv3. That relicence is not
proposed here — `spl.rs` is derived from GPLv2-only SpliX, so it is not ours
to make unilaterally — but it is the lever to pull if this is ever revisited.

**Actions this decision carries.** Done now: `LICENSE-APACHE` and `LICENSE-MIT`
at the repository root, and the per-crate split recorded in
`packaging/debian/copyright`. Outstanding, and only actionable once the crates
exist (they do not yet — nothing under `crates/` has been written): the
`license = "Apache-2.0 OR MIT"` field in both crates' `Cargo.toml`, and an
`SPDX-License-Identifier: Apache-2.0 OR MIT` header on every file in them.
Every other file in the tree stays `GPL-2.0-only`; the SPDX-header gap
recorded in the audit table below is unchanged and still open.

The audit that led here — the repository's licence was checked before
anything was assigned:

| Source | States | Agrees? |
|---|---|---|
| `Cargo.toml` `license =` | `GPL-2.0-only` | yes |
| `src/spl.rs:8` header | "GPLv2 (v2 only — same as the SpliX source)" | yes |
| `packaging/debian/copyright` | `License: GPL-2` (DEP-5 short name for v2-only; the or-later form would be `GPL-2+`) | yes |
| `LICENSE` | stock GPLv2 text only | neutral |
| `src/main.rs`, `src/raster.rs`, `src/golden.rs`, `ppd/*.ppd` | **no licence header at all** | gap |
| SPDX identifiers | **none anywhere in the tree** | gap |

The three declarations that speak all agree: **`GPL-2.0-only`**. The `LICENSE`
file is the stock GPLv2 document; its "either version 2 … or (at your option)
any later version" wording appears only inside the FSF's *"How to Apply These
Terms to Your New Programs"* appendix, which is boilerplate, not a statement
about this project. The v2-only choice is also substantively forced: `spl.rs`
is derived from SpliX, which is GPLv2-only, so the project cannot be
or-later.

**Therefore Apache-2.0 for our own wrapper would create a real internal
incompatibility** — Apache-2.0's patent-termination and notice clauses are
"further restrictions" under GPLv2 §6, and PAPPL's linking exception covers
PAPPL's code, not code we write.

Recommendation: **MIT for the `pappl` wrapper.** It is GPLv2-compatible, so no
internal conflict; it is permissive, so the wrapper stays reusable, which was
the entire point of not making it GPL; and it avoids the Apache-2.0 patent
clause that causes the incompatibility. `GPL-2.0-or-later` would also link
cleanly but would defeat the reuse goal.

The wrinkle that this recommendation missed, and that the decision above
resolves: `pappl-sys` at plain Apache-2.0 had the *same* problem as the
wrapper, for the same reason — it too would be linked into the GPL-2.0-only
binary. The earlier approval of "pappl-sys stays Apache-2.0" was taken before
that was noticed and is superseded; both crates are now `Apache-2.0 OR MIT`.

### Q-8b — Missing licence header on the PPD
**Decision: add a GPL-2 header to `ppd/samsung-ml2160.ppd`.**

More important under Q-5, not less: the PPD is now permanent project data and
the source of truth for the hard-margin table.

### Q-9 — Golden-file harness
**Decision: approved, with JSON sidecars and a margin-specific corpus case.**

Each golden `.spl` gets a sidecar recording the classic CUPS page-header values
that produced it (at minimum `Margins[]`, `ImagingBoundingBox`, `cupsWidth`,
`cupsHeight`, `cupsBytesPerLine`, `HWResolution`, `cupsBitsPerPixel`,
`cupsColorSpace`, `PageSize`, media name). These sidecars are the reference the
PAPPL-side option mapping is validated against and can only be captured while
the 1.x code runs. The corpus includes a page with 1-pixel registration marks
at the exact printable-area corners, on A4 and Letter, at every supported
resolution, so a margin regression fails at byte level rather than at ruler
level.

**Status: implemented.** See `src/golden.rs`, `goldens/`, and
`goldens/README.md`.

### Q-10 — Device transport and discovery
**Decision: USB first, socket second. Do not defer socket past 2.0 without
asking.**

Socket support is largely built into PAPPL, so once USB works the incremental
cost should be small; if it turns out not to be, raise it before doing the
work. The PPD advertises ML-2165W, and a wireless-only user gets nothing from a
USB-only release.

### Q-11 — Language for new code
**Decision: English for all code, comments, identifiers, commit messages,
documentation and packaging metadata.**

This is a GPL open-source project and contributors will not read Turkish. Any
Turkish user-facing text would be a separate translation layer added
deliberately later, never inlined into source.

**Known deviation:** `src/golden.rs` and `goldens/README.md` were written in
Turkish before this decision was taken and need converting.
