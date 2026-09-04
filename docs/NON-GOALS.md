# Non-Goals for the 2.0 Printer Application

Things deliberately left out of the 2.0 release, with the reasoning that put
them here and — where it applies — the evidence that would unblock them.

This document exists so these exclusions are not re-litigated. If you are about
to implement one of these, read the "what would unblock it" note first; if the
evidence it asks for still does not exist, the exclusion still stands.

Decided 2026-09-05; see `docs/DECISIONS.md` for the full decision log.

---

## Duplex printing

**Excluded from 2.0.** The Printer Application advertises single-sided output
only.

The SPL2/QPDL side is already implemented and tested: `duplex_mode`
(`src/main.rs:516`) maps the CUPS `Duplex`/`Tumble` pair to `SplDuplex`,
`begin_job` emits `@PJL SET DUPLEX=MANUAL` plus `BINDING=LONGEDGE|SHORTEDGE`,
and `begin_page` writes the duplex and tumble bytes at page-header offsets
`0xB` and `0xC`. None of it is reachable today, because
`ppd/samsung-ml2160.ppd` has no `*OpenUI *Duplex` block, so CUPS never sets
`Duplex` in the raster header and every job is Simplex.

Three reasons it stays out:

1. **No hardware to validate it on.** The ML-2160 series has no automatic
   duplexer; every duplex job on this family is *manual* duplex, which cannot
   be verified without a person feeding paper back in.
2. **It requires two-pass page ordering that does not exist.** Manual duplex
   prints one side of the whole job, then the operator reloads and the other
   side prints. The current pipeline streams pages in arrival order in a single
   pass and never splits the page sequence into passes. Shipping duplex without
   this prints double-sided jobs in the wrong order.
3. **It requires a last-page tray override that cannot be computed while
   streaming.** SpliX's `qpdl.cpp renderPage` temporarily switches the paper
   source to `Multi` (3) on the front-side pass of every page except the last
   (`if (tumble && !lastPage) paperSource = 3`). In a streaming CUPS raster
   pipeline the next page header cannot be read until the current page's data
   is consumed, so "is this the last page?" is unknown when the page header is
   written. Doing it correctly means buffering a whole page in memory — up to
   roughly 22 MB at the largest accepted geometry.

Both gaps are documented in the code at `src/main.rs:509` and
`src/main.rs:968`.

**Requirement for 2.0 despite the exclusion:** publish `sides-supported`
explicitly as one-sided only, rather than omitting the attribute. IPP clients
handle a present-but-limited attribute better than a missing one.

**What would unblock it:** access to the hardware for manual-duplex testing,
plus a decision to buffer pages (or to reorder at the job level) so the
two-pass sequence and the last-page tray override can both be implemented.

---

## Toner save / print density

**Excluded from 2.0.** No vendor option is exposed for density or economode.

`begin_job` currently emits a hardcoded `@PJL SET DENSITY=3`
(`src/spl.rs:888`), alongside several other hardcoded PJL settings
(`POWERSAVE=ON`, `POWERSAVETIME=5`, `JAMRECOVERY=OFF`, `ALTITUDE=LOW`,
`RET=NORMAL`). There is no PPD option for any of them and no code path that
varies them.

The reason it stays out is simple: **we do not know the printer's real PJL
vocabulary for toner save.** Project rule 1 forbids inventing protocol details,
and every other protocol constant in this driver is traceable to the existing
code or to upstream SpliX. There is no such source for an economode setting —
neither this project's PPD nor any of the sibling SpliX PPDs
(`ml1910.ppd`, `ml2010.ppd`, `ml2525.ppd`, `ml1640.ppd`, `ml2510.ppd`)
declares one. Guessing at `@PJL SET ECONOMODE=ON` or at the meaning of the
`DENSITY` scale risks either a silently ignored setting or a firmware-level
rejection.

**Required cleanup in 2.0 even though the feature is excluded:** replace the
hardcoded `3` with a named constant carrying a comment that cites where the
value came from, so a future capture can be wired in without archaeology.

**What would unblock it:** a capture of the traffic the Samsung/Windows driver
sends when toner save is toggled — enough to show the exact PJL line, its
accepted values, and whether `DENSITY` is the same knob or a separate one.
A packet or USB capture, or a vendor `.ppd`/`.ini` that names the option, would
all do. With that evidence the option can be added as a proper IPP vendor
attribute.
