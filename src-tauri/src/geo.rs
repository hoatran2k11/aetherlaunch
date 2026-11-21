//! Vị trí máy chủ game: phân giải host -> IP, hỏi ipinfo lấy quốc gia, tải ảnh cờ (FlagCDN, dự phòng FlagsAPI).
//!
//! - Chạy hoàn toàn ở phía Rust: webview không cần quyền truy cập mạng ngoài, CSP giữ nguyên.
//!   Ảnh cờ được trả về dạng `data:image/png;base64,...` nên chỉ cần `img-src data:` (đã có sẵn).
//! - ipinfo: có `IPINFO_TOKEN` thì dùng IPinfo Lite (`api.ipinfo.io/lite/<ip>`, miễn phí, không giới hạn);
//!   không có token thì dùng endpoint cũ `ipinfo.io/<ip>/json` (giới hạn thấp, có thêm thành phố / vùng).
//! - Không bao giờ gửi IP nội bộ (192.168.x.x, 127.0.0.1, ...) ra ngoài: trả `PRIVATE`.
//! - Token chỉ nằm trong query của request tới ipinfo, không xuất hiện trong thông báo lỗi.
//! - Cache trong bộ nhớ: IP 24 giờ, ảnh cờ cho tới khi tắt app.
//! Module không phụ thuộc Tauri (chỉ tokio + reqwest) để kiểm thử độc lập.

use crate::data::{check_url, client};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Lỗi đặc biệt: host là địa chỉ nội bộ (frontend không hiện gì, không coi là lỗi).
pub const PRIVATE: &str = "PRIVATE";

const GEO_TTL: Duration = Duration::from_secs(24 * 3600);
const MAX_JSON: usize = 64 * 1024;
const MAX_FLAG: usize = 64 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Geo {
    pub ip: String,
    /// Mã quốc gia ISO 3166-1 alpha-2, chữ hoa (VD "VN")
    pub country_code: String,
    /// Tên quốc gia tiếng Anh nếu ipinfo trả về (Lite); frontend tự dịch từ mã bằng Intl.DisplayNames
    pub country: String,
    pub city: String,
    pub region: String,
    /// Ảnh cờ dạng data URI; None nếu cả hai nhà cung cấp đều lỗi
    pub flag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Endpoints {
    pub ipinfo_lite: String,
    pub ipinfo_legacy: String,
    pub flagcdn: String,
    pub flagsapi: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            ipinfo_lite: "https://api.ipinfo.io/lite".into(),
            ipinfo_legacy: "https://ipinfo.io".into(),
            flagcdn: "https://flagcdn.com".into(),
            flagsapi: "https://flagsapi.com".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub token: Option<String>,
    /// true = thử FlagsAPI trước, FlagCDN dự phòng (mặc định ngược lại)
    pub flagsapi_first: bool,
    pub ep: Endpoints,
}

/// Giá trị nhúng lúc build (build.rs đọc `.env`).
fn builtin(key: &str) -> Option<&'static str> {
    match key {
        "IPINFO_TOKEN" => option_env!("IPINFO_TOKEN"),
        "FLAG_PROVIDER" => option_env!("FLAG_PROVIDER"),
        _ => None,
    }
}

impl Config {
    /// Ưu tiên biến môi trường lúc chạy > giá trị nhúng lúc build.
    pub fn load() -> Config {
        Self::from_lookup(|k| std::env::var(k).ok().or_else(|| builtin(k).map(String::from)))
    }
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Config {
        let val = |k: &str| get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        // Token ipinfo chỉ gồm chữ + số; giá trị lạ (lỡ dán dấu nháy, "your_token"...) coi như chưa cấu hình
        let token = val("IPINFO_TOKEN").filter(|t| (6..=64).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric()));
        let flagsapi_first = val("FLAG_PROVIDER").map(|v| v.eq_ignore_ascii_case("flagsapi")).unwrap_or(false);
        Config { token, flagsapi_first, ep: Endpoints::default() }
    }
}

// ---------------------------------------------------------------------
// IP công khai?
// ---------------------------------------------------------------------

pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xC0) == 64) // CGNAT 100.64.0.0/10
                || o[0] >= 224) // multicast + dự trữ
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            if let Some(m) = v.to_ipv4_mapped() {
                return is_public(IpAddr::V4(m));
            }
            !(v.is_loopback()
                || v.is_unspecified()
                || (s[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // link-local
                || (s[0] & 0xff00) == 0xff00 // multicast
                || (s[0] == 0x2001 && s[1] == 0x0db8)) // tài liệu
        }
    }
}

fn valid_host(h: &str) -> bool {
    !h.is_empty() && h.len() <= 253 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_' || b == b':')
}

/// host (IP hoặc domain) -> IP công khai. Ưu tiên IPv4.
async fn resolve(host: &str) -> Result<IpAddr, String> {
    if !valid_host(host) {
        return Err("Địa chỉ server không hợp lệ".into());
    }
    let ips: Vec<IpAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        let h = host.to_string();
        let r = tokio::time::timeout(
            Duration::from_secs(6),
            tokio::task::spawn_blocking(move || (h.as_str(), 0u16).to_socket_addrs().map(|i| i.map(|a| a.ip()).collect::<Vec<_>>())),
        )
        .await
        .map_err(|_| "Phân giải tên miền quá lâu".to_string())?
        .map_err(|e| e.to_string())?;
        r.map_err(|_| "Không phân giải được tên miền".to_string())?
    };
    if ips.is_empty() {
        return Err("Không phân giải được tên miền".into());
    }
    let public: Vec<IpAddr> = ips.into_iter().filter(|i| is_public(*i)).collect();
    public
        .iter()
        .find(|i| i.is_ipv4())
        .or_else(|| public.first())
        .copied()
        .ok_or_else(|| PRIVATE.to_string())
}

// ---------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------

fn net_err(e: reqwest::Error) -> String {
    // without_url: lỗi của reqwest chứa nguyên URL, mà URL ipinfo có token
    let e = e.without_url();
    if e.is_timeout() {
        "Hết thời gian chờ dịch vụ vị trí".into()
    } else if e.is_connect() {
        "Không kết nối được dịch vụ vị trí".into()
    } else {
        "Lỗi mạng khi tra vị trí".into()
    }
}

async fn read_capped(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    while let Some(c) = resp.chunk().await.map_err(net_err)? {
        buf.extend_from_slice(&c);
        if buf.len() > max {
            return Err("Phản hồi quá lớn".into());
        }
    }
    Ok(buf)
}

#[derive(Deserialize, Default)]
struct Raw {
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    bogon: Option<bool>,
}

/// Chuẩn hoá JSON của cả Lite (`country_code` + `country` là tên) lẫn endpoint cũ (`country` là mã 2 chữ).
fn parse_ipinfo(body: &[u8], ip: IpAddr) -> Result<Geo, String> {
    let r: Raw = serde_json::from_slice(body).map_err(|_| "ipinfo trả về dữ liệu không đọc được".to_string())?;
    if r.bogon == Some(true) {
        return Err(PRIVATE.into());
    }
    let is_cc = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic());
    let country = r.country.unwrap_or_default();
    let code = match r.country_code.as_deref().map(str::trim) {
        Some(c) if is_cc(c) => c.to_ascii_uppercase(),
        _ if is_cc(country.trim()) => country.trim().to_ascii_uppercase(),
        _ => return Err("ipinfo không cho biết quốc gia của IP này".into()),
    };
    let name = if is_cc(country.trim()) { String::new() } else { country.trim().to_string() };
    Ok(Geo {
        ip: ip.to_string(),
        country_code: code,
        country: name,
        city: r.city.unwrap_or_default(),
        region: r.region.unwrap_or_default(),
        flag: None,
    })
}

async fn fetch_ipinfo(c: &Client, ip: IpAddr, cfg: &Config) -> Result<Geo, String> {
    let mut url = match &cfg.token {
        Some(t) => {
            let mut u = Url::parse(&format!("{}/{}", cfg.ep.ipinfo_lite.trim_end_matches('/'), ip)).map_err(|_| "Cấu hình ipinfo sai")?;
            u.query_pairs_mut().append_pair("token", t);
            u
        }
        None => Url::parse(&format!("{}/{}/json", cfg.ep.ipinfo_legacy.trim_end_matches('/'), ip)).map_err(|_| "Cấu hình ipinfo sai")?,
    };
    check_url(&url)?;
    url.set_fragment(None);
    let resp = c.get(url).timeout(Duration::from_secs(8)).send().await.map_err(net_err)?;
    match resp.status() {
        s if s.is_success() => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err(if cfg.token.is_some() { "IPINFO_TOKEN không hợp lệ hoặc đã bị thu hồi".into() } else { "ipinfo từ chối yêu cầu (hãy đặt IPINFO_TOKEN trong .env)".into() })
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(if cfg.token.is_some() { "ipinfo giới hạn tần suất, thử lại sau".into() } else { "Hết hạn mức ipinfo không token – hãy đặt IPINFO_TOKEN (Lite miễn phí) trong .env".into() })
        }
        s => return Err(format!("ipinfo trả về mã {}", s.as_u16())),
    }
    let body = read_capped(resp, MAX_JSON).await?;
    parse_ipinfo(&body, ip)
}

// ---------------------------------------------------------------------
// Ảnh cờ
// ---------------------------------------------------------------------

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

fn flag_urls(cc: &str, cfg: &Config) -> [String; 2] {
    let cdn = format!("{}/w40/{}.png", cfg.ep.flagcdn.trim_end_matches('/'), cc.to_ascii_lowercase());
    let api = format!("{}/{}/flat/32.png", cfg.ep.flagsapi.trim_end_matches('/'), cc.to_ascii_uppercase());
    if cfg.flagsapi_first { [api, cdn] } else { [cdn, api] }
}

async fn fetch_flag_from(c: &Client, url: &str) -> Result<String, String> {
    let u = Url::parse(url).map_err(|_| "URL cờ sai")?;
    check_url(&u)?;
    let resp = c.get(u).timeout(Duration::from_secs(6)).send().await.map_err(net_err)?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let body = read_capped(resp, MAX_FLAG).await?;
    if !body.starts_with(PNG_MAGIC) {
        return Err("Không phải ảnh PNG".into());
    }
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(body)))
}

fn flag_cache() -> &'static Mutex<HashMap<String, String>> {
    static C: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Thử lần lượt các nhà cung cấp; lỗi hết thì None (không làm hỏng kết quả vị trí).
async fn fetch_flag(c: &Client, cc: &str, cfg: &Config) -> Option<String> {
    let key = format!("{}|{}", cc, cfg.flagsapi_first);
    if let Some(f) = flag_cache().lock().ok()?.get(&key) {
        return Some(f.clone());
    }
    for url in flag_urls(cc, cfg) {
        if let Ok(f) = fetch_flag_from(c, &url).await {
            if let Ok(mut m) = flag_cache().lock() {
                m.insert(key, f.clone());
            }
            return Some(f);
        }
    }
    None
}

// ---------------------------------------------------------------------
// API chính
// ---------------------------------------------------------------------

fn geo_cache() -> &'static Mutex<HashMap<IpAddr, (Instant, Geo)>> {
    static C: OnceLock<Mutex<HashMap<IpAddr, (Instant, Geo)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

pub async fn lookup(host: &str, cfg: &Config) -> Result<Geo, String> {
    let ip = resolve(host.trim()).await?;
    if let Some((t, g)) = geo_cache().lock().ok().and_then(|m| m.get(&ip).cloned()) {
        if t.elapsed() < GEO_TTL && (g.flag.is_some() || t.elapsed() < Duration::from_secs(300)) {
            return Ok(g);
        }
    }
    let c = client()?;
    let mut g = fetch_ipinfo(&c, ip, cfg).await?;
    g.flag = fetch_flag(&c, &g.country_code, cfg).await;
    if let Ok(mut m) = geo_cache().lock() {
        m.insert(ip, (Instant::now(), g.clone()));
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn public_ips() {
        for ok in ["8.8.8.8", "1.1.1.1", "103.28.36.1", "2606:4700:4700::1111", "::ffff:8.8.8.8"] {
            assert!(is_public(ip(ok)), "{ok}");
        }
        for bad in [
            "127.0.0.1", "10.1.2.3", "192.168.1.5", "172.16.0.1", "172.31.255.255", "169.254.1.1", "0.0.0.0", "100.64.0.1",
            "100.127.255.255", "224.0.0.1", "255.255.255.255", "192.0.2.1", "::1", "::", "fc00::1", "fd12::1", "fe80::1",
            "ff02::1", "2001:db8::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1",
        ] {
            assert!(!is_public(ip(bad)), "{bad}");
        }
        assert!(is_public(ip("172.32.0.1")) && is_public(ip("100.128.0.1")));
    }

    #[test]
    fn parses_lite_and_legacy() {
        let lite = br#"{"ip":"8.8.8.8","asn":"AS15169","as_name":"Google LLC","country_code":"us","country":"United States","continent_code":"NA"}"#;
        let g = parse_ipinfo(lite, ip("8.8.8.8")).unwrap();
        assert_eq!((g.country_code.as_str(), g.country.as_str(), g.city.as_str()), ("US", "United States", ""));
        let legacy = br#"{"ip":"1.2.3.4","city":"Hanoi","region":"Hanoi","country":"VN","loc":"21,105","org":"AS1 X"}"#;
        let g = parse_ipinfo(legacy, ip("1.2.3.4")).unwrap();
        assert_eq!((g.country_code.as_str(), g.country.as_str(), g.city.as_str()), ("VN", "", "Hanoi"));
        assert_eq!(parse_ipinfo(br#"{"ip":"10.0.0.1","bogon":true}"#, ip("10.0.0.1")).unwrap_err(), PRIVATE);
        assert!(parse_ipinfo(br#"{"ip":"1.1.1.1"}"#, ip("1.1.1.1")).is_err());
        assert!(parse_ipinfo(br#"{"country_code":"../x"}"#, ip("1.1.1.1")).is_err());
        assert!(parse_ipinfo(b"<html>", ip("1.1.1.1")).is_err());
    }

    #[test]
    fn config_token_and_provider() {
        let get = |t: &'static str, p: &'static str| move |k: &str| match k { "IPINFO_TOKEN" => Some(t.to_string()), "FLAG_PROVIDER" => Some(p.to_string()), _ => None };
        let c = Config::from_lookup(get("abc123DEF456", "FlagsAPI"));
        assert_eq!(c.token.as_deref(), Some("abc123DEF456")); assert!(c.flagsapi_first);
        for bad in ["", "your_token", "ab", "tok en123", "\"abc123def\"", "abc123def&x=1"] {
            assert!(Config::from_lookup(get(bad, "")).token.is_none(), "{bad}");
        }
        assert!(!Config::from_lookup(get("", "flagcdn")).flagsapi_first);
    }

    #[test]
    fn flag_urls_order() {
        let mut c = Config::default();
        let u = flag_urls("vn", &c);
        assert_eq!(u[0], "https://flagcdn.com/w40/vn.png"); assert_eq!(u[1], "https://flagsapi.com/VN/flat/32.png");
        c.flagsapi_first = true;
        assert_eq!(flag_urls("VN", &c)[0], "https://flagsapi.com/VN/flat/32.png");
    }
}
