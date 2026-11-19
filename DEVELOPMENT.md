# Development

## Windows desktop app

Sable uses Tauri 2 with a vanilla HTML, CSS, and JavaScript interface. The frontend is in app/. Rust provides package import/export and TEX conversion using LeagueToolkit, alongside Tauri's native file dialogs and File Explorer handoff.

### Build a test executable

Install the Rust stable toolchain, Microsoft C++ Build Tools, and the WebView2 Runtime. From the repository root, install the Tauri CLI and build:

    cargo install tauri-cli --version "^2" --locked
    cargo tauri dev

To build a standalone executable without an installer:

    cargo tauri build --no-bundle

The executable is written under src-tauri/target/release/. The GitHub Actions workflow builds and uploads it when app/ or src-tauri/ changes are pushed to main, or when started manually. The direct executable is for early testing; a signed installer can be added later.

### Unpack and repack flow

Sable imports .fantome and .modpkg packages into a LeagueToolkit mod project in project/. It decodes TEX files to PNG companions under Textures/ and stores a hash map so only changed PNG files are converted back. Repack uses the original package format and saves a new archive under Repacked/.

The source package is selected through the native file dialog. The workspace parent is chosen by the user; the Rust commands read and write only those selected paths. WebView filesystem permissions remain restricted to paths selected through dialogs.

### Project structure

- app/index.html and app/theme.css: shared browser and desktop interface.
- app/model-preview.js: SKN geometry parsing and WebGL mesh viewer with CPU skinning for matching SKL/ANM assets. Named package companions appear in the animation selector; WAD chunks can be paired manually because their paths are hashes. Textures/materials and ability VFX are not rendered yet.
- src-tauri/src/workspace.rs: native mod import, TEX conversion, repacking, and File Explorer opening.
- src-tauri/capabilities/default.json: narrow Tauri permissions for selected paths.
- .github/workflows/build-windows-exe.yml: Windows build and downloadable executable artifact.
- index.html: redirect to the browser interface for GitHub Pages.

### Format notes

The LeagueToolkit texture crate decodes all shipped TEX formats. Repacking edits is enabled for BC1, BC3, BC7, and BGRA8; ETC, BC5, and float texture previews remain unchanged during export. Block-compressed edited textures must use dimensions divisible by four for game-ready output. See the upstream [texture documentation](https://github.com/LeagueToolkit/league-toolkit/tree/main/crates/ltk_texture) and [mod project documentation](https://github.com/LeagueToolkit/league-mod/tree/main/crates/ltk_mod_project).
