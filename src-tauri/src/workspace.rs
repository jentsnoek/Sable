use ltk_mod_project::{
    fantome::{FantomeFormat, FantomeImporter},
    modpkg::{ModpkgFormat, ModpkgImporter},
    ProjectImporter, ProjectPacker,
};
use ltk_texture::{
    tex::{EncodeFormat, EncodeOptions, Format, MipmapFilter},
    Tex,
};
use ltk_wad::{Wad, WadHash};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Cursor},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::UNIX_EPOCH,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceResult {
    workspace_path: String,
    display_name: String,
    source_format: String,
    texture_count: usize,
    editable_texture_count: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackResult {
    output_path: String,
    texture_edits: usize,
    read_only_edits_skipped: usize,
    source_format: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceManifest {
    schema_version: u32,
    display_name: String,
    source_format: String,
    project_dir: String,
    texture_manifest: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureManifest {
    textures: Vec<TextureEntry>,
}

const WORKSPACE_MANIFEST_PATH: &str = "Sable data/workspace.json";
const TEXTURE_MANIFEST_PATH: &str = "Sable data/textures.json";
const LEGACY_WORKSPACE_MANIFEST_PATH: &str = "SABLE-workspace.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureEntry {
    project_path: String,
    png_path: String,
    original_png_sha256: String,
    format_id: u8,
    mipmaps: bool,
    editable: bool,
}

#[tauri::command]
pub async fn import_mod_package(
    source_path: String,
    parent_path: String,
) -> Result<WorkspaceResult, String> {
    tauri::async_runtime::spawn_blocking(move || import_mod_package_blocking(source_path, parent_path))
        .await
        .map_err(|error| error.to_string())?
}

fn import_mod_package_blocking(
    source_path: String,
    parent_path: String,
) -> Result<WorkspaceResult, String> {
    let source = PathBuf::from(source_path);
    let parent = PathBuf::from(parent_path);
    if !source.is_file() {
        return Err("The selected mod package could not be found.".to_owned());
    }
    if !parent.is_dir() {
        return Err("Choose an existing folder for the Sable workspace.".to_owned());
    }

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension != "modpkg" && extension != "fantome" {
        return Err("Sable can unpack .modpkg and .fantome packages.".to_owned());
    }

    let source_stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("League mod");
    let workspace = unique_directory(&parent, &format!("{} - Sable workspace", safe_component(source_stem)))?;
    let project_dir = workspace.join("Mod files");
    let source_file = File::open(&source).map_err(|error| format!("Could not open the package: {error}"))?;

    let project = if extension == "modpkg" {
        ProjectImporter::new(project_dir.to_string_lossy().into_owned())
            .import(ModpkgImporter::new(BufReader::new(source_file)))
            .map_err(|error| format!("Could not import this .modpkg: {error}"))?
    } else {
        ProjectImporter::new(project_dir.to_string_lossy().into_owned())
            .import(FantomeImporter::new(BufReader::new(source_file)))
            .map_err(|error| format!("Could not import this .fantome: {error}"))?
    };

    let display_name = if project.display_name.trim().is_empty() {
        if project.name.trim().is_empty() {
            source_stem.to_owned()
        } else {
            project.name.clone()
        }
    } else {
        project.display_name.clone()
    };

    let textures_dir = workspace.join("Edit PNGs");
    fs::create_dir_all(&textures_dir).map_err(|error| error.to_string())?;
    let mut texture_files = Vec::new();
    collect_tex_files(&project_dir, &mut texture_files)
        .map_err(|error| format!("Could not scan imported textures: {error}"))?;
    texture_files.sort();

    let mut textures = Vec::new();
    let mut warnings = Vec::new();
    for tex_path in texture_files {
        let relative = tex_path
            .strip_prefix(&project_dir)
            .map_err(|_| "An imported texture was outside the project folder.".to_owned())?;
        let mut png_relative = relative.to_path_buf();
        png_relative.set_extension("png");
        let png_path = textures_dir.join(&png_relative);

        let file = File::open(&tex_path)
            .map_err(|error| format!("Could not open {}: {error}", relative.display()))?;
        let texture = Tex::from_reader(&mut BufReader::new(file))
            .map_err(|error| format!("Could not read {}: {error}", relative.display()))?;
        let image = texture
            .decode_mipmap(0)
            .map_err(|error| format!("Could not decode {}: {error}", relative.display()))?
            .into_rgba_image()
            .map_err(|error| format!("Could not convert {} to PNG pixels: {error}", relative.display()))?;

        if let Some(parent) = png_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        image
            .save(&png_path)
            .map_err(|error| format!("Could not save PNG for {}: {error}", relative.display()))?;
        let png_bytes = fs::read(&png_path).map_err(|error| error.to_string())?;
        let format_id = texture.format.to_u8();
        let editable = texture.depth == 1 && is_encodable(format_id);
        if !editable {
            warnings.push(format!(
                "{} is previewable, but its texture format is kept read-only for safe repacking.",
                relative.display()
            ));
        }

        textures.push(TextureEntry {
            project_path: relative.to_string_lossy().replace('\\', "/"),
            png_path: png_path
                .strip_prefix(&workspace)
                .map_err(|_| "A generated PNG was outside the workspace.".to_owned())?
                .to_string_lossy()
                .replace('\\', "/"),
            original_png_sha256: sha256(&png_bytes),
            format_id,
            mipmaps: texture.has_mipmaps(),
            editable,
        });
    }

    let manifest = WorkspaceManifest {
        schema_version: 1,
        display_name: display_name.clone(),
        source_format: extension.clone(),
        project_dir: "Mod files".to_owned(),
        texture_manifest: TEXTURE_MANIFEST_PATH.to_owned(),
    };
    fs::create_dir_all(workspace.join("Sable data")).map_err(|error| error.to_string())?;
    fs::write(
        workspace.join(WORKSPACE_MANIFEST_PATH),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        workspace.join(TEXTURE_MANIFEST_PATH),
        serde_json::to_vec_pretty(&TextureManifest { textures: textures.clone() })
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let readme = format!(
        "SABLE WORKSPACE\n\nSource package: {} (.{})\n\nEDIT THESE PNGS\n  Edit PNGs\\ contains PNG versions of supported TEX textures. Edit these in your image editor.\n\nMOD FILES\n  Mod files\\ contains the extracted package assets used by Sable. Keep this folder and its paths intact.\n\nSABLE DATA\n  Sable data\\ contains Sable's internal workspace and texture maps. Leave these files alone.\n\nWHEN YOU ARE DONE\n  Save your PNG edits, open Sable, then click Repack. Sable creates a separate package copy and leaves your download unchanged. Some texture formats remain read-only for safe repacking.\n",
        source.file_name().and_then(|name| name.to_str()).unwrap_or("mod package"),
        extension
    );
    fs::write(workspace.join("START HERE.txt"), readme).map_err(|error| error.to_string())?;

    Ok(WorkspaceResult {
        workspace_path: workspace.to_string_lossy().into_owned(),
        display_name,
        source_format: extension,
        texture_count: textures.len(),
        editable_texture_count: textures.iter().filter(|entry| entry.editable).count(),
        warnings,
    })
}



#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TexturePreviewResult {
    png_bytes: Vec<u8>,
    texture_path: String,
}

static PREVIEW_IMPORT_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
pub async fn preview_package_texture(
    source_path: String,
    entry_path: String,
) -> Result<TexturePreviewResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        preview_package_texture_blocking(source_path, entry_path)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn preview_package_texture_blocking(
    source_path: String,
    entry_path: String,
) -> Result<TexturePreviewResult, String> {
    let workspace = ensure_preview_workspace(&source_path)?;
    let workspace_manifest = read_workspace_manifest(&workspace)?;
    let texture_map_path = safe_child(&workspace, &workspace_manifest.texture_manifest)?;
    let manifest: TextureManifest = serde_json::from_slice(
        &fs::read(texture_map_path)
            .map_err(|error| format!("Could not read the texture preview map: {error}"))?,
    )
    .map_err(|error| format!("The texture preview map is invalid: {error}"))?;

    let requested = normalize_package_path(&entry_path);
    let matching: Vec<&TextureEntry> = manifest
        .textures
        .iter()
        .filter(|texture| {
            let path = normalize_package_path(&texture.project_path);
            path == requested
                || path.ends_with(&format!("/{requested}"))
                || requested.ends_with(&format!("/{path}"))
        })
        .collect();
    if matching.len() == 1 {
        let texture = matching[0];
        let png_path = safe_child(&workspace, &texture.png_path)?;
        let png_bytes = fs::read(&png_path)
            .map_err(|error| format!("Could not read the generated TEX preview: {error}"))?;
        return Ok(TexturePreviewResult {
            png_bytes,
            texture_path: texture.project_path.clone(),
        });
    }
    if matching.len() > 1 {
        return Err(format!("More than one TEX file matches this path: {entry_path}"));
    }

    preview_wad_texture(&workspace, &requested, &entry_path)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WadTextureEntry {
    hash: String,
    size: usize,
}

#[tauri::command]
pub async fn inspect_package_wad_textures(
    source_path: String,
    wad_path: String,
) -> Result<Vec<WadTextureEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspect_package_wad_textures_blocking(source_path, wad_path)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn inspect_package_wad_textures_blocking(
    source_path: String,
    wad_path: String,
) -> Result<Vec<WadTextureEntry>, String> {
    let workspace = ensure_preview_workspace(&source_path)?;
    let workspace_manifest = read_workspace_manifest(&workspace)?;
    let project_dir = safe_child(&workspace, &workspace_manifest.project_dir)?;
    let wad_file = find_wad_file(&project_dir, &wad_path)?;
    let mut wad = Wad::mount(
        File::open(&wad_file).map_err(|error| format!("Could not open WAD archive: {error}"))?,
    )
    .map_err(|error| format!("Could not read WAD archive: {error}"))?;

    let chunks: Vec<_> = wad.chunks().iter().copied().collect();
    let mut textures = Vec::new();
    for chunk in chunks {
        if chunk.uncompressed_size() > 256 * 1024 * 1024 {
            continue;
        }
        if let Ok(bytes) = wad.load_chunk_decompressed(&chunk) {
            if bytes.starts_with(b"TEX\0") {
                textures.push(WadTextureEntry {
                    hash: format!("{:016x}", chunk.path_hash()),
                    size: bytes.len(),
                });
            }
        }
    }
    Ok(textures)
}

fn ensure_preview_workspace(source_path: &str) -> Result<PathBuf, String> {
    let _guard = PREVIEW_IMPORT_LOCK
        .lock()
        .map_err(|_| "The texture preview cache is unavailable.".to_owned())?;
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err("The selected mod package could not be found.".to_owned());
    }

    let metadata = fs::metadata(&source).map_err(|error| error.to_string())?;
    let canonical = source.canonicalize().map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let cache_key = sha256(
        format!("{}:{}:{}", canonical.to_string_lossy(), metadata.len(), modified).as_bytes(),
    );
    let cache_parent = std::env::temp_dir()
        .join("Sable")
        .join("TexturePreviewCache")
        .join(cache_key);
    fs::create_dir_all(&cache_parent).map_err(|error| error.to_string())?;

    let source_stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("League mod");
    let expected_workspace =
        cache_parent.join(format!("{} - Sable workspace", safe_component(source_stem)));
    if workspace_manifest_path(&expected_workspace).is_file() {
        return Ok(expected_workspace);
    }
    let result = import_mod_package_blocking(
        source.to_string_lossy().into_owned(),
        cache_parent.to_string_lossy().into_owned(),
    )?;
    Ok(PathBuf::from(result.workspace_path))
}

fn normalize_package_path(path: &str) -> String {
    path.replace('\\', "/")
        .replace(" / ", "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

fn find_wad_file(project_dir: &Path, requested_path: &str) -> Result<PathBuf, String> {
    let relative = normalize_package_path(requested_path);
    if let Ok(candidate) = safe_child(project_dir, &relative) {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let basename = Path::new(&relative)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("Invalid WAD path: {requested_path}"))?;
    let mut matches = Vec::new();
    collect_files_named(project_dir, basename, &mut matches)
        .map_err(|error| format!("Could not search imported WAD files: {error}"))?;
    match matches.len() {
        0 => Err(format!("Could not find WAD archive {requested_path} in the imported project.")),
        1 => Ok(matches.remove(0)),
        _ => {
            let suffix = format!("/{}", relative);
            if let Some(exact) = matches
                .iter()
                .find(|path| path.to_string_lossy().replace('\\', "/").to_ascii_lowercase().ends_with(&suffix))
            {
                Ok(exact.clone())
            } else {
                Err(format!("More than one imported WAD matches {requested_path}."))
            }
        }
    }
}

fn collect_files_named(root: &Path, filename: &str, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files_named(&path, filename, found)?;
        } else if file_type.is_file()
            && entry.file_name().to_string_lossy().eq_ignore_ascii_case(filename)
        {
            found.push(path);
        }
    }
    Ok(())
}

fn preview_wad_texture(
    workspace: &Path,
    requested: &str,
    display_path: &str,
) -> Result<TexturePreviewResult, String> {
    let (wad_relative, texture_name) = requested
        .rsplit_once('/')
        .ok_or_else(|| format!("Sable could not locate this TEX texture: {display_path}"))?;
    let hash_name = texture_name.strip_suffix(".tex").unwrap_or(texture_name);
    let hash_text = hash_name.strip_prefix("0x").unwrap_or(hash_name);
    let hash: WadHash = hash_text
        .parse()
        .map_err(|_| format!("This WAD texture has an invalid hash: {display_path}"))?;
    let workspace_manifest = read_workspace_manifest(workspace)?;
    let project_dir = safe_child(workspace, &workspace_manifest.project_dir)?;
    let wad_file = find_wad_file(&project_dir, wad_relative)?;
    let mut wad = Wad::mount(
        File::open(&wad_file).map_err(|error| format!("Could not open WAD archive: {error}"))?,
    )
    .map_err(|error| format!("Could not read WAD archive: {error}"))?;
    let chunk = wad
        .chunks()
        .get(hash)
        .copied()
        .ok_or_else(|| format!("The WAD no longer contains this texture: {display_path}"))?;
    let tex_bytes = wad
        .load_chunk_decompressed(&chunk)
        .map_err(|error| format!("Could not decompress this WAD texture: {error}"))?;
    if !tex_bytes.starts_with(b"TEX\0") {
        return Err(format!("This WAD entry is not a TEX texture: {display_path}"));
    }

    let mut reader = BufReader::new(Cursor::new(tex_bytes.as_ref()));
    let texture = Tex::from_reader(&mut reader)
        .map_err(|error| format!("Could not read WAD texture: {error}"))?;
    let rgba = texture
        .decode_mipmap(0)
        .map_err(|error| format!("Could not decode WAD texture: {error}"))?
        .into_rgba_image()
        .map_err(|error| format!("Could not convert WAD texture: {error}"))?;
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| format!("Could not encode WAD texture preview: {error}"))?;

    Ok(TexturePreviewResult {
        png_bytes: output.into_inner(),
        texture_path: display_path.to_owned(),
    })
}

#[tauri::command]
pub async fn pack_mod_workspace(workspace_path: String) -> Result<PackResult, String> {
    tauri::async_runtime::spawn_blocking(move || pack_mod_workspace_blocking(workspace_path))
        .await
        .map_err(|error| error.to_string())?
}

fn pack_mod_workspace_blocking(workspace_path: String) -> Result<PackResult, String> {
    let workspace = PathBuf::from(workspace_path);
    let manifest = read_workspace_manifest(&workspace)?;
    if manifest.schema_version != 1 {
        return Err("This workspace was made by a newer Sable version.".to_owned());
    }
    if manifest.source_format != "modpkg" && manifest.source_format != "fantome" {
        return Err("The workspace does not have a supported package format.".to_owned());
    }

    let project_dir = safe_child(&workspace, &manifest.project_dir)?;
    let texture_manifest_path = safe_child(&workspace, &manifest.texture_manifest)?;
    let texture_manifest: TextureManifest = serde_json::from_slice(
        &fs::read(texture_manifest_path).map_err(|error| format!("Could not read the texture map: {error}"))?,
    )
    .map_err(|error| format!("The texture map is invalid: {error}"))?;

    let mut texture_edits = 0;
    let mut read_only_edits_skipped = 0;
    for entry in texture_manifest.textures {
        let png_path = safe_child(&workspace, &entry.png_path)?;
        let tex_path = safe_child(&project_dir, &entry.project_path)?;
        if !png_path.is_file() {
            continue;
        }
        let png_bytes = fs::read(&png_path)
            .map_err(|error| format!("Could not read {}: {error}", png_path.display()))?;
        if sha256(&png_bytes) == entry.original_png_sha256 {
            continue;
        }
        if !entry.editable || !is_encodable(entry.format_id) {
            read_only_edits_skipped += 1;
            continue;
        }

        let format = encode_format(entry.format_id)
            .ok_or_else(|| format!("Sable cannot encode texture format {}.", entry.format_id))?;
        let mut options = EncodeOptions::new(format).with_mipmap_filter(MipmapFilter::Lanczos3);
        if entry.mipmaps {
            options = options.with_mipmaps();
        }
        let image = image::open(&png_path)
            .map_err(|error| format!("Could not decode edited PNG {}: {error}", png_path.display()))?;
        if matches!(entry.format_id, 10 | 11 | 12 | 13)
            && (image.width() % 4 != 0 || image.height() % 4 != 0)
        {
            return Err(format!(
                "{} must keep width and height divisible by 4 for game-ready block-compressed TEX output.",
                png_path.display()
            ));
        }
        let texture = Tex::encode_dynamic_image(image, options)
            .map_err(|error| format!("Could not encode {} back to TEX: {error}", png_path.display()))?;
        if let Some(parent) = tex_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let output = File::create(&tex_path)
            .map_err(|error| format!("Could not replace {}: {error}", tex_path.display()))?;
        texture
            .write(&mut BufWriter::new(output))
            .map_err(|error| format!("Could not write {}: {error}", tex_path.display()))?;
        texture_edits += 1;
    }

    let packer = ProjectPacker::from_dir(project_dir.to_string_lossy().into_owned())
        .map_err(|error| format!("Could not load the edited mod project: {error}"))?;
    let output_dir = workspace.join("Repacked");
    fs::create_dir_all(&output_dir).map_err(|error| format!("Could not create Repacked/: {error}"))?;
    let base_name = safe_component(&manifest.display_name);
    let extension = manifest.source_format.as_str();
    let output_path = unique_file(&output_dir, &format!("{base_name} - Edited"), extension)?;
    let output_file = File::create(&output_path).map_err(|error| format!("Could not create the package: {error}"))?;

    if manifest.source_format == "modpkg" {
        packer
            .pack(ModpkgFormat::new(output_file))
            .map_err(|error| format!("Could not rebuild the .modpkg: {error}"))?;
    } else {
        packer
            .pack(FantomeFormat::new(output_file))
            .map_err(|error| format!("Could not rebuild the .fantome: {error}"))?;
    }

    Ok(PackResult {
        output_path: output_path.to_string_lossy().into_owned(),
        texture_edits,
        read_only_edits_skipped,
        source_format: manifest.source_format,
    })
}

#[tauri::command]
pub async fn open_sable_workspace(workspace_path: String) -> Result<(), String> {
    let path = PathBuf::from(workspace_path);
    if !workspace_manifest_path(&path).is_file() {
        return Err("Sable can only open folders created by Unpack.".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            Command::new("explorer.exe")
                .arg(&path)
                .spawn()
                .map_err(|error| format!("Could not open File Explorer: {error}"))?;
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = path;
            Err("Opening the workspace folder is currently available on Windows.".to_owned())
        }
    })
    .await
    .map_err(|error| error.to_string())?
}


fn workspace_manifest_path(workspace: &Path) -> PathBuf {
    let current = workspace.join(WORKSPACE_MANIFEST_PATH);
    if current.is_file() {
        current
    } else {
        workspace.join(LEGACY_WORKSPACE_MANIFEST_PATH)
    }
}

fn read_workspace_manifest(workspace: &Path) -> Result<WorkspaceManifest, String> {
    let manifest_path = workspace_manifest_path(workspace);
    if !manifest_path.is_file() {
        return Err("This folder is not a Sable workspace. Open the folder created by Unpack.".to_owned());
    }
    serde_json::from_slice(
        &fs::read(&manifest_path).map_err(|error| format!("Could not read the workspace file: {error}"))?,
    )
    .map_err(|error| format!("The workspace settings are invalid: {error}"))
}

fn is_encodable(format_id: u8) -> bool {
    matches!(format_id, 10 | 11 | 12 | 13 | 20)
}

fn encode_format(format_id: u8) -> Option<EncodeFormat> {
    match Format::from_u8(format_id).ok()? {
        Format::Bc1 => Some(EncodeFormat::Bc1 { weigh_colour_by_alpha: true }),
        Format::Bc3 => Some(EncodeFormat::Bc3 { weigh_colour_by_alpha: true }),
        Format::Bc7 => Some(EncodeFormat::Bc7),
        Format::Bgra8 => Some(EncodeFormat::Bgra8),
        _ => None,
    }
}

fn collect_tex_files(root: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_tex_files(&path, found)?;
        } else if file_type.is_file()
            && path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("tex"))
        {
            found.push(path);
        }
    }
    Ok(())
}

fn safe_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "League Mod".to_owned()
    } else {
        trimmed.chars().take(90).collect()
    }
}

fn unique_directory(parent: &Path, base: &str) -> Result<PathBuf, String> {
    for index in 1..=999 {
        let name = if index == 1 { base.to_owned() } else { format!("{base} ({index})") };
        let candidate = parent.join(name);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not create the workspace folder: {error}")),
        }
    }
    Err("Could not find a free folder name.".to_owned())
}

fn unique_file(parent: &Path, base: &str, extension: &str) -> Result<PathBuf, String> {
    for index in 1..=999 {
        let suffix = if index == 1 { String::new() } else { format!(" ({index})") };
        let candidate = parent.join(format!("{base}{suffix}.{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Could not find a free output filename.".to_owned())
}

fn safe_child(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty() || path.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err("The workspace contains an unsafe file path.".to_owned());
    }
    Ok(root.join(path))
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
