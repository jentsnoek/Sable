# Sable

A beginner-friendly editor concept for customizing League of Legends `.fantome` mods, including HUDs and champion skins. The goal is to make supported visual edits understandable without manually digging through a mod's files.

## Current state

`index.html` is an interactive UI prototype, currently showing the HUD editing view. It lets you select HUD elements, hide them in the preview, and adjust preview position, scale, opacity, and accent color. It does **not** read or modify `.fantome` files yet; those controls currently affect only the sample preview. Skin-mod editing is part of the project scope and needs its own asset-aware preview and controls.

## Intended workflow

1. Open a downloaded `.fantome` file.
2. Browse its HUD or champion-skin assets in a visual list.
3. Select an asset and make changes supported by that asset type.
4. Export a new edited mod file, leaving the original untouched.

The editor should only show controls that are supported by the selected asset. A texture can be recolored or replaced; a layout element might support moving or resizing; some files may not expose either safely.

## Run the UI prototype

Open `index.html` in a modern browser. It has no build step or external dependencies.

## Project status

Design prototype only. The `.fantome` reader, HUD and skin asset handling, edit pipeline, and exporter still need implementation. See [`CLAUDE_HANDOFF.md`](./CLAUDE_HANDOFF.md) for the coding brief.

## Notes

This is an unofficial community project and is not affiliated with or endorsed by Riot Games.
