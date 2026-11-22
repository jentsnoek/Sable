# Sable

Sable is a Windows desktop workspace for opening League of Legends .fantome and .modpkg mods, including HUDs and champion skins.

## Try the Windows app

Download the repository ZIP from **Code → Download ZIP**, extract it, and launch `Sable-windows.exe` from the extracted folder. The same executable is also available as the **Sable-windows-exe** artifact on the latest successful run in the Actions tab.

The app uses the WebView2 runtime available on most current Windows installations. This early build is unsigned and distributed as a workflow artifact.

## What works

- Open and browse .fantome and .modpkg packages.
- Browse HUD assets in focused groups such as shop, inventory, scoreboard, abilities, champion UI, and effects.
- Filter package and WAD assets by model, texture, skeleton, animation, VFX, or other file type.
- Rotate and zoom SKN skin meshes in 3D, including models inside WAD archives. When Sable finds a matching diffuse texture, it applies it automatically and reports which texture, skeleton, and renderer it used.
- Group WAD entries by asset role and path owner, with cross-champion references separated as linked assets.
- Group ability and VFX definition BINs separately when browsing WAD contents.
- Resolve many hashed WAD entries to readable paths using references embedded in BIN data, including model and animation names.
- Show WAD classification progress and reuse recent classification results during the app session.
- Preview DDS textures alongside League TEX textures.
- Browse package contents by file type and detected paths.
- Unpack a package into a League Toolkit project you can edit in File Explorer.
- Convert TEX textures into PNG previews in a separate Textures folder.
- Repack the workspace into the same package type it came from. Sable creates a new file under Repacked/ and leaves the original download untouched.
- Open the workspace in File Explorer directly from Sable.

Texture PNGs are companions to the original TEX files. Only changed PNGs are converted back during repacking. Sable can re-encode BC1, BC3, BC7, and BGRA8 textures; formats that cannot be safely encoded remain unchanged. Block-compressed textures must have width and height divisible by four when edited.

Sable uses the LeagueToolkit project and texture crates for package import/export and TEX decoding/encoding: [league-mod](https://github.com/LeagueToolkit/league-mod) and [league-toolkit](https://github.com/LeagueToolkit/league-toolkit).

The browser interface remains available from app/index.html for package browsing and preview. Full PNG conversion, File Explorer workspaces, and repacking require the Windows desktop app.

WADs can contain opaque entries or hashed entries with no path references; those remain grouped under their detected type or as other WAD entries. Skin texture and skeleton detection depends on the assets included in the WAD.

This is an unofficial community project and is not affiliated with or endorsed by Riot Games.
