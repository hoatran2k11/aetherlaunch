//! Tải / cập nhật bộ dữ liệu game từ server của bạn.
//!
//! Server chỉ cần host file tĩnh + một `manifest.json` (tạo bằng `tools/make-manifest.mjs`).
//! - Mỗi file được kiểm tra SHA-256; file đã đúng thì bỏ qua, nên chạy lại = cập nhật / sửa file hỏng.
//! - Tải dở dang được nối tiếp bằng HTTP Range (nếu server hỗ trợ), tự thử lại khi mạng chập chờn.
//! - Chặn path traversal (`..`, đường dẫn tuyệt đối, ổ đĩa) và chỉ nhận https
//!   (http chỉ được phép với localhost để thử nghiệm).
//! - Không xoá file thừa trong thư mục cài (mod / file người dùng tự thêm được giữ nguyên).
//!
//! Module này cố ý không phụ thuộc Tauri (chỉ cần tokio + reqwest) để dễ kiểm thử; phần nối với Tauri nằm ở `main.rs`.

use reqwest::header::RANGE;
use reqwest::Client;
use reqwest::redirect::Policy;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Chuỗi lỗi đặc biệt khi người dùng bấm Huỷ (frontend so khớp để không báo lỗi đỏ).
pub const CANCELLED: &str = "CANCELLED";

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FILES: usize = 200_000;
const CHUNK: usize = 128 * 1024;
const MAX_ATTEMPTS: u32 = 4;

// ---------------------------------------------------------------------
// Kiểu dữ liệu
// ---------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub struct FileEntry {
    /// Đường dẫn tương đối, dùng dấu `/` (vd `models/car.dff`).
    pub path: String,
    pub size: u64,
    pub sha256: String,
    /// Tuỳ chọn: link tải riêng (tuyệt đối hoặc tương đối so với `baseUrl`).
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default)]
    pub name: String,
    pub version: String,
    /// Tuỳ chọn: thư mục chứa file. Mặc định là thư mục chứa manifest.json.
    #[serde(default)]
    pub base_url: Option<String>,
    pub files: Vec<FileEntry>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ManifestInfo {
    pub name: String,
    pub version: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// "checking" | "downloading" | "done"
    pub phase: &'static str,
    pub file: String,
    pub file_index: usize,
    pub file_count: usize,
    pub done: u64,
    pub total: u64,
    /// byte / giây (trung bình trượt)
    pub speed: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub dir: String,
    pub version: String,
    /// Đường dẫn gta_sa.exe nếu bộ dữ liệu có file này.
    pub exe: Option<String>,
    pub downloaded: usize,
    pub skipped: usize,
}

/// Cờ điều khiển dùng chung (Tauri `manage`).
#[derive(Default)]
pub struct Control {
    pub cancel: AtomicBool,
    pub running: AtomicBool,
}

impl Control {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------
// Kiểm tra đầu vào
// ---------------------------------------------------------------------

fn is_local(u: &Url) -> bool {
    match u.host_str() {
        Some(h) => {
            let h = h.trim_matches(|c| c == '[' || c == ']');
            h.eq_ignore_ascii_case("localhost")
                || h.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
        }
        None => false,
    }
}

fn check_url(u: &Url) -> Result<(), String> {
    match u.scheme() {
        "https" => Ok(()),
        "http" if is_local(u) => Ok(()),
        _ => Err("Link dữ liệu phải dùng https://".into()),
    }
}

/// Đường dẫn tương đối an toàn: không rỗng, dùng `/`, không `..`, không ổ đĩa / ADS (`:`), không `\`.
pub fn valid_rel_path(p: &str) -> bool {
    if p.is_empty() || p.len() > 400 || p.starts_with('/') || p.ends_with('/') {
        return false;
    }
    if p.chars().any(|c| c == '\\' || c == ':' || c == '\0' || c.is_control() || "<>\"|?*".contains(c)) {
        return false;
    }
    if p.to_ascii_lowercase().ends_with(".part") {
        return false; // tránh đụng file tạm
    }
    p.split('/').all(|s| !s.is_empty() && s != "." && s != ".." && !s.ends_with('.') && !s.ends_with(' '))
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn validate(m: &Manifest) -> Result<u64, String> {
    if m.version.trim().is_empty() || m.version.len() > 64 {
        return Err("Manifest thiếu trường version".into());
    }
    if m.files.is_empty() {
        return Err("Manifest không có file nào".into());
    }
    if m.files.len() > MAX_FILES {
        return Err("Manifest có quá nhiều file".into());
    }
    let mut seen = HashSet::new();
    let mut total: u64 = 0;
    for f in &m.files {
        if !valid_rel_path(&f.path) {
            return Err(format!("Đường dẫn file không hợp lệ trong manifest: {}", f.path));
        }
        if !is_sha256_hex(&f.sha256) {
            return Err(format!("SHA-256 không hợp lệ cho file: {}", f.path));
        }
        if !seen.insert(f.path.to_ascii_lowercase()) {
            return Err(format!("Manifest có file trùng tên: {}", f.path));
        }
        total = total.checked_add(f.size).ok_or("Tổng dung lượng trong manifest quá lớn")?;
    }
    Ok(total)
}

// ---------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------

fn net_err(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Hết thời gian chờ server dữ liệu".into()
    } else if e.is_connect() {
        "Không kết nối được server dữ liệu (kiểm tra mạng hoặc link)".into()
    } else {
        format!("Lỗi mạng: {e}")
    }
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("AetherLaunch/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .redirect(Policy::custom(|a| {
            if a.previous().len() >= 5 {
                a.error("quá nhiều lần chuyển hướng")
            } else if check_url(a.url()).is_ok() {
                a.follow()
            } else {
                a.error("server chuyển hướng sang địa chỉ không an toàn")
            }
        }))
        .build()
        .map_err(|e| format!("Không tạo được HTTP client: {e}"))
}

pub async fn fetch_manifest(url: &str) -> Result<(Manifest, Url), String> {
    let u = Url::parse(url.trim()).map_err(|_| "Link manifest không hợp lệ".to_string())?;
    check_url(&u)?;
    let mut resp = client()?
        .get(u)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(net_err)?;
    if !resp.status().is_success() {
        return Err(format!("Server dữ liệu trả về mã {} khi lấy manifest", resp.status().as_u16()));
    }
    let final_url = resp.url().clone();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(c) = resp.chunk().await.map_err(net_err)? {
        buf.extend_from_slice(&c);
        if buf.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("Manifest quá lớn".into());
        }
    }
    let m: Manifest = serde_json::from_slice(&buf).map_err(|e| format!("Manifest sai định dạng: {e}"))?;
    validate(&m)?;
    Ok((m, final_url))
}

pub async fn manifest_info(url: &str) -> Result<ManifestInfo, String> {
    let (m, _) = fetch_manifest(url).await?;
    let total = validate(&m)?;
    Ok(ManifestInfo {
        name: if m.name.trim().is_empty() { "Dữ liệu game".into() } else { m.name.clone() },
        version: m.version.clone(),
        file_count: m.files.len(),
        total_bytes: total,
    })
}

fn base_url(m: &Manifest, manifest_url: &Url) -> Result<Url, String> {
    let mut b = match &m.base_url {
        Some(s) if !s.trim().is_empty() => manifest_url
            .join(s.trim())
            .map_err(|_| "baseUrl trong manifest không hợp lệ".to_string())?,
        _ => manifest_url.join("./").map_err(|_| "Link manifest không hợp lệ".to_string())?,
    };
    check_url(&b)?;
    if !b.path().ends_with('/') {
        let p = format!("{}/", b.path());
        b.set_path(&p);
    }
    Ok(b)
}

fn file_url(base: &Url, e: &FileEntry) -> Result<Url, String> {
    if let Some(s) = e.url.as_deref().filter(|s| !s.trim().is_empty()) {
        let u = base.join(s.trim()).map_err(|_| format!("Link không hợp lệ cho file {}", e.path))?;
        check_url(&u)?;
        return Ok(u);
    }
    let mut u = base.clone();
    {
        let mut seg = u.path_segments_mut().map_err(|_| "baseUrl không hợp lệ".to_string())?;
        seg.pop_if_empty();
        seg.extend(e.path.split('/'));
    }
    Ok(u)
}

// ---------------------------------------------------------------------
// Tiến độ
// ---------------------------------------------------------------------

struct Tracker<'a> {
    emit: &'a mut (dyn FnMut(&Progress) + Send),
    p: Progress,
    last_emit: Instant,
    sample_at: Instant,
    sample_bytes: u64,
    session: u64,
}

impl<'a> Tracker<'a> {
    fn new(emit: &'a mut (dyn FnMut(&Progress) + Send), total: u64, file_count: usize) -> Self {
        let now = Instant::now();
        Tracker {
            emit,
            p: Progress { phase: "checking", total, file_count, ..Default::default() },
            last_emit: now - Duration::from_secs(1),
            sample_at: now,
            sample_bytes: 0,
            session: 0,
        }
    }
    fn send(&mut self, force: bool) {
        if force || self.last_emit.elapsed() >= Duration::from_millis(100) {
            self.last_emit = Instant::now();
            (self.emit)(&self.p);
        }
    }
    fn begin_file(&mut self, idx: usize, name: &str, phase: &'static str) {
        self.p.file_index = idx;
        self.p.file = name.to_string();
        self.p.phase = phase;
        self.send(true);
    }
    /// Cộng byte vừa tải thật sự (tính tốc độ).
    fn downloaded(&mut self, n: u64) {
        self.p.done += n;
        self.session += n;
        let dt = self.sample_at.elapsed();
        if dt >= Duration::from_millis(500) {
            let inst = ((self.session - self.sample_bytes) as f64 / dt.as_secs_f64()) as u64;
            self.p.speed = if self.p.speed == 0 { inst } else { (self.p.speed + inst) / 2 };
            self.sample_at = Instant::now();
            self.sample_bytes = self.session;
        }
        self.send(false);
    }
}

// ---------------------------------------------------------------------
// Tải một file
// ---------------------------------------------------------------------

enum Fail {
    Cancelled,
    Fatal(String),
    Retry(String),
}

fn join_rel(dir: &Path, rel: &str) -> PathBuf {
    let mut p = dir.to_path_buf();
    for s in rel.split('/') {
        p.push(s);
    }
    p
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 của một file; `limit` = chỉ băm `limit` byte đầu (dùng khi nối tiếp file .part).
fn hash_into(path: &Path, hasher: &mut Sha256, limit: u64, ctl: &Control) -> Result<(), Fail> {
    let mut f = File::open(path).map_err(|e| Fail::Fatal(format!("Không mở được {}: {e}", path.display())))?;
    let mut buf = vec![0u8; CHUNK];
    let mut left = limit;
    while left > 0 {
        if ctl.cancelled() {
            return Err(Fail::Cancelled);
        }
        let want = buf.len().min(left as usize);
        let n = f.read(&mut buf[..want]).map_err(|e| Fail::Fatal(format!("Lỗi đọc {}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(())
}

/// File đã có và đúng (cả dung lượng lẫn SHA-256)? Băm chạy trên luồng riêng để không chặn runtime.
async fn is_up_to_date(dest: &Path, e: &FileEntry, ctl: &Arc<Control>) -> Result<bool, Fail> {
    match fs::metadata(dest) {
        Ok(m) if m.is_file() && m.len() == e.size => {
            let (p, size, want, c) = (dest.to_path_buf(), e.size, e.sha256.clone(), ctl.clone());
            tokio::task::spawn_blocking(move || {
                let mut h = Sha256::new();
                hash_into(&p, &mut h, size, &c)?;
                Ok(hex(&h.finalize()).eq_ignore_ascii_case(&want))
            })
            .await
            .map_err(|er| Fail::Fatal(er.to_string()))?
        }
        _ => Ok(false),
    }
}

async fn hash_prefix(path: &Path, limit: u64, ctl: &Arc<Control>) -> Result<Sha256, Fail> {
    let (p, c) = (path.to_path_buf(), ctl.clone());
    tokio::task::spawn_blocking(move || {
        let mut h = Sha256::new();
        hash_into(&p, &mut h, limit, &c)?;
        Ok(h)
    })
    .await
    .map_err(|er| Fail::Fatal(er.to_string()))?
}

async fn try_once(
    client: &Client,
    url: &Url,
    dest: &Path,
    e: &FileEntry,
    base_done: u64,
    ctl: &Arc<Control>,
    tr: &mut Tracker<'_>,
) -> Result<(), Fail> {
    let part = part_path(dest);
    let mut start = match fs::metadata(&part) {
        Ok(m) if m.is_file() && m.len() <= e.size => m.len(),
        Ok(_) => {
            let _ = fs::remove_file(&part);
            0
        }
        Err(_) => 0,
    };
    tr.p.done = base_done + start;

    let mut hasher = Sha256::new();

    if start < e.size {
        let mut req = client.get(url.clone());
        if start > 0 {
            req = req.header(RANGE, format!("bytes={start}-"));
        }
        let mut resp = tokio::select! {
            r = req.send() => r.map_err(|er| Fail::Retry(net_err(er)))?,
            _ = wait_cancel(ctl) => return Err(Fail::Cancelled),
        };
        let st = resp.status();
        let append = match st {
            StatusCode::PARTIAL_CONTENT if start > 0 => true,
            StatusCode::OK => {
                if start > 0 {
                    start = 0; // server không hỗ trợ Range -> tải lại từ đầu
                    tr.p.done = base_done;
                }
                false
            }
            StatusCode::RANGE_NOT_SATISFIABLE => {
                let _ = fs::remove_file(&part);
                return Err(Fail::Retry("Server từ chối Range, tải lại từ đầu".into()));
            }
            s if s.is_server_error() || s == StatusCode::TOO_MANY_REQUESTS || s == StatusCode::REQUEST_TIMEOUT => {
                return Err(Fail::Retry(format!("Server dữ liệu lỗi tạm thời (HTTP {})", s.as_u16())));
            }
            s => {
                return Err(Fail::Fatal(format!("Không tải được {} (HTTP {})", e.path, s.as_u16())));
            }
        };

        if append {
            hasher = hash_prefix(&part, start, ctl).await?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(append)
            .truncate(!append)
            .open(&part)
            .map_err(|er| Fail::Fatal(format!("Không ghi được {}: {er}", part.display())))?;

        let mut written = start;
        let mut stalled = 0u32;
        loop {
            if ctl.cancelled() {
                return Err(Fail::Cancelled);
            }
            // Chờ tối đa 1 giây mỗi lần để còn kiểm tra nút Huỷ; 30 lần liên tiếp không có dữ liệu = mạng treo.
            let chunk = match tokio::time::timeout(Duration::from_secs(1), resp.chunk()).await {
                Err(_) => {
                    stalled += 1;
                    if stalled >= 30 {
                        return Err(Fail::Retry(format!("Mạng treo khi tải {}", e.path)));
                    }
                    continue;
                }
                Ok(Err(er)) => return Err(Fail::Retry(format!("Mất kết nối khi tải {}: {er}", e.path))),
                Ok(Ok(c)) => c,
            };
            stalled = 0;
            let Some(chunk) = chunk else { break };
            written += chunk.len() as u64;
            if written > e.size {
                drop(file);
                let _ = fs::remove_file(&part);
                return Err(Fail::Fatal(format!("File {} lớn hơn dung lượng khai báo trong manifest", e.path)));
            }
            file.write_all(&chunk)
                .map_err(|er| Fail::Fatal(format!("Lỗi ghi đĩa (hết dung lượng?): {er}")))?;
            hasher.update(&chunk);
            tr.downloaded(chunk.len() as u64);
        }
        file.flush().map_err(|er| Fail::Fatal(format!("Lỗi ghi đĩa: {er}")))?;
        drop(file);
        if written != e.size {
            return Err(Fail::Retry(format!("Kết nối bị ngắt khi tải {}", e.path)));
        }
    } else {
        // .part đã đủ dung lượng từ lần trước (hoặc file rỗng 0 byte): chỉ cần kiểm tra hash
        if !part.exists() {
            File::create(&part).map_err(|er| Fail::Fatal(format!("Không ghi được {}: {er}", part.display())))?;
        }
        hasher = hash_prefix(&part, e.size, ctl).await?;
    }

    if !hex(&hasher.finalize()).eq_ignore_ascii_case(&e.sha256) {
        let _ = fs::remove_file(&part);
        return Err(Fail::Fatal(format!(
            "Sai SHA-256 của {} (file trên server khác manifest, hãy tạo lại manifest)",
            e.path
        )));
    }
    fs::rename(&part, dest).map_err(|er| Fail::Fatal(format!("Không đổi tên {}: {er}", part.display())))?;
    Ok(())
}

async fn wait_cancel(ctl: &Control) {
    while !ctl.cancelled() {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn download_file(
    client: &Client,
    url: &Url,
    dest: &Path,
    e: &FileEntry,
    base_done: u64,
    ctl: &Arc<Control>,
    tr: &mut Tracker<'_>,
) -> Result<(), String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match try_once(client, url, dest, e, base_done, ctl, tr).await {
            Ok(()) => return Ok(()),
            Err(Fail::Cancelled) => return Err(CANCELLED.into()),
            Err(Fail::Fatal(m)) => return Err(m),
            Err(Fail::Retry(m)) => {
                if attempt >= MAX_ATTEMPTS {
                    return Err(format!("{m} (đã thử {MAX_ATTEMPTS} lần)"));
                }
                // chờ 1s, 2s, 3s…, vẫn phản hồi nút Huỷ
                for _ in 0..(attempt * 10) {
                    if ctl.cancelled() {
                        return Err(CANCELLED.into());
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------
// API chính
// ---------------------------------------------------------------------

pub fn find_game_exe(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        (e.file_name().to_string_lossy().eq_ignore_ascii_case("gta_sa.exe") && p.is_file()).then_some(p)
    })
}

/// Tải / cập nhật toàn bộ bộ dữ liệu vào `dir`.
pub async fn install(
    url: &str,
    dir: &Path,
    ctl: Arc<Control>,
    emit: &mut (dyn FnMut(&Progress) + Send),
) -> Result<InstallResult, String> {
    if !dir.is_absolute() {
        return Err("Hãy chọn thư mục cài đặt (đường dẫn đầy đủ)".into());
    }
    let (m, murl) = fetch_manifest(url).await?;
    let total = validate(&m)?;
    let base = base_url(&m, &murl)?;
    fs::create_dir_all(dir).map_err(|e| format!("Không tạo được thư mục {}: {e}", dir.display()))?;
    let client = client()?;

    let mut tr = Tracker::new(emit, total, m.files.len());
    let (mut downloaded, mut skipped) = (0usize, 0usize);

    for (i, e) in m.files.iter().enumerate() {
        if ctl.cancelled() {
            return Err(CANCELLED.into());
        }
        let dest = join_rel(dir, &e.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|er| format!("Không tạo được thư mục {}: {er}", parent.display()))?;
        }
        let base_done = tr.p.done;

        tr.begin_file(i + 1, &e.path, "checking");
        match is_up_to_date(&dest, e, &ctl).await {
            Ok(true) => {
                tr.p.done = base_done + e.size;
                skipped += 1;
                tr.send(false);
                continue;
            }
            Ok(false) => {}
            Err(Fail::Cancelled) => return Err(CANCELLED.into()),
            Err(Fail::Fatal(m)) | Err(Fail::Retry(m)) => return Err(m),
        }

        tr.begin_file(i + 1, &e.path, "downloading");
        let furl = file_url(&base, e)?;
        download_file(&client, &furl, &dest, e, base_done, &ctl, &mut tr).await?;
        tr.p.done = base_done + e.size;
        downloaded += 1;
    }

    tr.p.phase = "done";
    tr.p.file = String::new();
    tr.p.done = total;
    tr.send(true);

    Ok(InstallResult {
        dir: dir.to_string_lossy().into_owned(),
        version: m.version,
        exe: find_game_exe(dir).map(|p| p.to_string_lossy().into_owned()),
        downloaded,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_paths() {
        for ok in ["gta_sa.exe", "models/car.dff", "a b/c d.txt", "data/maps/x.ide"] {
            assert!(valid_rel_path(ok), "{ok}");
        }
        for bad in [
            "", "/etc/passwd", "../x", "a/../b", "a/./b", "C:/x", "a\\b", "a//b", "dir/", "x.part", "con:", "a/b.", "a /b",
            "a?b", "..", ".",
        ] {
            assert!(!valid_rel_path(bad), "{bad}");
        }
    }

    #[test]
    fn urls() {
        assert!(check_url(&Url::parse("https://cdn.example.com/a").unwrap()).is_ok());
        assert!(check_url(&Url::parse("http://localhost:8080/a").unwrap()).is_ok());
        assert!(check_url(&Url::parse("http://127.0.0.1/a").unwrap()).is_ok());
        assert!(check_url(&Url::parse("http://example.com/a").unwrap()).is_err());
        assert!(check_url(&Url::parse("ftp://example.com/a").unwrap()).is_err());
    }

    #[test]
    fn file_url_encodes_segments() {
        let base = Url::parse("https://cdn.example.com/pack/v1/").unwrap();
        let e = FileEntry { path: "a b/c#d?.txt".replace('?', "_"), size: 1, sha256: "0".repeat(64), url: None };
        let u = file_url(&base, &e).unwrap();
        assert_eq!(u.as_str(), "https://cdn.example.com/pack/v1/a%20b/c%23d_.txt");
    }
}
