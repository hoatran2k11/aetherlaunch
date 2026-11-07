fn main() {
    // Nhúng các biến DISCORD_* trong file `.env` (thư mục gốc dự án hoặc src-tauri) vào bản build,
    // để bản cài đặt chạy được mà không cần kèm file .env. (Application ID của Discord là thông tin công khai.)
    let files = ["../.env", ".env"];
    for f in files {
        println!("cargo:rerun-if-changed={f}");
    }
    for key in [
        "DISCORD_CLIENT_ID",
        "DISCORD_LARGE_IMAGE",
        "DISCORD_SMALL_IMAGE",
        "DISCORD_LARGE_TEXT",
        "DISCORD_BUTTON_LABEL",
        "DISCORD_BUTTON_URL",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    for f in files {
        if let Ok(iter) = dotenvy::from_path_iter(f) {
            for (k, v) in iter.flatten() {
                if k.starts_with("DISCORD_") && !v.contains(['\n', '\r']) {
                    println!("cargo:rustc-env={k}={v}");
                }
            }
        }
    }
    tauri_build::build()
}
