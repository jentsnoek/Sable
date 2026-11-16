mod workspace;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            workspace::import_mod_package,
            workspace::pack_mod_workspace,
            workspace::open_sable_workspace,
            workspace::preview_package_texture,
            workspace::inspect_package_wad_textures
        ])
        .run(tauri::generate_context!())
        .expect("Sable failed to start");
}
