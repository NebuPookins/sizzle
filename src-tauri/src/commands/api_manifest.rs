use serde_json::Value;

#[tauri::command]
pub fn get_api_manifest() -> Value {
    serde_json::from_str(crate::generated_api_manifest::API_MANIFEST_JSON)
        .expect("generated API manifest JSON is valid")
}
