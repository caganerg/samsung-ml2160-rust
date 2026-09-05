# Hard Margins — an open question that hardware settles

*Written 2026-09-05, ahead of P5. **The margin table has not been changed.***

The driver's hard margin is a driver constant derived from
`ppd/samsung-ml2160.ppd`, which declares `*ImageableArea … "12 12 …"` — 12 pt
on every edge of every medium (decision Q-2). While fetching the SpliX source
to settle the copyright attribution, it turned out that **upstream SpliX does
not use 12 pt for either of the two models this driver claims to support**, and
that it uses two different values for them.

This document records what upstream says, what it would cost to be wrong, and
the three ways the question can be resolved. It is deliberately open: the
person reading it has the hardware, and a ruler settles in one page what no
amount of source reading can.

## What upstream actually says

From the Debian source package `splix 2.0.1-1` (`http://splix.ap2c.org/`),
which is the source this driver's protocol is derived from.

`ppd/samsung.drv.in` puts the ML-1915 and ML-2165 in a block of their own, with
an explicit override and a comment saying why:

```
266  }
267  }
268
269  //
270  // ML-1915/ML-2165 printers (different margins than the other monochrome
271  // printers)
272  //
273  {
274      HWMargins 12.5 12.5 12.5 12.5
275      #import "spl2basic.defs"
…
302                  ModelName "ML-2165"
303                  PCFileName "ml2165.ppd"
```

The ML-2160 is not in that block. It sits at line 241, inside the file's
outermost group, which imports `spl2.defs` at line 19 and never overrides the
margins:

```
19   #import "spl2.defs"
…
241                  ModelName "ML-2160"
242                  PCFileName "ml2160.ppd"
```

and `ppd/spl2.defs` sets:

```
 9  // Supported paper format
10  HWMargins 10.75 15 10.75 15
```

No other file in the import chain sets `HWMargins` — `spl2basic.defs` and
`monochrome-v2.defs` do not, and the only other definition in the tree,
`ppd/spl2bandedjbig.defs` line 12, belongs to the banded-JBIG colour printers
imported at `samsung.drv.in:453`, not to the ML-216x at all:

```
10  // For banded jbig printers, all hardware margins seems to be 12pt.
11  // HWMargins left bottom right top
12  HWMargins 12 12 12 12
```

So, per upstream:

| Model | Upstream `HWMargins` (left bottom right top) | Source |
|---|---|---|
| **ML-2160** | `10.75 15 10.75 15` | `spl2.defs:10`, via `samsung.drv.in:19` |
| **ML-2165** (and ML-1915) | `12.5 12.5 12.5 12.5` | `samsung.drv.in:274`, explicit override |
| *(banded-JBIG colour models)* | `12 12 12 12` | `spl2bandedjbig.defs:12` — **not** these printers |
| **This driver's PPD** | `12 12 12 12` | `ppd/samsung-ml2160.ppd` |

Two things follow that were not understood before. Our 12 pt matches **neither**
model upstream describes; the file it does match covers different hardware.
And the ML-2160's upstream margin is **asymmetric** — 10.75 pt left/right,
15 pt top/bottom — whereas every value in our PPD is 12.

Upstream's confidence is worth noting too. The comment attached to the 12 pt
definition says "all hardware margins **seems to be** 12pt". Upstream was
estimating in at least that file, so "upstream says X" is evidence, not proof.

## What it costs to be wrong

`hard_margin_bytes` (`src/main.rs`) converts the margin to pixels and rounds
**up to a whole 8-pixel column**, because the band buffer is byte-addressed.
That rounding is what turns a fraction of a point into a visible error:

| Left margin | @300 dpi | @600 dpi | @1200 dpi |
|---|---|---|---|
| 10.75 pt (upstream ML-2160) | 6 B | 12 B | 23 B |
| **12 pt (this driver today)** | **7 B** | **13 B** | **25 B** |
| 12.5 pt (upstream ML-2165) | 7 B | 14 B | 27 B |

At 300 dpi the 12 and 12.5 pt cases collapse to the same 7 bytes, so the
question is invisible there. Everywhere else it is a whole number of byte
columns:

* **12 vs 12.5 pt** — 1 byte at 600 dpi, 2 bytes at 1200 dpi: 8 and 16 pixels
  respectively, both **≈ 0.34 mm**.
* **12 vs 10.75 pt** — 1 byte at 600 dpi, 2 bytes at 1200 dpi, the same
  ≈ 0.34 mm, in the other direction.

If the true hard margin is larger than the table says, the whole raster lands
that far to the right of where the engine expects, and the rightmost byte
column is pushed towards — or past — the edge of the printable area. This is
the R-1 failure mode, and it is the same class of bug as the D-06 regression
recorded at `src/main.rs:427`, which was 13 bytes and about 4 mm. A third of a
millimetre will not be noticed by eye; it will be measured.

The vertical axis carries the same question. The driver relies on the
horizontal and vertical origins agreeing — `band_placement`'s documentation
works through the 600 dpi case, where centring (100 lines) and `hardMarginY`
(12 pt = 100 lines) cancel exactly. If the ML-2160's true top margin is 15 pt
(125 lines at 600 dpi) rather than 12, that cancellation is wrong by 25 lines
and the page shifts vertically as well.

## The three resolutions

To be picked after measurement, not before.

**(a) Upstream is right and one value serves everything.** The table becomes a
single upstream-derived value for all models. Note this now has two sub-cases
rather than one: 12.5 pt (if the ML-2165 override describes the family) or
10.75/15 pt (if `spl2.defs` does). "12.5 for all models" is only half of
option (a) as originally framed, because our 12 pt was never upstream's value
for the ML-2160 either.

**(b) The models genuinely differ, and the driver-capability table carries
per-model margins.** Upstream separated the ML-2165 deliberately and said so in
a comment, which is a model-specific claim rather than an accident. PAPPL's
driver-capability table can express per-model margins without difficulty; the
single classic PPD cannot, which is an argument for the migration rather than
against it.

**(c) Upstream's `.drv` is wrong for our hardware and 12 pt stays**, with the
comment at the margin table replaced by a citation of the measurement that
proved it — the model, the resolution, the medium, the measured distance.

**What I expect: (b)**, and with our current 12 pt wrong for both models. The
reasoning is that upstream's separation of the ML-2165 is deliberate and
documented, and that our 12 pt traces to a defs file written for different
hardware — which looks like how it got here, rather than a measurement anyone
made on an ML-216x. But (c) is entirely live: upstream hedges in the very file
our value seems to come from, and no source reading beats one printed page.

## How to settle it

Print a `*-marks` golden through the 1.x path and measure. The corpus contains
one-pixel registration marks at the exact printable-area corners on A4 and
Letter at every supported resolution, so the distance from the sheet edge to
the first printed dot **is** the hard margin, and 600 or 1200 dpi will show a
0.34 mm error where 300 dpi will not.

This is release gate **G-1** (`docs/GOLDEN-VALIDATION.md` §4). Two constraints
it now carries:

* **Record the exact model.** Measuring an ML-2160 says nothing about the
  ML-2165 and vice versa — that is the whole question.
* **Measure every model we claim.** The README and PPD advertise ML-2160,
  ML-2165, ML-2165W and ML-2168. If the models differ, G-1 needs a measurement
  per model, or the claim narrows to what has been measured.

## If the table changes

Changing the margin changes the bytes that reach the printer, so every one of
the 32 goldens moves. That is a deliberate behaviour change and follows the
bless discipline in `CONTRIBUTING.md`: refresh in the same commit, cite the
measurement in the commit message, and include the diff in review. The
registration-mark cases exist precisely so that this shows up as a byte
difference rather than a ruler difference.
