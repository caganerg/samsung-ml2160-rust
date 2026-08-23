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

First find your printer's device URI:

```sh
lpinfo -v
```

Copy the URI from the second column of the line matching your printer — a USB printer looks like `usb://Samsung/ML-2165W%20Series?serial=...`, an mDNS/Bonjour-discovered one like `dnssd://Samsung%20ML-2165W%20Series._pdl-datastream._tcp.local/`. A network/Wi-Fi model that isn't listed (e.g. an ML-2165W that mDNS hasn't found) accepts raw print data on the JetDirect port, so use `socket://<printer-ip>:9100` — these printers do not speak IPP.

Then pass it, together with the queue name you want, to the install script — it treats every URI form the same way, so only the value below changes:

```sh
DEVICE_URI="usb://Samsung/ML-2165W%20Series?serial=Z1A2B3C4D5"   # USB
DEVICE_URI="socket://192.168.1.50:9100"                         # network / Wi-Fi (JetDirect)

./install.sh ML2160_Rust "$DEVICE_URI"
```

A `dnssd://...` URI copied straight out of `lpinfo -v` is passed exactly like the two above. Quote the URI in every case: `usb://` URIs contain `?` and `&`, which the shell would otherwise interpret.

The script builds the project (`cargo build --release`), verifies that nothing outside your control can substitute the build artefacts, installs the filter binary into `/usr/lib/cups/filter/`, validates the PPD (`cupstestppd`), and registers the CUPS queue. It only asks for a `sudo` password on the steps that write to system files — **don't run the whole script with `sudo`**.

Send a test print:

```sh
lp -d ML2160_Rust file.pdf
```

> [!NOTE]
> Both arguments are required on purpose. The script used to auto-detect the printer by grepping `lpinfo -v`, but CUPS device discovery is unauthenticated — over the network (mDNS/Bonjour/SNMP) and over USB (descriptor strings) alike — so any device can advertise itself as a "Samsung ML-216x" and be wired up as the print destination, silently receiving your documents over unencrypted JetDirect. Reading `lpinfo -v` yourself keeps the same human review without the script having to render device-supplied text in your terminal.

> [!IMPORTANT]
> Keep the repository on a path whose every component is writable only by you or by root. The install script copies the filter binary and the PPD into system locations **as root**, and a PPD is not a passive config file: its `*cupsFilter`/`*cupsFilter2` line names the program CUPS executes for every print job, as user `lp`. If another user can replace the repository directory — or anything above it — they can have their own binary installed as a root-owned CUPS filter. The script refuses to continue if it finds such a path, but you can check yourself with `namei -l .`.

### Manual Installation

If you'd rather not use `install.sh`, run the same steps by hand:

```sh
cargo build --release
sudo install -m 755 -o root -g root \
    target/release/rastertospl-rust /usr/lib/cups/filter/rastertospl-rust
cupstestppd ppd/samsung-ml2160.ppd
sudo lpadmin -p ML2160_Rust -E -v "$DEVICE_URI" -P ppd/samsung-ml2160.ppd
```

`$DEVICE_URI` is the same URI you would have passed to `install.sh` — a `usb://`, `socket://` or `dnssd://` one, depending on the connection.

Two details the script would otherwise handle for you:

- **Use `install`, not `cp`.** The `-m 755 -o root -g root` flags matter: a filter that is writable by a non-root user is a filter someone else can replace.
- **Run `cupstestppd` after the binary is in place.** It checks that the file referenced by the PPD's `cupsFilter`/`cupsFilter2` line actually exists, so running it first reports a failure that isn't real.

## Uninstallation

```sh
./uninstall.sh ML2160_Rust
```

This removes the named queue and then deletes the filter binary — but only once no installed PPD under `/etc/cups/ppd/` still references it, so other queues using this driver keep working. List your queues with `lpstat -p` if you're unsure of the name.

By hand:

```sh
sudo lpadmin -x ML2160_Rust
sudo rm -f /usr/lib/cups/filter/rastertospl-rust   # only if no other queue uses this driver
```

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
- `install.sh` — build + system installation
- `uninstall.sh` — queue and filter removal
- `test_pipeline.sh` — end-to-end pipeline test and SPL2 format validator

## License

GPLv2 (v2 only) — see [LICENSE](LICENSE). The protocol implementation is derived from the GPLv2-licensed OpenPrinting SpliX project, so it's licensed to match.
