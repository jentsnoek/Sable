# Sable

A beginner-friendly editor concept for customizing League of Legends `.fantome` mods, including HUDs and champion skins. The goal is to make supported visual edits understandable without manually digging through a mod's files.

## Current state

`index.html` opens legacy ZIP-based `.fantome` packages and modern League Toolkit `.modpkg` packages locally. It groups and searches package files by likely HUD purpose, asset type, and target WAD when those names are available, opens supported `.wad.client` archives into grouped internal entries, and previews PNG, JPEG, WebP, GIF, BMP, and supported League `.tex` textures. The "Likely HUD" label is guessed from filenames. Package contents stay in the browser; the original file is never changed.

This is a read-only inspection step. WAD entries are identified from their contents and shown by hash because WAD archives do not store the original paths. TEX previews currently support BC1, BC3, and BGRA8 textures. The HUD controls still affect only the sample preview, and export stays disabled until real file edits can be preserved safely.

## Intended workflow

1. Open a downloaded `.fantome` file.
2. Browse its HUD or champion-skin assets in a visual list.
3. Select an asset and make changes supported by that asset type.
4. Export a new edited mod file, leaving the original untouched.

The editor should only show controls that are supported by the selected asset. A texture can be recolored or replaced; a layout element might support moving or resizing; some files may not expose either safely.

## Run Sable

Open `index.html` in a modern browser. There is no build step. If the browser cannot decode Zstandard itself, Sable loads the small `fzstd` decoder from jsDelivr when a compressed entry is opened; the mod file contents are not sent to the CDN.

## Current limitations

- ZIP64, encrypted, multi-disk, and packages over 512 MB are not supported.
- `.modpkg` format versions other than version 1 are not supported.
- Zstandard previews need native browser support or an internet connection to load the fallback decoder.
- Individual previews are limited to 96 MB and ZIP store/deflate compression.
- WAD major versions other than 3 and WAD chunked-Zstandard compression are not supported yet.
- TEX visual previews currently support format IDs 10, 11, 12, and 20.
- No package files are edited or exported yet.

See [`DEVELOPMENT.md`](./DEVELOPMENT.md) for the next development stages and format references.

## Notes

This is an unofficial community project and is not affiliated with or endorsed by Riot Games.
