# Claude implementation handoff

Paste the prompt below into Claude with this project folder or repository attached.

---

## Prompt

Build the real file-processing side of **Sable**, a beginner-friendly Windows tool for making supported edits to League of Legends HUD and champion-skin mods distributed as `.fantome` files. The existing `index.html` is the UI direction. Keep its dark plum palette, three-panel layout, left-side asset list, central preview, and right-side inspector. Improve the code structure as needed, but preserve the simple workflow.

### Product goal

A user downloads a HUD or champion-skin mod, opens its `.fantome` file, makes a supported visual adjustment, previews it, then exports a **new** mod file. Never overwrite the downloaded original. Be honest in the UI when a particular asset cannot be edited.

### Current implementation

The single-file browser app already opens ordinary ZIP-based `.fantome` packages locally, displays `META/info.json` metadata, lists and searches outer package entries, and previews common image files. It deliberately keeps editing and export disabled. It does not unpack WAD files, which are often the actual containers for HUD and champion-skin assets.

Continue in small phases:

1. **Inspect real samples:** use user-provided HUD and skin `.fantome` files. Do not assume every package has the same layout. A `.fantome` commonly has `META/info.json` and `WAD/` or `RAW/` content; WAD may itself be packed or represented as loose files. Identify real asset types before offering edits.
2. **Read-only WAD support:** use a maintained LeagueToolkit reader to inspect nested WAD entries and preview supported textures. Handle malformed or unsupported files with clear errors.
3. **Safe basic edits:** start with edits that can be reliably round-tripped for the supplied HUD and skin samples. Preserve untouched entries byte-for-byte where practical. Do not silently drop entries or layers.
4. **Preview controls:** connect inspector controls only to real supported properties. Do not present mock controls as successful file edits.
5. **Export:** create a new `.fantome`, preserve metadata and unknown entries, and confirm the output can be reopened. Keep source and output separate.

Before choosing language or libraries, inspect what is installed and research maintained LeagueToolkit format libraries. Prefer a well-maintained parser/writer over inventing the WAD format. The official LeagueToolkit `league-mod` / `ltk_fantome` source is a strong reference for package structure and read/write behavior. Keep the app local-first; do not upload mod files or request Riot credentials.

### UI behavior to implement

- “Open .fantome” selects a local file and updates the project title.
- The left panel lets the user browse the mod's HUD and champion-skin assets, grouped by type.
- The middle canvas previews HUD assets over a neutral mock game scene and skin assets in a suitable texture/model preview.
- The right panel shows only edits that are actually supported for the selected element.
- Position, scale, opacity, color, and visibility may remain disabled until the underlying asset type supports each operation.
- “Create edited copy” stays disabled until there is at least one valid edit; it exports to a new file and confirms the destination.
- Include undo/reset for preview changes and useful error messages for unsupported assets.

### Constraints

- Do not inject code into League, bypass anti-cheat, alter Riot's installed files, or automate game patching. The tool only edits a user-selected mod package and exports a separate file for an existing mod manager.
- Do not bundle or redistribute Riot game assets. Use generated placeholders in the UI.
- Don't claim a HUD element can be moved or resized until you have identified and validated the corresponding format in a real mod.
- Keep the first version focused. Don't build a full general-purpose League mod manager or IDE.
- Add a README with setup, supported file types, limitations, and screenshots. Keep source code readable and document format assumptions.

### Reference material

- LeagueToolkit mod package docs: https://github.com/LeagueToolkit/docs/blob/main/Mod-Package-Format.md
- LeagueToolkit `league-mod` Fantome design notes: https://github.com/LeagueToolkit/league-mod/blob/main/crates/ltk_fantome/DESIGN.md
- LTK Manager (existing import/editor workflow): https://github.com/LeagueToolkit/ltk-manager
- ReyEngine (existing `.fantome` import and HUD inspection): https://github.com/TheKillerey/ReyEngine

The differentiator is a much simpler, focused HUD editing flow. Existing tools already import `.fantome` packages and support advanced asset editing, so do not duplicate their general-purpose mod manager features.

---

## Notes for the project owner

I can't send this to Claude directly from this chat. After attaching the project folder or repo, paste the prompt above and include sample `.fantome` files for a HUD mod and a champion-skin mod you have permission to modify. Make copies first; file-format behavior can vary by mod.
