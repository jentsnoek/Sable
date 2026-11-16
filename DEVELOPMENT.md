# Development

## Windows desktop app

Sable uses Tauri 2 with the existing vanilla HTML, CSS, and JavaScript interface. The frontend is in app/ and is loaded directly by the Tauri shell. Rust currently starts the application and registers Tauri's dialog and filesystem plugins for native file selection and unpacking.

### Build a test executable

Install the Rust stable toolchain, the Microsoft C++ Build Tools, and the WebView2 Runtime. From the repository root, install the Tauri command line tool and build:

    cargo install tauri-cli --version "^2" --locked
    cargo tauri dev

To build the standalone executable without producing an installer:

    cargo tauri build --no-bundle

The executable is written under src-tauri/target/release/. The GitHub Actions workflow builds and uploads it when changes are pushed to main, or when the workflow is started manually. This direct executable is a convenient early test build. A signed installer can be added later.

### Project structure

- app/index.html: shared browser and desktop interface.
- src-tauri/: Tauri configuration, capabilities, and Rust entry point.
- .github/workflows/build-windows-exe.yml: Windows build and downloadable executable artifact.
- index.html: small redirect to the app interface for GitHub Pages.

### Current limits and next steps

Sable can inspect and unpack supported package and WAD data. It does not yet edit binary assets or repack a mod. The next major technical step is to integrate LeagueToolkit through Rust for reliable package and WAD editing, then add safe round-trip export. Keep WAD files and package originals unchanged unless the user explicitly exports a modified copy.

The browser interface includes a pinned fzstd fallback from jsDelivr for Zstandard chunks that WebView2 cannot decode natively. The app currently needs an internet connection to fetch that fallback when required.
