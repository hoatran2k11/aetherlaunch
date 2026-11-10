//! Discord Rich Presence qua IPC cục bộ (ứng dụng Discord desktop phải đang chạy).
//!
//! - Mọi thao tác IPC chạy trên MỘT LUỒNG RIÊNG: giao diện không bao giờ bị treo dù Discord chậm / không phản hồi.
//! - Các cập nhật dồn dập được gộp, chỉ gửi bản mới nhất.
//! - Có đọc phản hồi của Discord: biết được lỗi (ví dụ link nút sai) và tự kết nối lại khi Discord khởi động lại.
//! - Cấu hình lấy từ biến môi trường / `.env` (xem README): DISCORD_CLIENT_ID, DISCORD_LARGE_IMAGE, ...

use crate::forum;
use discord_rich_presence::{
    activity::{Activity, Assets, Button, Party, Timestamps},
    DiscordIpc, DiscordIpcClient,
};
use serde::{Deserialize, Serialize};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

// =====================================================================
// Cấu hình (.env)
// =====================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RpcConfig {
    pub client_id: String,
    pub large_image: String,
    pub small_image: String,
    pub large_text: String,
}

/// Giá trị được nhúng lúc build (build.rs đọc `.env`). Dùng khi lúc chạy không có biến môi trường tương ứng.
fn builtin(key: &str) -> Option<&'static str> {
    match key {
        "DISCORD_CLIENT_ID" => option_env!("DISCORD_CLIENT_ID"),
        "DISCORD_LARGE_IMAGE" => option_env!("DISCORD_LARGE_IMAGE"),
        "DISCORD_SMALL_IMAGE" => option_env!("DISCORD_SMALL_IMAGE"),
        "DISCORD_LARGE_TEXT" => option_env!("DISCORD_LARGE_TEXT"),
        _ => None,
    }
}

impl RpcConfig {
    /// Ưu tiên: biến môi trường thật / `.env` nạp lúc chạy > giá trị nhúng lúc build.
    pub fn load() -> Result<RpcConfig, String> {
        Self::from_lookup(|k| std::env::var(k).ok().or_else(|| builtin(k).map(String::from)))
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<RpcConfig, String> {
        let val = |k: &str| get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let client_id = val("DISCORD_CLIENT_ID")
            .ok_or_else(|| "Chưa cấu hình DISCORD_CLIENT_ID (xem README, mục Discord Rich Presence)".to_string())?;
        if !(17..=20).contains(&client_id.len()) || !client_id.chars().all(|c| c.is_ascii_digit()) {
            return Err("DISCORD_CLIENT_ID không hợp lệ – phải là dãy 17–20 chữ số (Application ID trong Developer Portal)".into());
        }
        Ok(RpcConfig {
            client_id,
            large_image: val("DISCORD_LARGE_IMAGE").unwrap_or_else(|| "aether_logo".into()),
            small_image: val("DISCORD_SMALL_IMAGE").unwrap_or_else(|| "gta_sa".into()),
            large_text: val("DISCORD_LARGE_TEXT")
                .unwrap_or_else(|| format!("AetherLaunch v{}", env!("CARGO_PKG_VERSION"))),
        })
    }
}

// =====================================================================
// Dữ liệu từ giao diện + làm sạch theo giới hạn của Discord
// =====================================================================
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ActivityPayload {
    pub details: String,
    pub state: String,
    pub start_timestamp: Option<i64>,
    pub party_current: Option<i64>,
    pub party_max: Option<i64>,
    pub party_id: Option<String>,
    pub small_text: Option<String>,
    pub buttons: Vec<ButtonPayload>,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct ButtonPayload {
    pub label: String,
    pub url: String,
}

/// Gộp khoảng trắng, cắt theo BYTE (giới hạn của Discord tính theo byte ở một số trường) tại ranh giới ký tự.
fn clip_bytes(s: &str, max: usize, min_chars: usize) -> Option<String> {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() < min_chars {
        return None;
    }
    if s.len() <= max {
        return Some(s);
    }
    let mut out = String::new();
    for c in s.chars() {
        if out.len() + c.len_utf8() > max.saturating_sub(3) {
            break;
        }
        out.push(c);
    }
    out.push('…');
    Some(out)
}

#[derive(Debug, PartialEq)]
struct Prepared {
    details: Option<String>,
    state: Option<String>,
    start: Option<i64>,
    party: Option<([i32; 2], String)>,
    small_text: Option<String>,
    buttons: Vec<(String, String)>,
}

fn prepare(p: &ActivityPayload) -> Prepared {
    // Discord: details/state dài 2–128; nút tối đa 2 (label ≤ 32, url ≤ 512, http/https)
    let mut buttons: Vec<(String, String)> = p
        .buttons
        .iter()
        .filter_map(|b| {
            let label = clip_bytes(&b.label, 32, 1)?;
            let url = forum::validate_url(&b.url).ok().filter(|u| u.len() <= 512)?;
            Some((label, url))
        })
        .collect();
    buttons.truncate(2);

    let party = match (p.party_current, p.party_max) {
        (Some(c), Some(m)) if c >= 1 && m >= c && m <= i32::MAX as i64 => {
            let id = p
                .party_id
                .as_deref()
                .and_then(|s| clip_bytes(s, 128, 1))
                .unwrap_or_else(|| "aether".into());
            Some(([c as i32, m as i32], id))
        }
        _ => None,
    };

    Prepared {
        details: clip_bytes(&p.details, 128, 2),
        state: clip_bytes(&p.state, 128, 2),
        start: p.start_timestamp.filter(|t| *t > 0),
        party,
        small_text: p.small_text.as_deref().and_then(|s| clip_bytes(s, 128, 2)),
        buttons,
    }
}

fn build<'a>(pr: &'a Prepared, cfg: &'a RpcConfig) -> Activity<'a> {
    let mut a = Activity::new();
    if let Some(d) = &pr.details {
        a = a.details(d.as_str());
    }
    if let Some(s) = &pr.state {
        a = a.state(s.as_str());
    }
    if let Some(t) = pr.start {
        a = a.timestamps(Timestamps::new().start(t));
    }
    if let Some((size, id)) = &pr.party {
        a = a.party(Party::new().id(id.as_str()).size(*size));
    }
    let mut assets = Assets::new()
        .large_image(cfg.large_image.as_str())
        .large_text(cfg.large_text.as_str());
    if !cfg.small_image.is_empty() {
        assets = assets.small_image(cfg.small_image.as_str());
        if let Some(t) = &pr.small_text {
            assets = assets.small_text(t.as_str());
        }
    }
    a = a.assets(assets);
    if !pr.buttons.is_empty() {
        a = a.buttons(pr.buttons.iter().map(|(l, u)| Button::new(l.as_str(), u.as_str())).collect());
    }
    a
}

// =====================================================================
// Luồng IPC
// =====================================================================
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RpcInfo {
    pub configured: bool,
    pub connected: bool,
    pub error: Option<String>,
    /// Nhãn các nút launcher THỰC SỰ đã gửi cho Discord ở lần cập nhật gần nhất
    pub sent_buttons: Vec<String>,
}

enum Msg {
    Set(ActivityPayload),
    Clear,
    Info(mpsc::SyncSender<RpcInfo>),
}

pub struct Rpc {
    tx: Option<Mutex<mpsc::Sender<Msg>>>,
    shared: Arc<Mutex<RpcInfo>>,
}

impl Rpc {
    pub fn new(cfg: Result<RpcConfig, String>) -> Rpc {
        match cfg {
            Err(e) => Rpc {
                tx: None,
                shared: Arc::new(Mutex::new(RpcInfo { configured: false, connected: false, error: Some(e), sent_buttons: vec![] })),
            },
            Ok(cfg) => {
                let shared = Arc::new(Mutex::new(RpcInfo { configured: true, ..Default::default() }));
                let (tx, rx) = mpsc::channel();
                let s = shared.clone();
                std::thread::Builder::new()
                    .name("discord-rpc".into())
                    .spawn(move || worker(cfg, rx, s))
                    .expect("không tạo được luồng Discord RPC");
                Rpc { tx: Some(Mutex::new(tx)), shared }
            }
        }
    }

    fn send(&self, m: Msg) -> Result<(), String> {
        match &self.tx {
            None => Err(self.snapshot().error.unwrap_or_else(|| "Discord RPC chưa cấu hình".into())),
            Some(tx) => tx.lock().map_err(|_| "lock".to_string())?.send(m).map_err(|_| "Luồng Discord RPC đã dừng".to_string()),
        }
    }
    fn snapshot(&self) -> RpcInfo {
        self.shared.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Đặt trạng thái (không chờ kết quả; luôn trả về ngay).
    pub fn set(&self, p: ActivityPayload) -> Result<(), String> {
        self.send(Msg::Set(p))
    }
    /// Xoá trạng thái đang hiển thị.
    pub fn clear(&self) -> Result<(), String> {
        self.send(Msg::Clear)
    }
    /// Trạng thái kết nối. `connect = true` sẽ thử kết nối ngay (chờ tối đa ~2 giây).
    pub fn info(&self, connect: bool) -> RpcInfo {
        if connect && self.tx.is_some() {
            let (rtx, rrx) = mpsc::sync_channel(1);
            if self.send(Msg::Info(rtx)).is_ok() {
                if let Ok(i) = rrx.recv_timeout(Duration::from_secs(2)) {
                    return i;
                }
            }
        }
        self.snapshot()
    }
}

fn update(shared: &Arc<Mutex<RpcInfo>>, f: impl FnOnce(&mut RpcInfo)) {
    if let Ok(mut g) = shared.lock() {
        f(&mut g);
    }
}

fn ensure_connected(cfg: &RpcConfig, client: &mut Option<DiscordIpcClient>, shared: &Arc<Mutex<RpcInfo>>) -> bool {
    if client.is_some() {
        return true;
    }
    let mut c = DiscordIpcClient::new(&cfg.client_id);
    match c.connect() {
        Ok(()) => {
            *client = Some(c);
            update(shared, |i| {
                i.connected = true;
                i.error = None;
            });
            true
        }
        Err(_) => {
            update(shared, |i| {
                i.connected = false;
                i.error = Some("Chưa thấy Discord đang chạy – hãy mở ứng dụng Discord".into());
            });
            false
        }
    }
}

fn apply(cfg: &RpcConfig, client: &mut Option<DiscordIpcClient>, shared: &Arc<Mutex<RpcInfo>>, act: &Option<ActivityPayload>) {
    for attempt in 0..2 {
        if !ensure_connected(cfg, client, shared) {
            return;
        }
        let c = client.as_mut().expect("vừa kết nối");
        let mut labels: Vec<String> = vec![];
        let sent = match act {
            Some(p) => {
                let pr = prepare(p);
                labels = pr.buttons.iter().map(|(l, _)| l.clone()).collect();
                c.set_activity(build(&pr, cfg))
            }
            None => c.clear_activity(),
        };
        // Đọc phản hồi: vừa dọn ống IPC, vừa biết Discord có từ chối không
        match sent.and_then(|_| c.recv()) {
            Ok((_, v)) => {
                let err = (v["evt"] == "ERROR").then(|| {
                    let m = v["data"]["message"].as_str().unwrap_or("lỗi không rõ");
                    format!("Discord từ chối trạng thái: {m}")
                });
                update(shared, |i| {
                    i.connected = true;
                    i.error = err;
                    i.sent_buttons = labels;
                });
                return;
            }
            Err(_) => {
                // Discord đã đóng / khởi động lại: bỏ kết nối cũ và thử lại một lần
                *client = None;
                update(shared, |i| {
                    i.connected = false;
                    if attempt == 1 {
                        i.error = Some("Mất kết nối với Discord".into());
                    }
                });
            }
        }
    }
}

fn worker(cfg: RpcConfig, rx: mpsc::Receiver<Msg>, shared: Arc<Mutex<RpcInfo>>) {
    let mut client: Option<DiscordIpcClient> = None;
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        while let Ok(m) = rx.try_recv() {
            batch.push(m); // gộp các lệnh dồn lại
        }
        let mut act: Option<Option<ActivityPayload>> = None;
        let mut infos = vec![];
        for m in batch {
            match m {
                Msg::Set(p) => act = Some(Some(p)),
                Msg::Clear => act = Some(None),
                Msg::Info(tx) => infos.push(tx),
            }
        }
        match &act {
            Some(a) => apply(&cfg, &mut client, &shared, a),
            None => {
                ensure_connected(&cfg, &mut client, &shared);
            }
        }
        let snap = shared.lock().map(|g| g.clone()).unwrap_or_default();
        for tx in infos {
            let _ = tx.send(snap.clone());
        }
    }
}

// =====================================================================
// Test (Discord giả qua unix socket; chỉ chạy trên Linux/macOS)
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(pairs: &[(&str, &str)]) -> Result<RpcConfig, String> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        RpcConfig::from_lookup(move |k| pairs.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone()))
    }
    const ID: &str = "123456789012345678";

    #[test]
    fn config_requires_valid_client_id() {
        assert!(cfg_with(&[]).unwrap_err().contains("DISCORD_CLIENT_ID"));
        assert!(cfg_with(&[("DISCORD_CLIENT_ID", "  ")]).is_err());
        assert!(cfg_with(&[("DISCORD_CLIENT_ID", "your_application_id")]).unwrap_err().contains("không hợp lệ"));
        assert!(cfg_with(&[("DISCORD_CLIENT_ID", "12345")]).is_err());
        let c = cfg_with(&[("DISCORD_CLIENT_ID", ID)]).unwrap();
        assert_eq!((c.large_image.as_str(), c.small_image.as_str()), ("aether_logo", "gta_sa"));
        assert!(c.large_text.starts_with("AetherLaunch v"));
    }

    #[test]
    fn buttons_are_validated_and_capped_at_two() {
        let btn = |l: &str, u: &str| ButtonPayload { label: l.into(), url: u.into() };
        let p = ActivityPayload {
            buttons: vec![
                btn("Created by Max Bennett", "https://guns.lol/aoh20119"),
                btn("Xấu", "javascript:alert(1)"),
                btn("", "https://empty-label.example"),
                btn("Thứ hai", "https://b.example/"),
                btn("Thứ ba", "https://c.example/"),
            ],
            ..Default::default()
        };
        let pr = prepare(&p);
        // link xấu và nhãn rỗng bị loại; tối đa 2 nút
        assert_eq!(pr.buttons.len(), 2);
        assert_eq!(pr.buttons[0], ("Created by Max Bennett".to_string(), "https://guns.lol/aoh20119".to_string()));
        assert_eq!(pr.buttons[1].0, "Thứ hai");
        // không có nút nào được tự thêm từ cấu hình
        assert!(prepare(&ActivityPayload::default()).buttons.is_empty());
        // nhãn dài bị cắt theo byte (≤ 32)
        let long = prepare(&ActivityPayload { buttons: vec![btn(&"Đ".repeat(40), "https://a.b")], ..Default::default() });
        assert!(long.buttons[0].0.len() <= 32);
    }

    #[test]
    fn clip_respects_byte_limits_and_char_boundaries() {
        assert_eq!(clip_bytes("  a   b  ", 128, 2).as_deref(), Some("a b"));
        assert_eq!(clip_bytes("x", 128, 2), None);
        let long = "Đ".repeat(100); // 2 byte / ký tự
        let c = clip_bytes(&long, 128, 2).unwrap();
        assert!(c.len() <= 128 && c.ends_with('…') && c.is_char_boundary(c.len()));
        assert_eq!(clip_bytes(&"a".repeat(40), 32, 1).unwrap().len(), 29 + 3);
    }

    #[test]
    fn prepare_applies_discord_limits() {
        let p = ActivityPayload {
            details: "a".into(), // quá ngắn -> bỏ
            state: "Nhân vật: Max_Bennett".into(),
            start_timestamp: Some(1_700_000_000),
            party_current: Some(0), // 0 người -> bỏ party
            party_max: Some(300),
            buttons: vec![
                ButtonPayload { label: "Forum".into(), url: "https://f.example/".into() },
                ButtonPayload { label: "Xấu".into(), url: "javascript:alert(1)".into() },
                ButtonPayload { label: "Thứ ba".into(), url: "https://g.example/".into() },
            ],
            ..Default::default()
        };
        let pr = prepare(&p);
        assert_eq!(pr.details, None);
        assert_eq!(pr.state.as_deref(), Some("Nhân vật: Max_Bennett"));
        assert_eq!(pr.party, None);
        // link xấu bị loại; tối đa 2 nút (phần dư bị cắt)
        assert_eq!(pr.buttons.len(), 2);
        assert_eq!(pr.buttons[0].0, "Forum");
        assert_eq!(pr.buttons[1].0, "Thứ ba");
        let p2 = ActivityPayload { party_current: Some(63), party_max: Some(300), party_id: Some("aether:h:7777".into()), ..Default::default() };
        assert_eq!(prepare(&p2).party, Some(([63, 300], "aether:h:7777".into())));
        let p3 = ActivityPayload { party_current: Some(301), party_max: Some(300), ..Default::default() };
        assert_eq!(prepare(&p3).party, None);
    }

    #[test]
    fn unconfigured_rpc_reports_and_rejects() {
        let rpc = Rpc::new(Err("thiếu id".into()));
        assert_eq!(rpc.set(ActivityPayload::default()).unwrap_err(), "thiếu id");
        let i = rpc.info(true);
        assert!(!i.configured && !i.connected && i.error.as_deref() == Some("thiếu id"));
    }

    #[cfg(unix)]
    mod ipc {
        use super::*;
        use serde_json::{json, Value};
        use std::io::{Read, Write};
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering::SeqCst};
        use std::time::Instant;

        fn read_frame(s: &mut UnixStream) -> Option<(u32, Value)> {
            let mut h = [0u8; 8];
            s.read_exact(&mut h).ok()?;
            let (op, len) = (u32::from_le_bytes(h[..4].try_into().unwrap()), u32::from_le_bytes(h[4..].try_into().unwrap()));
            let mut b = vec![0u8; len as usize];
            s.read_exact(&mut b).ok()?;
            Some((op, serde_json::from_slice(&b).ok()?))
        }
        fn write_frame(s: &mut UnixStream, op: u32, v: &Value) {
            let b = v.to_string();
            let mut out = op.to_le_bytes().to_vec();
            out.extend((b.len() as u32).to_le_bytes());
            out.extend(b.as_bytes());
            let _ = s.write_all(&out);
        }
        fn wait_for(mut f: impl FnMut() -> bool) -> bool {
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(3) {
                if f() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            false
        }

        struct Mock {
            got: Arc<Mutex<Vec<Value>>>,
            conns: Arc<AtomicUsize>,
            mode: Arc<AtomicU8>, // 0 ok, 1 trả ERROR
            kill: Arc<AtomicBool>,
        }
        fn start_mock(dir: &std::path::Path) -> Mock {
            let _ = std::fs::remove_file(dir.join("discord-ipc-0"));
            let l = UnixListener::bind(dir.join("discord-ipc-0")).unwrap();
            let m = Mock { got: Default::default(), conns: Default::default(), mode: Default::default(), kill: Default::default() };
            let (got, conns, mode, kill) = (m.got.clone(), m.conns.clone(), m.mode.clone(), m.kill.clone());
            std::thread::spawn(move || {
                for s in l.incoming() {
                    let Ok(mut s) = s else { break };
                    conns.fetch_add(1, SeqCst);
                    let (got, mode, kill) = (got.clone(), mode.clone(), kill.clone());
                    std::thread::spawn(move || {
                        while let Some((op, v)) = read_frame(&mut s) {
                            if op == 0 {
                                assert_eq!(v["client_id"], "123456789012345678");
                                write_frame(&mut s, 1, &json!({"cmd":"DISPATCH","evt":"READY","data":{}}));
                            } else if op == 1 {
                                if kill.swap(false, SeqCst) {
                                    let _ = s.shutdown(std::net::Shutdown::Both);
                                    return;
                                }
                                got.lock().unwrap().push(v["args"]["activity"].clone());
                                if mode.load(SeqCst) == 1 {
                                    write_frame(&mut s, 1, &json!({"cmd":"SET_ACTIVITY","evt":"ERROR","data":{"code":4000,"message":"Invalid asset"},"nonce":v["nonce"]}));
                                } else {
                                    write_frame(&mut s, 1, &json!({"cmd":"SET_ACTIVITY","evt":null,"data":{},"nonce":v["nonce"]}));
                                }
                            }
                        }
                    });
                }
            });
            m
        }

        #[test]
        fn rpc_end_to_end_with_fake_discord() {
            let dir = std::env::temp_dir().join(format!("aether_rpc_{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::env::set_var("XDG_RUNTIME_DIR", &dir);

            // 0) Discord chưa chạy
            let cfg = cfg_with(&[("DISCORD_CLIENT_ID", ID)]).unwrap();
            let rpc = Rpc::new(Ok(cfg));
            let i = rpc.info(true);
            assert!(i.configured && !i.connected && i.error.as_deref().unwrap().contains("Chưa thấy Discord"), "{i:?}");

            // Discord bật lên
            let m = start_mock(&dir);
            let payload = ActivityPayload {
                details: "Đang chơi GTA Viet Nam Online | GVN".into(),
                state: "Nhân vật: Max_Bennett".into(),
                start_timestamp: Some(1_700_000_000),
                party_current: Some(63),
                party_max: Some(300),
                party_id: Some("aether:sv.gtaviet.net:7777".into()),
                small_text: Some("sv.gtaviet.net:7777".into()),
                buttons: vec![
                    ButtonPayload { label: "Created by Max Bennett".into(), url: "https://guns.lol/aoh20119".into() },
                    ButtonPayload { label: "Xấu".into(), url: "javascript:alert(1)".into() },
                ],
            };

            // 1) Gửi đủ thông tin
            rpc.set(payload.clone()).unwrap();
            assert!(wait_for(|| m.got.lock().unwrap().len() == 1), "không nhận được activity");
            let a = m.got.lock().unwrap()[0].clone();
            assert_eq!(a["details"], "Đang chơi GTA Viet Nam Online | GVN");
            assert_eq!(a["state"], "Nhân vật: Max_Bennett");
            assert_eq!(a["timestamps"]["start"], 1_700_000_000);
            assert_eq!(a["party"]["size"], json!([63, 300]));
            assert_eq!(a["party"]["id"], "aether:sv.gtaviet.net:7777");
            assert_eq!(a["assets"]["large_image"], "aether_logo");
            assert_eq!(a["assets"]["small_image"], "gta_sa");
            assert_eq!(a["assets"]["small_text"], "sv.gtaviet.net:7777");
            assert!(a["assets"]["large_text"].as_str().unwrap().starts_with("AetherLaunch v"));
            assert_eq!(a["buttons"], json!([{"label":"Created by Max Bennett","url":"https://guns.lol/aoh20119"}]));
            assert!(wait_for(|| rpc.info(false).connected));
            // launcher báo lại đúng những nút đã gửi cho Discord
            assert!(wait_for(|| rpc.info(false).sent_buttons == ["Created by Max Bennett"]), "{:?}", rpc.info(false));

            // 2) Gộp lệnh dồn dập: bản cuối cùng luôn tới nơi
            for n in 1..=5 {
                let mut p = payload.clone();
                p.party_current = Some(n);
                rpc.set(p).unwrap();
            }
            assert!(wait_for(|| m.got.lock().unwrap().last().map(|a| a["party"]["size"] == json!([5, 300])).unwrap_or(false)));

            // 3) Xoá trạng thái
            rpc.clear().unwrap();
            assert!(wait_for(|| m.got.lock().unwrap().last().map(|a| a.is_null()).unwrap_or(false)));

            // 4) Discord từ chối -> báo lỗi rõ ràng, sau đó thành công thì hết lỗi
            m.mode.store(1, SeqCst);
            rpc.set(payload.clone()).unwrap();
            assert!(wait_for(|| rpc.info(false).error.map(|e| e.contains("Invalid asset")).unwrap_or(false)));
            m.mode.store(0, SeqCst);
            rpc.set(payload.clone()).unwrap();
            assert!(wait_for(|| rpc.info(false).error.is_none()));

            // 5) Discord khởi động lại (đóng kết nối) -> tự kết nối lại và vẫn gửi được
            let before = m.conns.load(SeqCst);
            let n_before = m.got.lock().unwrap().len();
            m.kill.store(true, SeqCst);
            let mut p = payload.clone();
            p.state = "Sau khi Discord restart".into();
            rpc.set(p).unwrap();
            assert!(wait_for(|| m.got.lock().unwrap().len() > n_before), "không gửi lại sau khi mất kết nối");
            assert_eq!(m.got.lock().unwrap().last().unwrap()["state"], "Sau khi Discord restart");
            assert_eq!(m.conns.load(SeqCst), before + 1, "phải tạo đúng 1 kết nối mới");
            assert!(rpc.info(false).connected);
        }
    }
}
