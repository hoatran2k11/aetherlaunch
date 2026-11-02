#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launch;
mod query;

/// Query thông tin server (info + rules + ping + người chơi) qua UDP.
#[tauri::command]
async fn query_server(
    host: String,
    port: u16,
    encoding: Option<String>,
) -> Result<query::ServerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        query::query_all(host.trim(), port, encoding.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Danh sách người chơi (opcode 'd', dự phòng 'c'). Gọi riêng để không chặn thẻ trạng thái.
#[tauri::command]
async fn query_players(
    host: String,
    port: u16,
    encoding: Option<String>,
) -> Result<query::PlayerList, String> {
    tauri::async_runtime::spawn_blocking(move || {
        query::query_players(host.trim(), port, encoding.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kiểm tra gta_sa.exe và samp.dll cho trang Cài đặt.
#[tauri::command]
fn check_game(exe_path: String) -> launch::GameCheck {
    launch::check(&exe_path)
}

/// Gọi helper x86 để chạy game + inject samp.dll.
#[tauri::command]
async fn launch_game(
    app: tauri::AppHandle,
    exe_path: String,
    name: String,
    host: String,
    port: u16,
    password: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        launch::run(&app, &exe_path, &name, host.trim(), port, password.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![query_server, query_players, check_game, launch_game])
        .run(tauri::generate_context!())
        .expect("Lỗi khi chạy ứng dụng Tauri");
}
