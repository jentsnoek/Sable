use ltk_mod_project::{
    fantome::{FantomeFormat, FantomeImporter},
    modpkg::{ModpkgFormat, ModpkgImporter},
    ProjectImporter, ProjectPacker,
};
use ltk_anim::{Animation, AnimationAsset, RigResource};
use ltk_hash::{BinHash, Hash as _};
use ltk_meta::Bin as PropertyBin;
use ltk_texture::{
    tex::{EncodeFormat, EncodeOptions, Format, MipmapFilter},
    Tex,
};
use ltk_wad::{Wad, WadHash};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
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
    let project_dir = safe_child(&workspace, &workspace_manifest.project_dir)?;
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

    if requested.ends_with(".dds") {
        if let Ok(dds_file) = find_project_file(&project_dir, &entry_path) {
            let bytes = fs::read(&dds_file).map_err(|error| format!("Could not read DDS texture: {error}"))?;
            if bytes.starts_with(b"DDS ") {
                return decode_dds_preview(&bytes, &entry_path);
            }
        }
    }

    preview_wad_texture(&workspace, &requested, &entry_path)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WadAssetEntry {
    hash: String,
    path: Option<String>,
    size: usize,
    asset_type: String,
    extension: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreviewBundle {
    model_bytes: Vec<u8>,
    skeleton: Option<SkeletonPreview>,
    skeleton_path: Option<String>,
    animations: Vec<AnimationPreviewInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonPreview {
    name: String,
    asset_name: String,
    influences: Vec<i16>,
    joints: Vec<SkeletonJointPreview>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkeletonJointPreview {
    name: String,
    hash: u32,
    id: i16,
    parent_id: i16,
    local_translation: [f32; 3],
    local_rotation: [f32; 4],
    local_scale: [f32; 3],
    inverse_bind_matrix: [f32; 16],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationPreviewInfo {
    path: String,
    name: String,
    duration: f32,
    fps: f32,
    joint_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationPreview {
    name: String,
    duration: f32,
    fps: f32,
    sample_rate: f32,
    frames: Vec<Vec<AnimationJointSample>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationJointSample {
    joint: usize,
    rotation: [f32; 4],
    translation: [f32; 3],
    scale: [f32; 3],
}

#[tauri::command]
pub async fn inspect_package_wad_assets(
    source_path: String,
    wad_path: String,
) -> Result<Vec<WadAssetEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspect_package_wad_assets_blocking(source_path, wad_path)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn inspect_package_wad_assets_blocking(
    source_path: String,
    wad_path: String,
) -> Result<Vec<WadAssetEntry>, String> {
    let workspace = ensure_preview_workspace(&source_path)?;
    let workspace_manifest = read_workspace_manifest(&workspace)?;
    let project_dir = safe_child(&workspace, &workspace_manifest.project_dir)?;
    let wad_file = find_project_file(&project_dir, &wad_path)?;
    let mut wad = Wad::mount(
        File::open(&wad_file).map_err(|error| format!("Could not open WAD archive: {error}"))?,
    )
    .map_err(|error| format!("Could not read WAD archive: {error}"))?;

    let chunks: Vec<_> = wad.chunks().iter().copied().collect();
    let mut assets = Vec::new();
    let mut resolved_paths = HashMap::new();
    for chunk in chunks {
        if chunk.uncompressed_size() > 256 * 1024 * 1024 {
            continue;
        }
        if let Ok(bytes) = wad.load_chunk_decompressed(&chunk) {
            if bytes.starts_with(b"PROP") || bytes.starts_with(b"PTCH") {
                for path in extract_wad_path_references(&bytes) {
                    let referenced_hash = WadHash::hash_str(&path);
                    if wad.chunks().contains(referenced_hash) {
                        resolved_paths.entry(format!("{referenced_hash:016x}")).or_insert(path);
                    }
                }
            }
            let kind = classify_wad_asset(&bytes);
            if let Some((asset_type, extension)) = kind {
                let hash = format!("{:016x}", chunk.path_hash());
                assets.push(WadAssetEntry {
                    path: resolved_paths.get(&hash).cloned(),
                    hash,
                    size: bytes.len(),
                    asset_type: asset_type.to_owned(),
                    extension: extension.to_owned(),
                });
            }
        }
    }
    for asset in &mut assets {
        if asset.path.is_none() {
            asset.path = resolved_paths.get(&asset.hash).cloned();
        }
    }
    Ok(assets)
}

fn extract_wad_path_references(bytes: &[u8]) -> Vec<String> {
    const PATH_PREFIXES: [&str; 7] = [
        "assets/", "data/", "characters/", "maps/", "map/", "sounds/", "skins/",
    ];
    const EXTENSIONS: [&str; 14] = [
        ".wad.client", ".troybin", ".skn", ".skl", ".anm", ".tex", ".dds", ".scb",
        ".sco", ".bin", ".bnk", ".wpk", ".wem", ".png",
    ];
    let is_path_byte = |byte: u8| byte.is_ascii_alphanumeric() || b"_./\\-".contains(&byte);
    let mut references = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut offset = 0;

    while offset < bytes.len() {
        while offset < bytes.len() && !is_path_byte(bytes[offset]) { offset += 1; }
        let start = offset;
        while offset < bytes.len() && is_path_byte(bytes[offset]) { offset += 1; }
        if start == offset { continue; }
        let Ok(token) = std::str::from_utf8(&bytes[start..offset]) else { continue; };
        let lower = token.to_ascii_lowercase();
        let Some(path_start) = PATH_PREFIXES.iter().filter_map(|prefix| lower.find(prefix)).min() else { continue; };
        let candidate = &token[path_start..];
        let candidate_lower = candidate.to_ascii_lowercase();
        let end = EXTENSIONS.iter()
            .filter_map(|extension| candidate_lower.find(extension).map(|index| index + extension.len()))
            .min();
        let Some(end) = end else { continue; };
        let path = candidate[..end].replace('\\', "/");
        if path.matches('/').count() < 2 || !seen.insert(path.to_ascii_lowercase()) { continue; }
        references.push(path);
    }
    references
}

fn classify_wad_asset(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"TEX\0") { return Some(("Textures", "tex")); }
    if bytes.starts_with(b"DDS ") { return Some(("Textures", "dds")); }
    if bytes.starts_with(&[0x33, 0x22, 0x11, 0x00]) { return Some(("Models", "skn")); }
    if bytes.len() >= 8 && u32::from_le_bytes(bytes[4..8].try_into().ok()?) == 0x22FD4FC3 {
        return Some(("Skeletons", "skl"));
    }
    if bytes.starts_with(b"r3d2canm") || bytes.starts_with(b"r3d2anmd") {
        return Some(("Animations", "anm"));
    }
    if bytes.starts_with(b"SCB\0") { return Some(("Static models", "scb")); }
    if bytes.starts_with(b"SCO\0") { return Some(("Static models", "sco")); }
    if bytes.starts_with(b"PROP") {
        if is_ability_property_bin(bytes) { return Some(("Abilities & VFX", "bin")); }
        return Some(("Game data", "bin"));
    }
    if bytes.starts_with(b"PTCH") { return Some(("Game data", "bin")); }
    if bytes.starts_with(b"BKHD") { return Some(("Audio", "bnk")); }
    if bytes.starts_with(b"RIFF") { return Some(("Audio", "wem")); }
    if bytes.starts_with(b"WPK\0") { return Some(("Audio", "wpk")); }
    if bytes.starts_with(b"RW") { return Some(("WAD archives", "wad")); }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { return Some(("Images", "png")); }
    None
}

fn is_ability_property_bin(bytes: &[u8]) -> bool {
    let mut cursor = Cursor::new(bytes);
    let Ok(bin) = PropertyBin::from_reader(&mut cursor) else { return false; };
    let ability_classes = [
        BinHash::from("SpellDataResource").0,
        BinHash::from("SpellObjectDefinitionData").0,
        BinHash::from("VfxSystemDefinitionData").0,
    ];
    bin.objects.values().any(|object| ability_classes.contains(&object.class_hash.0))
}

#[tauri::command]
pub async fn read_package_model(
    source_path: String,
    entry_path: String,
) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !entry_path.to_ascii_lowercase().ends_with(".skn") {
            return Err("Choose an SKN model to preview.".to_owned());
        }
        let workspace = ensure_preview_workspace(&source_path)?;
        let manifest = read_workspace_manifest(&workspace)?;
        let project_dir = safe_child(&workspace, &manifest.project_dir)?;
        let model_file = find_project_file(&project_dir, &entry_path)?;
        if fs::metadata(&model_file).map_err(|error| error.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("This model is over the 32 MB preview limit.".to_owned());
        }
        let bytes = fs::read(&model_file).map_err(|error| format!("Could not read SKN model: {error}"))?;
        if !bytes.starts_with(&[0x33, 0x22, 0x11, 0x00]) {
            return Err("This file is not a League SKN model.".to_owned());
        }
        Ok(bytes)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_package_model_preview_bundle(
    source_path: String,
    entry_path: String,
) -> Result<ModelPreviewBundle, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !entry_path.to_ascii_lowercase().ends_with(".skn") {
            return Err("Choose an SKN model to preview.".to_owned());
        }
        let workspace = ensure_preview_workspace(&source_path)?;
        let manifest = read_workspace_manifest(&workspace)?;
        let project_dir = safe_child(&workspace, &manifest.project_dir)?;
        let model_file = find_project_file(&project_dir, &entry_path)?;
        if fs::metadata(&model_file).map_err(|error| error.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("This model is over the 32 MB preview limit.".to_owned());
        }
        let model_bytes = fs::read(&model_file).map_err(|error| format!("Could not read SKN model: {error}"))?;
        if !model_bytes.starts_with(&[0x33, 0x22, 0x11, 0x00]) {
            return Err("This file is not a League SKN model.".to_owned());
        }

        let model_relative = model_file.strip_prefix(&project_dir).map_err(|error| error.to_string())?;
        let mut skeleton_file = model_relative.with_extension("skl");
        let mut skeleton_path = project_dir.join(&skeleton_file);
        if !skeleton_path.is_file() {
            let parent = model_file.parent().unwrap_or(&project_dir);
            let mut siblings = Vec::new();
            collect_files_with_extension(parent, "skl", &mut siblings)
                .map_err(|error| format!("Could not search for the model skeleton: {error}"))?;
            skeleton_file = if siblings.len() == 1 {
                siblings.remove(0).strip_prefix(&project_dir).map_err(|error| error.to_string())?.to_path_buf()
            } else {
                PathBuf::new()
            };
            skeleton_path = if skeleton_file.as_os_str().is_empty() { PathBuf::new() } else { project_dir.join(&skeleton_file) };
        }

        let (skeleton, skeleton_path_text) = if skeleton_path.is_file() {
            let skeleton_bytes = fs::read(&skeleton_path).map_err(|error| format!("Could not read model skeleton: {error}"))?;
            let skeleton = parse_skeleton_preview(&skeleton_bytes)?;
            let relative = skeleton_path.strip_prefix(&project_dir).map_err(|error| error.to_string())?;
            (Some(skeleton), Some(relative.to_string_lossy().replace('\\', "/")))
        } else {
            (None, None)
        };

        let mut animation_files = Vec::new();
        collect_files_with_extension(&project_dir, "anm", &mut animation_files)
            .map_err(|error| format!("Could not search for model animations: {error}"))?;
        let scope = model_animation_scope(&entry_path);
        animation_files.retain(|path| {
            let relative = path.strip_prefix(&project_dir).unwrap_or(path).to_string_lossy().replace('\\', "/").to_ascii_lowercase();
            if let Some(scope) = &scope { relative.starts_with(scope) } else { path.parent() == model_file.parent() }
        });
        animation_files.sort();
        animation_files.truncate(128);
        let mut animations = Vec::new();
        for path in animation_files {
            let Ok(metadata) = fs::metadata(&path) else { continue };
            if metadata.len() > 16 * 1024 * 1024 { continue; }
            let Ok(bytes) = fs::read(&path) else { continue };
            let Ok(asset) = AnimationAsset::from_reader(&mut Cursor::new(bytes.as_slice())) else { continue };
            if !asset.duration().is_finite() || asset.duration() <= 0.0 || asset.duration() > 120.0 {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&project_dir) else { continue };
            let relative_path = relative.to_string_lossy().replace('\\', "/");
            let name = path.file_stem().and_then(|value| value.to_str()).unwrap_or("Animation").to_owned();
            animations.push(AnimationPreviewInfo {
                path: relative_path,
                name,
                duration: asset.duration(),
                fps: if asset.fps().is_finite() { asset.fps() } else { 0.0 },
                joint_count: asset.joint_count(),
            });
        }

        Ok(ModelPreviewBundle { model_bytes, skeleton, skeleton_path: skeleton_path_text, animations })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_package_animation_preview(
    source_path: String,
    animation_path: String,
    skeleton_path: String,
) -> Result<AnimationPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = ensure_preview_workspace(&source_path)?;
        let manifest = read_workspace_manifest(&workspace)?;
        let project_dir = safe_child(&workspace, &manifest.project_dir)?;
        let animation_file = find_project_file(&project_dir, &animation_path)?;
        let skeleton_file = find_project_file(&project_dir, &skeleton_path)?;
        if fs::metadata(&animation_file).map_err(|error| error.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("This animation is over the 32 MB preview limit.".to_owned());
        }
        if fs::metadata(&skeleton_file).map_err(|error| error.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("The animation skeleton is over the 32 MB preview limit.".to_owned());
        }
        let animation_bytes = fs::read(&animation_file).map_err(|error| format!("Could not read animation: {error}"))?;
        let skeleton_bytes = fs::read(&skeleton_file).map_err(|error| format!("Could not read animation skeleton: {error}"))?;
        create_animation_preview(&animation_bytes, &skeleton_bytes, animation_file.file_stem().and_then(|value| value.to_str()).unwrap_or("Animation"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_package_wad_model(
    source_path: String,
    wad_path: String,
    hash: String,
) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = ensure_preview_workspace(&source_path)?;
        let manifest = read_workspace_manifest(&workspace)?;
        let project_dir = safe_child(&workspace, &manifest.project_dir)?;
        let wad_file = find_project_file(&project_dir, &wad_path)?;
        let mut wad = Wad::mount(
            File::open(&wad_file).map_err(|error| format!("Could not open WAD archive: {error}"))?,
        )
        .map_err(|error| format!("Could not read WAD archive: {error}"))?;
        let path_hash: WadHash = hash.parse().map_err(|_| "Invalid model hash.".to_owned())?;
        let chunk = wad.chunks().get(path_hash).copied()
            .ok_or_else(|| "This WAD no longer contains the selected model.".to_owned())?;
        if chunk.uncompressed_size() > 32 * 1024 * 1024 {
            return Err("This model is over the 32 MB preview limit.".to_owned());
        }
        let bytes = wad.load_chunk_decompressed(&chunk)
            .map_err(|error| format!("Could not decompress WAD model: {error}"))?;
        if !bytes.starts_with(&[0x33, 0x22, 0x11, 0x00]) {
            return Err("This WAD entry is not an SKN model.".to_owned());
        }
        Ok(bytes.to_vec())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_package_wad_skeleton_preview(
    source_path: String,
    wad_path: String,
    hash: String,
) -> Result<SkeletonPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = read_wad_asset_bytes(&source_path, &wad_path, &hash)?;
        parse_skeleton_preview(&bytes)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_package_wad_animation_preview(
    source_path: String,
    wad_path: String,
    animation_hash: String,
    skeleton_hash: String,
) -> Result<AnimationPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let animation_bytes = read_wad_asset_bytes(&source_path, &wad_path, &animation_hash)?;
        let skeleton_bytes = read_wad_asset_bytes(&source_path, &wad_path, &skeleton_hash)?;
        create_animation_preview(&animation_bytes, &skeleton_bytes, &format!("0x{animation_hash}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn read_wad_asset_bytes(source_path: &str, wad_path: &str, hash: &str) -> Result<Vec<u8>, String> {
    let workspace = ensure_preview_workspace(source_path)?;
    let manifest = read_workspace_manifest(&workspace)?;
    let project_dir = safe_child(&workspace, &manifest.project_dir)?;
    let wad_file = find_project_file(&project_dir, wad_path)?;
    let mut wad = Wad::mount(File::open(&wad_file).map_err(|error| format!("Could not open WAD archive: {error}"))?)
        .map_err(|error| format!("Could not read WAD archive: {error}"))?;
    let path_hash: WadHash = hash.parse().map_err(|_| "Invalid WAD asset hash.".to_owned())?;
    let chunk = wad.chunks().get(path_hash).copied()
        .ok_or_else(|| "This WAD no longer contains the selected asset.".to_owned())?;
    if chunk.uncompressed_size() > 32 * 1024 * 1024 {
        return Err("This WAD asset is over the 32 MB preview limit.".to_owned());
    }
    wad.load_chunk_decompressed(&chunk)
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("Could not decompress WAD asset: {error}"))
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

fn model_animation_scope(entry_path: &str) -> Option<String> {
    let normalized = normalize_package_path(entry_path);
    let parts: Vec<_> = normalized.split('/').collect();
    if let Some(index) = parts.iter().position(|part| *part == "characters" || *part == "champions") {
        if parts.get(index + 1).is_some() {
            return Some(format!("{}/", parts[..=index + 1].join("/")));
        }
    }
    normalized.rsplit_once('/').map(|(parent, _)| format!("{parent}/"))
}

fn collect_files_with_extension(root: &Path, extension: &str, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files_with_extension(&path, extension, found)?;
        } else if file_type.is_file()
            && path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            found.push(path);
        }
    }
    Ok(())
}

fn parse_skeleton_preview(bytes: &[u8]) -> Result<SkeletonPreview, String> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("The model skeleton is over the 32 MB preview limit.".to_owned());
    }
    let rig = RigResource::from_reader(&mut Cursor::new(bytes))
        .map_err(|error| format!("Could not read model skeleton: {error}"))?;
    Ok(SkeletonPreview {
        name: rig.name().to_owned(),
        asset_name: rig.asset_name().to_owned(),
        influences: rig.influences().to_vec(),
        joints: rig.joints().iter().map(|joint| SkeletonJointPreview {
            name: joint.name().to_owned(),
            hash: BinHash::hash_str(joint.name()).0,
            id: joint.id(),
            parent_id: joint.parent_id(),
            local_translation: joint.local_translation().to_array(),
            local_rotation: joint.local_rotation().to_array(),
            local_scale: joint.local_scale().to_array(),
            inverse_bind_matrix: joint.inverse_bind_transform().to_cols_array(),
        }).collect(),
    })
}

fn create_animation_preview(bytes: &[u8], skeleton_bytes: &[u8], name: &str) -> Result<AnimationPreview, String> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("This animation is over the 32 MB preview limit.".to_owned());
    }
    let animation = AnimationAsset::from_reader(&mut Cursor::new(bytes))
        .map_err(|error| format!("Could not read animation: {error}"))?;
    let skeleton = parse_skeleton_preview(skeleton_bytes)?;
    let duration = animation.duration();
    if !duration.is_finite() || duration <= 0.0 || duration > 120.0 {
        return Err("This animation has a duration Sable cannot preview.".to_owned());
    }
    let source_fps = animation.fps();
    let requested_rate = if source_fps.is_finite() && source_fps > 0.0 { source_fps.min(30.0) } else { 24.0 };
    let sample_count = ((duration * requested_rate).ceil() as usize).clamp(1, 600) + 1;
    let sample_rate = (sample_count - 1) as f32 / duration;
    let joint_hashes: Vec<_> = skeleton.joints.iter().map(|joint| joint.hash).collect();
    let mut frames = Vec::with_capacity(sample_count);
    let mut matched_joint_count = 0;
    for frame_index in 0..sample_count {
        let time = duration * frame_index as f32 / (sample_count - 1) as f32;
        let evaluated = animation.evaluate(time);
        let mut frame = Vec::new();
        for (joint_index, hash) in joint_hashes.iter().enumerate() {
            if let Some((rotation, translation, scale)) = evaluated.get(hash) {
                frame.push(AnimationJointSample {
                    joint: joint_index,
                    rotation: rotation.to_array(),
                    translation: translation.to_array(),
                    scale: scale.to_array(),
                });
            }
        }
        matched_joint_count = matched_joint_count.max(frame.len());
        frames.push(frame);
    }
    if matched_joint_count == 0 {
        return Err("This animation does not contain tracks for the selected skeleton.".to_owned());
    }
    let fps = if animation.fps().is_finite() { animation.fps() } else { sample_rate };
    Ok(AnimationPreview {
        name: name.to_owned(),
        duration,
        fps,
        sample_rate,
        frames,
    })
}

fn find_project_file(project_dir: &Path, requested_path: &str) -> Result<PathBuf, String> {
    let relative = normalize_package_path(requested_path);
    if let Ok(candidate) = safe_child(project_dir, &relative) {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let basename = Path::new(&relative)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("Invalid package path: {requested_path}"))?;
    let mut matches = Vec::new();
    collect_files_named(project_dir, basename, &mut matches)
        .map_err(|error| format!("Could not search imported files: {error}"))?;
    match matches.len() {
        0 => Err(format!("Could not find {requested_path} in the imported project.")),
        1 => Ok(matches.remove(0)),
        _ => {
            let suffix = format!("/{}", relative);
            if let Some(exact) = matches
                .iter()
                .find(|path| path.to_string_lossy().replace('\\', "/").to_ascii_lowercase().ends_with(&suffix))
            {
                Ok(exact.clone())
            } else {
                Err(format!("More than one imported file matches {requested_path}."))
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
        .ok_or_else(|| format!("Sable could not locate this texture: {display_path}"))?;
    let (hash_name, is_dds) = if let Some(name) = texture_name.strip_suffix(".dds") {
        (name, true)
    } else if let Some(name) = texture_name.strip_suffix(".tex") {
        (name, false)
    } else {
        return Err(format!("This WAD texture path must end in .tex or .dds: {display_path}"));
    };
    let hash_text = hash_name.strip_prefix("0x").unwrap_or(hash_name);
    let hash: WadHash = hash_text
        .parse()
        .map_err(|_| format!("This WAD texture has an invalid hash: {display_path}"))?;
    let workspace_manifest = read_workspace_manifest(workspace)?;
    let project_dir = safe_child(workspace, &workspace_manifest.project_dir)?;
    let wad_file = find_project_file(&project_dir, wad_relative)?;
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
    if is_dds {
        if !tex_bytes.starts_with(b"DDS ") {
            return Err(format!("This WAD entry is not a DDS texture: {display_path}"));
        }
        return decode_dds_preview(&tex_bytes, display_path);
    }
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

fn decode_dds_preview(bytes: &[u8], display_path: &str) -> Result<TexturePreviewResult, String> {
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Dds)
        .map_err(|error| format!("Could not decode DDS texture {display_path}: {error}"))?;
    let mut output = Cursor::new(Vec::new());
    decoded
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| format!("Could not encode DDS preview: {error}"))?;
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
