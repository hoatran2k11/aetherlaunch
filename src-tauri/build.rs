// Bộ đọc .env dùng chung với ứng dụng (dễ tính với giá trị có dấu cách, xem src/envfile.rs)
#[path = "src/envfile.rs"]
mod envfile;

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
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    for f in files {
        if let Some(text) = envfile::read_text(std::path::Path::new(f)) {
            for (k, v) in envfile::parse(&text) {
                if k.starts_with("DISCORD_") {
                    println!("cargo:rustc-env={k}={v}");
                }
            }
        }
    }
    tauri_build::build()
}
