# Sable

Sable is a Windows desktop workspace for opening League of Legends .fantome and .modpkg mods, including HUDs and champion skins.

## Try the Windows app

1. Open the repository's Actions tab.
2. Open the latest successful **Build Windows executable** run.
3. Download and unzip the **Sable-windows-exe** artifact, then launch sable.exe.

The app uses the WebView2 runtime available on most current Windows installations. This early build is unsigned and distributed as a workflow artifact.

## What works

- Open and browse .fantome and .modpkg packages.
- Browse HUD assets in focused groups such as shop, inventory, scoreboard, abilities, champion UI, and effects.
- Rotate and zoom untextured 3D previews of SKN skin meshes, including SKN files inside WAD archives.
- Group ability and VFX definition BINs separately when browsing WAD contents.
- Resolve many hashed WAD entries to readable paths using references embedded in BIN data, including model and animation names.
- Preview DDS textures alongside League TEX textures.
- Browse package contents by file type and detected paths.
- Unpack a package into a League Toolkit project you can edit in File Explorer.
- Convert TEX textures into PNG previews in a separate Textures folder.
- Repack the workspace into the same package type it came from. Sable creates a new file under Repacked/ and leaves the original download untouched.
- Open the workspace in File Explorer directly from Sable.

Texture PNGs are companions to the original TEX files. Only changed PNGs are converted back during repacking. Sable can re-encode BC1, BC3, BC7, and BGRA8 textures; formats that cannot be safely encoded remain unchanged. Block-compressed textures must have width and height divisible by four when edited.

Sable uses the LeagueToolkit project and texture crates for package import/export and TEX decoding/encoding: [league-mod](https://github.com/LeagueToolkit/league-mod) and [league-toolkit](https://github.com/LeagueToolkit/league-toolkit).

The browser interface remains available from app/index.html for package browsing and preview. Full PNG conversion, File Explorer workspaces, and repacking require the Windows desktop app.

This is an unofficial community project and is not affiliated with or endorsed by Riot Games.
