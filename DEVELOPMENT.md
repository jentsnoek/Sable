# Sable development guide

## Goal

Sable should make it easier to inspect and customize League of Legends HUD and champion-skin mods distributed as `.fantome` and `.modpkg` files. It should only offer edits that are understood and safely supported, and export a new copy without changing the downloaded original.

## Current implementation

- `index.html` is a single-page browser prototype. It opens `.fantome` and `.modpkg` files locally.
- The asset list groups package entries by type and, for layered `.modpkg` files, by target WAD.
- Supported version 3 WAD archives can be indexed. Their internal paths are hashes, so entries are shown by hash when a name cannot be resolved.
- TEX previews support BC1, BC3, and BGRA8. Zstandard decoding uses native browser support or loads the pinned `fzstd` fallback from jsDelivr when needed.
- Editing and export are still disabled. The HUD controls only change the mock preview.

## Next priorities

1. Replace the unused Summoner's Rift screen shown before a mod is opened with a clear empty state.
2. Make the asset browser easier to understand: group assets by target and purpose, and avoid dropping users into a long list of raw files.
3. Improve previews for HUD textures and skin assets. Add editing controls only when the underlying asset format supports them.
4. Export a new mod while preserving metadata, layers, and untouched files.
5. Package the finished Windows app as an executable and installer after the core workflow is stable.

## Working constraints

- Keep mod files local; do not upload them to a service.
- Never overwrite the source package or modify the installed League client.
- Do not bypass anti-cheat or automate game patching.
- Do not redistribute Riot game assets. Use generated placeholders in the UI.
- Show clear messages when an asset is unsupported, unresolved, or read-only.
- Keep the first version focused on helping users understand and edit supported HUD and skin assets.

## Useful references

- [LeagueToolkit package format](https://github.com/LeagueToolkit/docs/blob/main/Mod-Package-Format.md)
- [LeagueToolkit Fantome design notes](https://github.com/LeagueToolkit/league-mod/blob/main/crates/ltk_fantome/DESIGN.md)
- [LeagueToolkit](https://github.com/LeagueToolkit/league-toolkit)

## Run locally

Open `index.html` in a modern browser. Zstandard fallback decoding loads `fzstd` from jsDelivr only when needed; the mod contents remain in the browser.
