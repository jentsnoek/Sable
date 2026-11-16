# Sable

Sable is a desktop workspace for exploring League of Legends .fantome and .modpkg mods, including HUDs and champion skins.

## Try the Windows app

The first build is a direct Windows executable so you can try Sable without installing a development environment.

1. Open the repository's Actions tab.
2. Select Build Windows executable and open its latest successful run.
3. Download the Sable-windows-exe artifact, unzip it, and launch sable.exe.

The app uses the WebView2 runtime available on most current Windows installations. This early build is unsigned and distributed as a workflow artifact.

## What works today

- Open .fantome and .modpkg files with a native file picker.
- Browse files grouped by HUD and interface, champion, map, audio, metadata, and other assets when their paths contain enough information.
- Preview supported images and League .tex textures.
- Open supported WAD archives and browse their internal entries by detected file type.
- Unpack a working copy to a folder you choose. Package files are preserved and readable WAD entries are organized under wad-contents/.
- Keep the downloaded source package untouched.

Sable is still read-only. The unpacked files are for inspection and manual editing; Sable cannot rebuild edits into a working .fantome or .modpkg yet. WAD entries without a resolved game path keep their hash as the filename. Some compressed, encrypted, or oversized entries are reported as skipped.

The browser version remains available from app/index.html and keeps a browser file picker fallback. When Zstandard is unavailable, Sable can load the pinned fzstd decoder from jsDelivr when needed.

This is an unofficial community project and is not affiliated with or endorsed by Riot Games.
