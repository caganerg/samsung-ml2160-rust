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

`test_pipeline.sh` generates a sample PDF (via Ghostscript), converts it to a CUPS raster stream with `cupsfilter`, runs it through the filter, and parses/validates the resulting SPL2 file's PJL/QPDL record structure (page header, band records, checksums, job end):

```sh
./test_pipeline.sh                 # auto-generates a sample PDF
./test_pipeline.sh mine.pdf        # or use your own PDF
```

Unit tests (including an Algo 0x11 RLE round-trip test):

```sh
cargo test
```

## Project Structure

- `src/main.rs` — CUPS filter entry point: argument parsing, page/band loop
- `src/raster.rs` — CUPS Raster (V1/V2/V3) header parser
- `src/spl.rs` — SPL2/QPDL protocol: PJL envelope, page/band records, Algo 0x11 RLE
- `ppd/samsung-ml2160.ppd` — CUPS PPD file
- `test_pipeline.sh` — end-to-end pipeline test and SPL2 format validator

## License

GPLv2 (v2 only) — see [LICENSE](LICENSE). The protocol implementation is derived from the GPLv2-licensed OpenPrinting SpliX project, so it's licensed to match.
