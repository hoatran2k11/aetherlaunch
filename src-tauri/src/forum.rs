//! Kiểm tra link forum trước khi mở trong cửa sổ trình duyệt tích hợp.
//! Chỉ cho phép http/https để tránh `file://`, `javascript:`... và các mẹo đánh lừa kiểu `https://a.com@evil.com`.

pub fn validate_url(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() || s.len() > 2048 {
        return Err("Link forum trống hoặc quá dài".into());
    }
    if s.contains('\\') || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Link forum chứa ký tự không hợp lệ".into());
    }
    let lower = s.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &s[8..]
    } else if lower.starts_with("http://") {
        &s[7..]
    } else {
        return Err("Link forum phải bắt đầu bằng http:// hoặc https://".into());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.starts_with(':') || authority.starts_with('.') {
        return Err("Link forum thiếu tên miền".into());
    }
    if authority.contains('@') {
        return Err("Link forum không được chứa user@host".into());
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_links() {
        for u in ["https://gtaviet.net/forum", "HTTP://a.b", "https://a.b:8080/x?y#z", "  https://f.example.com/t/1  "] {
            assert!(validate_url(u).is_ok(), "{u}");
        }
        assert_eq!(validate_url("  https://a.b/x ").unwrap(), "https://a.b/x");
    }

    #[test]
    fn rejects_unsafe_or_malformed() {
        for u in [
            "", "gtaviet.net", "ftp://x.y", "javascript:alert(1)", "file:///c:/x", "data:text/html,hi",
            "https://", "https:///x", "https://?q=1", "https://a b", "https://good.com@evil.com",
            "https://a.b\\x", "https://:80/x", "https://a.b/\nx",
        ] {
            assert!(validate_url(u).is_err(), "{u:?} lẽ ra phải bị từ chối");
        }
        assert!(validate_url(&format!("https://a.b/{}", "x".repeat(3000))).is_err());
    }
}
