# Sable

A beginner-friendly editor concept for customizing League of Legends `.fantome` mods, including HUDs and champion skins. The goal is to make supported visual edits understandable without manually digging through a mod's files.

## Current state

`index.html` opens legacy ZIP-based `.fantome` packages and modern League Toolkit `.modpkg` packages locally. It reads package metadata, lists and searches entries, and previews supported PNG, JPEG, WebP, GIF, and BMP images when their contents can be decoded by the browser. The original package is never uploaded or changed.

This is the first read-only file-inspection step. It does not unpack `.wad.client` archives or edit HUD/skin assets yet. `.modpkg` entries compressed with Zstandard can be listed, but browsers such as Chrome do not currently expose a built-in Zstandard decoder for previews. The HUD controls still affect only the sample preview, and export is intentionally disabled until real file edits can be preserved safely.

## Intended workflow

1. Open a downloaded `.fantome` file.
2. Browse its HUD or champion-skin assets in a visual list.
3. Select an asset and make changes supported by that asset type.
4. Export a new edited mod file, leaving the original untouched.

The editor should only show controls that are supported by the selected asset. A texture can be recolored or replaced; a layout element might support moving or resizing; some files may not expose either safely.

## Run Sable

Open `index.html` in a modern browser. It has no build step or external dependencies.

## Current limitations

- ZIP64, encrypted, multi-disk, and packages over 512 MB are not supported.
- `.modpkg` format versions other than version 1 are not supported.
- Zstandard-compressed `.modpkg` entries are listed but may not preview in the browser.
- Individual previews are limited to 96 MB and ZIP store/deflate compression.
- WAD archives are listed but not unpacked.
- No package files are edited or exported yet.

See [`CLAUDE_HANDOFF.md`](./CLAUDE_HANDOFF.md) for the next development stages and format references.

## Notes

This is an unofficial community project and is not affiliated with or endorsed by Riot Games.
