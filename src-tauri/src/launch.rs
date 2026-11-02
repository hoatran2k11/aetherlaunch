//! Chạy GTA:SA qua helper x86 (samp_helper.exe).
//! App Tauri thường là x64 nên KHÔNG thể tự inject vào gta_sa.exe (32-bit):
//! địa chỉ LoadLibraryW của kernel32 64-bit khác với tiến trình 32-bit.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCheck {
    pub exe: bool,
    pub samp_dll: bool,
}

fn game_dir(exe_path: &str) -> Option<PathBuf> {
    let p = Path::new(exe_path);
    let is_exe = p
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("gta_sa.exe"))
        .unwrap_or(false);
    if is_exe && p.is_file() {
        p.parent().map(|d| d.to_path_buf())
    } else {
        None
    }
}

pub fn check(exe_path: &str) -> GameCheck {
    match game_dir(exe_path) {
        Some(d) => GameCheck {
            exe: true,
            samp_dll: d.join("samp.dll").is_file(),
        },
        None => GameCheck {
            exe: false,
            samp_dll: false,
        },
    }
}

fn find_helper(app: &AppHandle) -> Result<PathBuf, String> {
    let mut c: Vec<PathBuf> = vec![];
    if let Ok(r) = app.path().resource_dir() {
        c.push(r.join("resources").join("samp_helper.exe"));
        c.push(r.join("samp_helper.exe"));
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        c.push(d.join("resources").join("samp_helper.exe"));
        c.push(d.join("samp_helper.exe"));
    }
    c.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("samp_helper.exe"));
    c.into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| "Không tìm thấy samp_helper.exe (build bằng helper/build_helper.bat)".to_string())
}

/// Tên SA-MP: 3–20 ký tự, BẮT BUỘC có dấu gạch dưới '_'. Dấu cách tự đổi thành '_'.
fn normalize_name(name: &str) -> Result<String, String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join("_");
    let ok = (3..=20).contains(&n.chars().count())
        && n.contains('_')
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_[]()$@.=".contains(c));
    if ok {
        Ok(n)
    } else {
        Err("Tên người chơi phải dài 3–20 ký tự, bắt buộc có dấu gạch dưới (_), chỉ gồm chữ/số và _ [ ] ( ) $ @ . =".into())
    }
}

fn check_host(h: &str) -> Result<(), String> {
    let ok = !h.is_empty()
        && h.len() <= 253
        && h.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c));
    if ok {
        Ok(())
    } else {
        Err("IP/hostname server không hợp lệ".into())
    }
}

fn explain(step: &str, code: u32) -> String {
    let hint = match code {
        2 | 3 => "không tìm thấy file",
        5 => "bị từ chối truy cập – thử chạy launcher bằng quyền Administrator",
        126 => "không nạp được samp.dll (thiếu thư viện phụ thuộc hoặc DLL hỏng)",
        193 => "sai kiến trúc – helper phải được build x86 (32-bit)",
        740 => "gta_sa.exe cần quyền Administrator – hãy chạy launcher bằng Run as administrator",
        _ => "",
    };
    format!("Helper lỗi ở bước {step} (mã {code}). {hint}").trim().to_string()
}

pub fn run(
    app: &AppHandle,
    exe_path: &str,
    name: &str,
    host: &str,
    port: u16,
    password: Option<&str>,
) -> Result<String, String> {
    let dir = game_dir(exe_path).ok_or("Đường dẫn gta_sa.exe không hợp lệ hoặc file không tồn tại")?;
    if !dir.join("samp.dll").is_file() {
        return Err("Không thấy samp.dll cạnh gta_sa.exe – hãy cài SA-MP hoặc open.mp client trước".into());
    }
    let name = normalize_name(name)?;
    check_host(host)?;
    if port == 0 {
        return Err("Port không hợp lệ".into());
    }
    let pass = password.filter(|p| !p.is_empty());
    if let Some(p) = pass {
        if p.len() > 64 || p.chars().any(|c| c.is_whitespace() || c == '"' || c.is_control()) {
            return Err("Mật khẩu server không được chứa dấu cách, dấu nháy hoặc ký tự điều khiển".into());
        }
    }

    let helper = find_helper(app)?;
    let mut cmd = Command::new(helper);
    cmd.arg(&dir)
        .args(["-c", "-h", host, "-p", &port.to_string(), "-n", &name]);
    if let Some(p) = pass {
        cmd.args(["-z", p]);
    }
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd.output().map_err(|e| format!("Không chạy được helper: {e}"))?;
    if out.status.success() {
        return Ok(format!("Đã khởi chạy GTA:SA → {host}:{port} (tên: {name})"));
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(line) = stderr.lines().find(|l| l.starts_with("ERR|")) {
        let mut it = line.split('|').skip(1);
        let step = it.next().unwrap_or("?");
        let code = it.next().and_then(|c| c.trim().parse().ok()).unwrap_or(0);
        return Err(explain(step, code));
    }
    Err(format!(
        "Helper thoát với mã {}. {}",
        out.status.code().unwrap_or(-1),
        stderr.trim()
    ))
}
