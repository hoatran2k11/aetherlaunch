//! Đọc file `.env` đơn giản và dễ tính.
//!
//! - `KEY=giá trị có dấu cách` KHÔNG cần dấu nháy (thư viện dotenvy báo lỗi và DỪNG ở dòng như vậy,
//!   khiến mọi biến phía sau không được nạp).
//! - Hỗ trợ dấu nháy đơn/kép, `export KEY=...`, dòng trống, `# chú thích`, ` # chú thích cuối dòng`,
//!   BOM UTF-8, xuống dòng kiểu Windows (CRLF) và file lưu UTF-16 (PowerShell `echo ... > .env`).
//! - Dùng chung cho `build.rs` (nhúng lúc build) và lúc chạy ứng dụng.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Giải mã nội dung file: UTF-16 (có BOM) hoặc UTF-8.
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8], le: bool| -> String {
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

pub fn read_text(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| decode(&b))
}

pub fn parse(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in content.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map(str::trim_start).unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let v = v.trim();
        let quoted = v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
        let value = if quoted {
            v[1..v.len() - 1].to_string()
        } else {
            // " #" mở đầu chú thích cuối dòng; "#" dính liền (VD: link có #phần-neo) được giữ nguyên
            v.find(" #").map(|i| v[..i].trim_end()).unwrap_or(v).to_string()
        };
        out.push((k.to_string(), value));
    }
    out
}

/// Nạp file vào biến môi trường của tiến trình, KHÔNG ghi đè biến đã có. Trả về số biến đã nạp.
pub fn load_into_env(path: &Path) -> usize {
    let Some(content) = read_text(path) else { return 0 };
    let mut n = 0;
    for (k, v) in parse(&content) {
        if std::env::var_os(&k).is_none() {
            std::env::set_var(&k, v);
            n += 1;
        }
    }
    n
}

/// Tìm `.env` từ thư mục `start` đi ngược lên các thư mục cha.
pub fn find_up(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(|d| d.join(".env")).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(v: &'a [(String, String)], k: &str) -> Option<&'a str> {
        v.iter().find(|(a, _)| a == k).map(|(_, b)| b.as_str())
    }

    #[test]
    fn user_case_unquoted_spaces_do_not_break_following_lines() {
        let v = parse("DISCORD_CLIENT_ID=123456789012345678\nDISCORD_BUTTON_LABEL=by Max Bennett\nDISCORD_BUTTON_URL=https://guns.lol/aoh20119\n");
        assert_eq!(get(&v, "DISCORD_BUTTON_LABEL"), Some("by Max Bennett"));
        assert_eq!(get(&v, "DISCORD_BUTTON_URL"), Some("https://guns.lol/aoh20119"));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn quotes_comments_export_bom_crlf() {
        let text = "\u{feff}# chú thích\r\n\r\nexport A=\"Tải AetherLaunch\"\r\nB='x y'  \r\nC=giá trị # chú thích cuối dòng\r\nD=https://a.b/x#phan-neo\r\n  E = có khoảng trắng quanh dấu =  \r\nKHONG_CO_DAU_BANG\r\n=khong_co_ten\r\nBAD KEY=1\r\n";
        let v = parse(text);
        assert_eq!(get(&v, "A"), Some("Tải AetherLaunch"));
        assert_eq!(get(&v, "B"), Some("x y"));
        assert_eq!(get(&v, "C"), Some("giá trị"));
        assert_eq!(get(&v, "D"), Some("https://a.b/x#phan-neo"));
        assert_eq!(get(&v, "E"), Some("có khoảng trắng quanh dấu ="));
        assert_eq!(v.len(), 5, "{v:?}");
    }

    #[test]
    fn decodes_utf16_files_saved_by_powershell() {
        let s = "DISCORD_BUTTON_LABEL=by Max Bennett\r\n";
        let mut le = vec![0xFF, 0xFE];
        le.extend(s.encode_utf16().flat_map(|u| u.to_le_bytes()));
        assert_eq!(get(&parse(&decode(&le)), "DISCORD_BUTTON_LABEL"), Some("by Max Bennett"));
        let mut be = vec![0xFE, 0xFF];
        be.extend(s.encode_utf16().flat_map(|u| u.to_be_bytes()));
        assert_eq!(get(&parse(&decode(&be)), "DISCORD_BUTTON_LABEL"), Some("by Max Bennett"));
        assert_eq!(decode("Tiếng Việt".as_bytes()), "Tiếng Việt");
    }

    #[test]
    fn load_into_env_does_not_override_and_find_up_walks_parents() {
        let dir = std::env::temp_dir().join(format!("aether_env_{}", std::process::id()));
        let sub = dir.join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join(".env"), "AETHER_T1=từ file\nAETHER_T2=giữ nguyên biến thật\n").unwrap();
        std::env::set_var("AETHER_T2", "đã có");
        assert_eq!(find_up(&sub), Some(dir.join(".env")));
        assert_eq!(load_into_env(&dir.join(".env")), 1);
        assert_eq!(std::env::var("AETHER_T1").unwrap(), "từ file");
        assert_eq!(std::env::var("AETHER_T2").unwrap(), "đã có");
        assert_eq!(load_into_env(&dir.join("khong_ton_tai.env")), 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
