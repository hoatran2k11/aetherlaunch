#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod forum;
mod launch;
mod query;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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

/// Đặt trạng thái Discord Rich Presence (trả về ngay; việc gửi chạy ở luồng riêng).
#[tauri::command]
fn discord_set_activity(rpc: tauri::State<'_, discord::Rpc>, activity: discord::ActivityPayload) -> Result<(), String> {
    rpc.set(activity)
}

/// Xoá trạng thái Discord đang hiển thị.
#[tauri::command]
fn discord_clear(rpc: tauri::State<'_, discord::Rpc>) -> Result<(), String> {
    rpc.clear()
}

/// Tình trạng cấu hình / kết nối Discord (connect = true: thử kết nối ngay).
#[tauri::command]
async fn discord_info(app: tauri::AppHandle, connect: bool) -> discord::RpcInfo {
    tauri::async_runtime::spawn_blocking(move || app.state::<discord::Rpc>().info(connect))
        .await
        .unwrap_or_default()
}

/// gta_sa.exe có đang chạy không (để biết khi nào đổi trạng thái Discord về "trong launcher").
#[tauri::command]
async fn game_running() -> bool {
    tauri::async_runtime::spawn_blocking(launch::game_running).await.unwrap_or(false)
}

/// Mở link forum trong cửa sổ trình duyệt tích hợp (một cửa sổ dùng chung, tái sử dụng khi mở link khác).
/// Cửa sổ này tải trang ngoài nên KHÔNG nằm trong capabilities: trang forum không có quyền gọi API của Tauri.
#[tauri::command]
async fn open_forum(app: tauri::AppHandle, url: String, title: Option<String>) -> Result<(), String> {
    let url = forum::validate_url(&url)?;
    let parsed: tauri::Url = url.parse().map_err(|e| format!("Link forum không hợp lệ: {e}"))?;
    let name = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Forum".into());
    let title = format!("{name} — AetherLaunch");

    if let Some(w) = app.get_webview_window("forum") {
        let js = format!(
            "window.location.replace({});",
            serde_json::to_string(parsed.as_str()).map_err(|e| e.to_string())?
        );
        w.eval(&js).map_err(|e| e.to_string())?;
        let _ = w.set_title(&title);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "forum", WebviewUrl::External(parsed))
        .title(title)
        .inner_size(1200.0, 800.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Nạp `.env`: cạnh file .exe (bản cài), rồi thư mục hiện tại và các thư mục cha (khi chạy `tauri dev`).
/// Biến môi trường thật luôn được ưu tiên; không có thì dùng giá trị đã nhúng lúc build.
fn load_dotenv() {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        let _ = dotenvy::from_path(dir.join(".env"));
    }
    let _ = dotenvy::dotenv();
}

fn main() {
    load_dotenv();
    tauri::Builder::default()
        .manage(discord::Rpc::new(discord::RpcConfig::load()))
        .plugin(tauri_plugin_dialog::init())
        // Đóng cửa sổ chính thì thoát hẳn app (kể cả cửa sổ forum đang mở)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            query_server,
            query_players,
            check_game,
            launch_game,
            open_forum,
            discord_set_activity,
            discord_clear,
            discord_info,
            game_running
        ])
        .run(tauri::generate_context!())
        .expect("Lỗi khi chạy ứng dụng Tauri");
}
