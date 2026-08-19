# samsung-ml2160-rust

A CUPS raster filter (`rastertospl-rust`) for Samsung ML-2160 series monochrome laser printers, written in Rust. It converts CUPS's standard raster stream (`RaSt`/`RaS2`/`RaS3`) into the printer's native binary **SPL2 / QPDL v3** format: PJL job envelope, 17-byte page header, Algo 0x11 RLE-compressed band records, and checksums.

The protocol implementation was verified against the actual source of the [OpenPrinting SpliX](https://github.com/OpenPrinting/splix) driver (`document.cpp`, `compress.cpp`, `qpdl.cpp`, `algo0x11.cpp`, `printer.cpp`) and tested on real hardware.

## Supported Models

ML-2160, ML-2165, ML-2165W, ML-2168 (same QPDL v3 protocol family).

## Requirements

- Rust toolchain (`cargo`)
- CUPS (`lpadmin`, `lpinfo`, `cupstestppd`)
- Printer connected via USB and powered on

## Installation

```sh
./install.sh [queue-name] [device-uri]
```

The script builds the project (`cargo build --release`), validates the PPD file (`cupstestppd`), installs the filter binary into `/usr/lib/cups/filter/`, and auto-detects a connected Samsung ML-2160 series USB printer to register as a CUPS queue (default name `ML2160_Rust`). It only asks for a `sudo` password on the steps that write to system files — don't run the whole script with `sudo`.

If you're using a non-USB connection (e.g. a network printer), pass the device URI manually:

```sh
./install.sh MyPrinter "ipp://192.168.1.50/ipp/print"
```

Send a test print after installing:

```sh
lp -d ML2160_Rust file.pdf
```

### Manual Installation

If you'd rather not use `install.sh`, run the same steps by hand:

```sh
cargo build --release
sudo install -m 755 -o root -g root target/release/rastertospl-rust /usr/lib/cups/filter/rastertospl-rust
sudo lpadmin -p ML2160_Rust -E -v <device-uri> -P ppd/samsung-ml2160.ppd
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
- `test_pipeline.sh` — end-to-end pipeline test and SPL2 format validator

## License

GPLv2 (v2 only) — see [LICENSE](LICENSE). The protocol implementation is derived from the GPLv2-licensed OpenPrinting SpliX project, so it's licensed to match.
