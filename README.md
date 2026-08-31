# samsung-ml2160-rust
> [!WARNING]
> **Experimental Driver:** This project is currently in active development and should be considered experimental. Use at your own risk. Feedback, bug reports, and pull requests are highly appreciated!

A CUPS raster filter (`rastertospl-rust`) for Samsung ML-2160 series monochrome laser printers, written in Rust. It converts CUPS's standard raster stream (`RaSt`/`RaS2`/`RaS3`) into the printer's native binary **SPL2 / QPDL v3** format: PJL job envelope, 17-byte page header, Algo 0x11 RLE-compressed band records, and checksums.

The protocol implementation was verified against the actual source of the [OpenPrinting SpliX](https://github.com/OpenPrinting/splix) driver (`document.cpp`, `compress.cpp`, `qpdl.cpp`, `algo0x11.cpp`, `printer.cpp`) and tested on real hardware.

## Supported Models

ML-2160, ML-2165, ML-2165W, ML-2168 (same QPDL v3 protocol family).

## Requirements

- Rust toolchain (`cargo`)
- CUPS (`lpadmin`, `lpinfo`, `cupstestppd`)
- Printer powered on and reachable — over USB, or over the network on the JetDirect port (9100)

## Installation

There is no install script — the steps below *are* the procedure. Run them from
the repository root as your normal user: only the two `sudo` lines write to
system paths, so **don't run the whole sequence as root**.

### 1. Check that the repository path is under your control

```sh
namei -l .
```

Every component from `/` down to the repository must be owned by you or by
root, and none of them may be group- or world-writable (a sticky directory such
as `/tmp` is acceptable for the parents, since only an entry's owner can replace
it there). Check the build inputs too — `src/`, `Cargo.toml` and `Cargo.lock`
end up compiled into the binary, and `ppd/` into the installed PPD:

```sh
find . -path ./target -prune -o -perm /022 -print
```

This is not boilerplate caution. Steps 3 and 4 copy the filter binary and the
PPD into system locations **as root**, and a PPD is not a passive config file: its
`*cupsFilter`/`*cupsFilter2` line names the program CUPS executes for every
print job, as user `lp`. Anyone who can write into the repository — or replace
any directory above it — can therefore have a program of their own installed as
a root-owned CUPS filter.

### 2. Find your printer's device URI

```sh
lpinfo -v
```

Copy the URI from the second column of the line matching your printer — a USB printer looks like `usb://Samsung/ML-2165W%20Series?serial=...`, an mDNS/Bonjour-discovered one like `dnssd://Samsung%20ML-2165W%20Series._pdl-datastream._tcp.local/`. A network/Wi-Fi model that isn't listed (e.g. an ML-2165W that mDNS hasn't found) accepts raw print data on the JetDirect port, so use `socket://<printer-ip>:9100` — these printers do not speak IPP.

Every form is used the same way below, so only the value changes:

```sh
DEVICE_URI="usb://Samsung/ML-2165W%20Series?serial=Z1A2B3C4D5"   # USB
DEVICE_URI="socket://192.168.1.50:9100"                         # network / Wi-Fi (JetDirect)
```

Keep it quoted everywhere: `usb://` URIs contain `?` and `&`, which the shell would otherwise interpret.

### 3. Build, install the filter, validate the PPD

```sh
cargo build --release
sudo install -m 755 -o root -g root \
    target/release/rastertospl-rust /usr/lib/cups/filter/rastertospl-rust
cupstestppd ppd/samsung-ml2160.ppd
```

- **Use `install`, not `cp`.** The `-m 755 -o root -g root` flags matter: a filter that is writable by a non-root user is a filter someone else can replace.
- **Run `cupstestppd` after the binary is in place.** It checks that the file referenced by the PPD's `cupsFilter`/`cupsFilter2` line actually exists, so running it first reports a failure that isn't real.

### 4. Register the CUPS queue

```sh
sudo lpadmin -p ML2160_Rust -E -v "$DEVICE_URI" -P ppd/samsung-ml2160.ppd
```

`ML2160_Rust` is the queue name and is yours to choose; CUPS allows at most 127 characters and rejects spaces, `/` and `#`, so stick to letters, digits, `_`, `.` and `-`.

Then send a test print:

```sh
lp -d ML2160_Rust file.pdf
```

> [!NOTE]
> Pick the device URI yourself rather than letting anything auto-detect it. CUPS device discovery is unauthenticated — over the network (mDNS/Bonjour/SNMP) and over USB (descriptor strings) alike — so any device can advertise itself as a "Samsung ML-216x" and be wired up as the print destination, silently receiving your documents over unencrypted JetDirect. Reading `lpinfo -v` and choosing the line yourself is the review step that prevents this.

## Print options

Beyond page size and resolution, the PPD exposes two options that the filter
reads from the CUPS raster page header and forwards to the printer:

```sh
lp -d ML2160_Rust -o InputSlot=Manual -o MediaType=ENV envelope.pdf
lpoptions -d ML2160_Rust -l            # list every option and its choices
```

- **`InputSlot`** — `Auto` (the cassette) or `Manual` (the manual feed slot).
  The PPD numbers these with the QPDL paper-source codes themselves
  (`<</MediaPosition 1>>` and `2`), which the filter writes straight into byte
  `0x9` of the QPDL page header.
- **`MediaType`** — `OFF`, `NORMAL`, `THICK`, `THIN`, `BOND`, `OHP`, `CARD`,
  `LABEL`, `USED`, `COLOR`, `ENV`, `COTTON`, `RECYCLED`, `ARCHIVE`. These
  uppercase keywords look unfriendly because they are not labels: each one is
  the literal value sent as `@PJL SET PAPERTYPE=...`, taken from the
  `*MediaType` list in upstream SpliX's PPDs for this engine family
  (`ml1910.ppd`, `ml2010.ppd`, `ml2525.ppd`, `ml1640.ppd`, `ml2510.ppd`). The
  printer picks its fuser temperature and feed speed from this, so it is worth
  setting for envelopes, labels and card stock. `OFF` is the default and means
  "use the printer's own setting". Anything the filter does not recognise falls
  back to `OFF` and is reported on stderr, so a stale PPD shows up in
  `/var/log/cups/error_log` rather than silently printing envelopes on
  plain-paper settings.

> [!IMPORTANT]
> If you installed an earlier version of this PPD, re-run the `lpadmin` command
> from step 4 to load the current one. The older PPD offered paper types under
> readable names (`Plain`, `Envelope`, …) that never reached the printer, and a
> third paper source ("Tray 1") that does not exist on this hardware. Saved
> defaults referring to those names (`lpoptions -o MediaType=Plain`) are no
> longer valid choices and should be set again.

## Uninstallation

### 1. Remove the queue

```sh
lpstat -p                       # if you're unsure of the name
sudo lpadmin -x ML2160_Rust
```

### 2. Remove the filter binary, but only once nothing still uses it

The binary is shared by every queue built on this driver, so deleting it while
another one is still installed breaks that queue silently — its jobs start
failing with "filter failed". Ask which installed PPDs still name the filter:

```sh
sudo grep -rlsF rastertospl-rust /etc/cups/ppd/
```

`lpadmin -x` already deleted the removed queue's own PPD, so it won't appear
here. If the command prints nothing, no queue needs the filter any more:

```sh
sudo rm -f /usr/lib/cups/filter/rastertospl-rust
```

Note that the question is which PPD references the filter, not which queue looks
like a Samsung: a queue created against a plain `socket://<ip>:9100` address
carries no model name anywhere in its device URI.

## Testing

```sh
cargo test
```

75 unit tests cover the CUPS Raster parser (v1/v2/v3, both endiannesses, the
v2 line-RLE decoder), page-header validation, the SPL2/QPDL record layout, the
Algo 0x11 RLE round trip, PJL field sanitisation, and every resource limit the
filter enforces.

Several of them are pinned against measurements from real `cupsfilter` output
rather than from the specification — `test_validate_page_header_accepts_real_cupsfilter_heights`
carries the observed `cupsHeight` for each paper size and resolution (A4 at
600 DPI is 6816 lines, not the 7017 the page dimensions alone suggest, because
the PPD's `*ImageableArea` margins come off first). Keep that table measured,
not computed, if you extend it.

There is no end-to-end script. To check the filter against the real CUPS
toolchain by hand:

```sh
cargo build --release
cupsfilter -p ppd/samsung-ml2160.ppd -m application/vnd.cups-raster -- doc.pdf > test.raster
./target/release/rastertospl-rust 101 testuser Test 1 "" test.raster > out.spl
```

A well-formed `out.spl` starts with `\x1b%-12345X@PJL`, contains
`@PJL ENTER LANGUAGE = QPDL`, and ends with `\t\x1b%-12345X`.

## Project Structure

- `src/main.rs` — CUPS filter entry point: argument parsing, page/band loop
- `src/raster.rs` — CUPS Raster (V1/V2/V3) header parser
- `src/spl.rs` — SPL2/QPDL protocol: PJL envelope, page/band records, Algo 0x11 RLE
- `ppd/samsung-ml2160.ppd` — CUPS PPD file

## License

GPLv2 (v2 only) — see [LICENSE](LICENSE). The protocol implementation is derived from the GPLv2-licensed OpenPrinting SpliX project, so it's licensed to match.
