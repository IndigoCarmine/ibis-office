<p align="center"><b>English</b> | <a href="README-ja.md">日本語</a></p>

<p align="center">
  <img src="asset/AcaudaOffice_logo.svg" alt="Acauda Office" width="480">
</p>

<p align="center">
  <b>An unofficial fork of ibisOffice, the office suite built with Rust and the GPU.</b><br>
  Word (.docx), Excel (.xlsx) and PowerPoint (.pptx).<br>
  Powered by <a href="https://github.com/ibisjp/ibis-office">ibisOffice</a>.
</p>

---

## What is Acauda Office?

Acauda Office is a fork of [ibisOffice](https://github.com/ibisjp/ibis-office) by ibis inc. It follows ibisOffice and adds features of its own on top.

**This is not an official ibis inc. product, and ibis inc. has not approved or sponsored it.** Please report problems with Acauda Office in [this repository's Issues](https://github.com/IndigoCarmine/ibis-office/issues), not to ibis inc.

The name comes from the long-tailed tit, *<b>A</b>egithalos <b>cauda</b>tus* (シマエナガ in Japanese), as a small bird beside the ibis.

## What this fork adds

- **Embedded OLE objects edited in their own editors**: open, paste, copy and show embedded objects (for example ChemDraw structures) through an external editor, over the protocol in [`crates/ibis-embed-protocol`](crates/ibis-embed-protocol/README.md).
- **Transparent pictures**: whole-picture transparency, EMF alpha, and unfilled OLE shapes.
- **Quick Access Toolbar**.
- **Smoother window resizing** with large slides.
- **Windows installer**: a per-user Inno Setup installer, besides the release zip and its SHA-256.

For what ibisOffice itself can do (its features, compatibility and speed), see the [ibisOffice README](https://github.com/ibisjp/ibis-office#readme) and [Implemented Features](docs/implemented-feature.md).

## Download

Builds are on the [Releases](https://github.com/IndigoCarmine/ibis-office/releases) page.

- **Windows**: unzip `AcaudaOffice-<version>-Windows.zip` and run `AcaudaOffice.exe` in the `AcaudaOffice` folder, or install with `AcaudaOffice-<version>-Windows-Setup.exe` (for your user only; no administrator rights needed).
- **macOS**: unzip `AcaudaOffice-<version>-macOS.zip` and move `AcaudaOffice.app` to your Applications folder.
- **Linux**: download `AcaudaOffice-<version>-Linux.AppImage`, allow it to run (`chmod +x`), and double-click.

Acauda Office keeps its settings apart from ibisOffice's (in `AcaudaOffice` folders), so both can be installed side by side. Its automatic update looks only at this repository's releases.

The command-line options are described in [Command-Line Options](docs/cli-options.md).

## Building

```bash
git clone --recurse-submodules https://github.com/IndigoCarmine/ibis-office.git
cd ibis-office
cargo build --release
```

- Requires the latest stable Rust.
- On macOS, `scripts/bundle-macos.sh` builds the app at `target/release/AcaudaOffice.app`.
- On Windows, `pwsh scripts/bundle-windows.ps1` builds the release zip and the installer in `target/dist` (the installer needs [Inno Setup 6](https://jrsoftware.org/isdl.php)).
- On Linux, `scripts/bundle-linux.sh` builds the AppImage in `target/dist`.
- For the browser edition, build with `scripts/build-web.sh --release` and serve `www/` (browsers with WebGPU).

## License

Acauda Office is released under **AGPL-3.0-only** ([LICENSE](LICENSE)).

- The code from ibisOffice is copyright ibis inc. The additional term under AGPL-3.0 section 7(b) in [NOTICE](NOTICE) applies: Acauda Office shows "Powered by ibisOffice", with a link to https://github.com/ibisjp/ibis-office, on its About screen.
- ibis inc. also offers ibisOffice under a commercial license ([COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md)). That license covers ibisOffice from ibis inc. only; this fork is offered under AGPL-3.0 alone.
- Third-party components and their notices are listed in [THIRD-PARTY-NOTICES.md](docs/THIRD-PARTY-NOTICES.md).

## Contributing

Issues and pull requests are welcome in this repository. See [CONTRIBUTING.md](docs/CONTRIBUTING.md). Please report security issues as described in [SECURITY.md](docs/SECURITY.md).

## Trademarks

ibisOffice and its logo are trademarks of ibis inc. ([TRADEMARK.md](docs/TRADEMARK.md)). Acauda Office is a separate name, and does not use the ibisOffice logo.

Microsoft, Microsoft Office, Word, Excel and PowerPoint are registered trademarks or trademarks of Microsoft Corporation in the United States and other countries. All other company and product names are trademarks of their respective owners. Acauda Office is not affiliated with, endorsed by or sponsored by any of these companies.
