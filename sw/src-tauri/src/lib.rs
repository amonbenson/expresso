// TODO: deserialise into the expresso crate's SettingsPatch type and forward as SysEx.
#[tauri::command]
fn patch_settings(patch: serde_json::Value) {
    println!("Settings patch: {patch}");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![patch_settings])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
