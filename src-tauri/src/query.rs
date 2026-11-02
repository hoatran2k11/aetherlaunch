//! SA-MP query protocol (UDP): 'i' info, 'r' rules, 'd' detailed players, 'p' ping.
//! Port từ test.py. Header gói: "SAMP" + IPv4(4 byte) + port(u16 LE) + opcode.

use encoding_rs::{Encoding, WINDOWS_1252};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
struct Opts {
    timeout: Duration,
    retries: u32,
}
const FAST: Opts = Opts { timeout: Duration::from_millis(1500), retries: 1 };
/// Nghỉ giữa các gói để không bị server coi là flood.
const PACE: Duration = Duration::from_millis(120);

#[derive(Clone, Copy)]
struct Target {
    addr: SocketAddr,
    ip: [u8; 4],
    port: u16,
}

fn pace() {
    std::thread::sleep(PACE);
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Player {
    pub id: Option<u8>,
    pub name: String,
    pub score: i32,
    pub ping: Option<u32>,
}

/// Kết quả lấy danh sách người chơi. `detailed=false` nghĩa là chỉ có tên + điểm (opcode 'c').
#[derive(Serialize, Debug)]
pub struct PlayerList {
    pub players: Vec<Player>,
    pub detailed: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub ip: String,
    pub port: u16,
    pub password: bool,
    pub players: u16,
    pub max_players: u16,
    pub hostname: String,
    pub mode: String,
    pub language: String,
    pub ping_ms: f64,
    pub rules: BTreeMap<String, String>,
}

// ---------- reader an toàn, không tràn buffer ----------
struct Reader<'a> {
    d: &'a [u8],
    o: usize,
    enc: &'static Encoding,
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8], enc: &'static Encoding) -> Self {
        Self { d, o: 0, enc }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.o.checked_add(n).filter(|e| *e <= self.d.len());
        match end {
            Some(e) => {
                let b = &self.d[self.o..e];
                self.o = e;
                Ok(b)
            }
            None => Err("Gói phản hồi bị cụt".into()),
        }
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn dec(&self, b: &[u8]) -> String {
        self.enc.decode_without_bom_handling(b).0.into_owned()
    }
    fn s8(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        let b = self.take(n)?;
        Ok(self.dec(b))
    }
    fn s32(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        let b = self.take(n)?;
        Ok(self.dec(b))
    }
}

fn resolve(host: &str, port: u16) -> Result<Target, String> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Không phân giải được '{host}': {e}"))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("'{host}' không có địa chỉ IPv4"))?;
    match addr.ip() {
        IpAddr::V4(v4) => Ok(Target { addr, ip: v4.octets(), port }),
        IpAddr::V6(_) => Err("IPv6 không được SA-MP hỗ trợ".into()),
    }
}

/// Gửi 1 gói query; trả (payload sau header 11 byte, rtt).
fn query(t: &Target, op: u8, extra: &[u8], o: Opts) -> Result<(Vec<u8>, Duration), String> {
    let mut pkt = Vec::with_capacity(11 + extra.len());
    pkt.extend_from_slice(b"SAMP");
    pkt.extend_from_slice(&t.ip);
    pkt.extend_from_slice(&t.port.to_le_bytes());
    pkt.push(op);
    pkt.extend_from_slice(extra);

    let mut last = String::from("hết thời gian chờ");
    for _ in 0..=o.retries {
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        sock.set_read_timeout(Some(o.timeout)).ok();
        let t0 = Instant::now();
        if let Err(e) = sock.send_to(&pkt, t.addr) {
            last = e.to_string();
            continue;
        }
        let mut buf = [0u8; 8192];
        match sock.recv_from(&mut buf) {
            Ok((n, _)) => {
                let rtt = t0.elapsed();
                if n < 11 || buf[..11] != pkt[..11] {
                    last = "header phản hồi không khớp".into();
                    continue;
                }
                return Ok((buf[11..n].to_vec(), rtt));
            }
            Err(e) => {
                last = match e.kind() {
                    ErrorKind::TimedOut | ErrorKind::WouldBlock => "hết thời gian chờ".to_string(),
                    _ => e.to_string(),
                }
            }
        }
    }
    Err(format!("Không có phản hồi cho opcode '{}': {last}", op as char))
}

fn ping(t: &Target, o: Opts) -> Result<f64, String> {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x5A4D_5053);
    let tok = n.to_le_bytes();
    let (data, rtt) = query(t, b'p', &tok, o)?;
    if data.get(..4) != Some(&tok[..]) {
        return Err("Ping token không khớp".into());
    }
    Ok(rtt.as_secs_f64() * 1000.0)
}

fn parse_rules(data: &[u8], enc: &'static Encoding) -> Result<BTreeMap<String, String>, String> {
    let mut r = Reader::new(data, enc);
    let n = r.u16()?;
    let mut m = BTreeMap::new();
    for _ in 0..n {
        let k = r.s8()?;
        let v = r.s8()?;
        m.insert(k, v);
    }
    Ok(m)
}

fn parse_detailed(data: &[u8], enc: &'static Encoding) -> Result<Vec<Player>, String> {
    let mut r = Reader::new(data, enc);
    let n = r.u16()?;
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        v.push(Player {
            id: Some(r.u8()?),
            name: r.s8()?,
            score: r.i32()?,
            ping: Some(r.u32()?),
        });
    }
    Ok(v)
}

fn parse_clients(data: &[u8], enc: &'static Encoding) -> Result<Vec<Player>, String> {
    let mut r = Reader::new(data, enc);
    let n = r.u16()?;
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        v.push(Player { id: None, name: r.s8()?, score: r.i32()?, ping: None });
    }
    Ok(v)
}

fn encoding(label: Option<&str>) -> &'static Encoding {
    label
        .and_then(|l| Encoding::for_label(l.as_bytes()))
        .unwrap_or(WINDOWS_1252)
}

/// Thông tin server: info -> ping -> rules, TUẦN TỰ và có nghỉ giữa các gói.
pub fn query_all(host: &str, port: u16, enc_label: Option<&str>) -> Result<ServerStatus, String> {
    if host.is_empty() || port == 0 {
        return Err("Địa chỉ server không hợp lệ".into());
    }
    let enc = encoding(enc_label);
    let t = resolve(host, port)?;

    let (data, info_rtt) = query(&t, b'i', &[], FAST)?;
    let mut r = Reader::new(&data, enc);
    let password = r.u8()? != 0;
    let players = r.u16()?;
    let max_players = r.u16()?;
    let hostname = r.s32()?;
    let mode = r.s32()?;
    let language = r.s32()?;

    pace();
    let ping_ms = ping(&t, FAST).unwrap_or(info_rtt.as_secs_f64() * 1000.0);
    pace();
    let rules = query(&t, b'r', &[], FAST)
        .and_then(|(d, _)| parse_rules(&d, enc))
        .unwrap_or_default();

    Ok(ServerStatus {
        ip: Ipv4(t.ip).to_string(),
        port,
        password,
        players,
        max_players,
        hostname,
        mode,
        language,
        ping_ms,
        rules,
    })
}

/// Danh sách người chơi: thử 'c' (tên + điểm, đúng thứ giao diện cần) trước;
/// nếu server im lặng thì dự phòng 'd'.
pub fn query_players(host: &str, port: u16, enc_label: Option<&str>) -> Result<PlayerList, String> {
    let t = resolve(host, port)?;
    Ok(players_with(&t, encoding(enc_label), FAST, FAST))
}

fn players_with(t: &Target, enc: &'static Encoding, c_opts: Opts, d_opts: Opts) -> PlayerList {
    pace();
    if let Ok(v) = query(t, b'c', &[], c_opts).and_then(|(d, _)| parse_clients(&d, enc)) {
        return PlayerList { players: v, detailed: false, error: None };
    }
    pace();
    match query(t, b'd', &[], d_opts).and_then(|(d, _)| parse_detailed(&d, enc)) {
        Ok(v) => PlayerList { players: v, detailed: true, error: None },
        Err(e) => PlayerList {
            players: vec![],
            detailed: false,
            error: Some(format!(
                "Server không trả danh sách người chơi ({e}). Có thể server đã tắt tính năng này."
            )),
        },
    }
}

struct Ipv4([u8; 4]);
impl std::fmt::Display for Ipv4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn s8(x: &str) -> Vec<u8> {
        let mut v = vec![x.len() as u8];
        v.extend_from_slice(x.as_bytes());
        v
    }
    fn s32(x: &str) -> Vec<u8> {
        let mut v = (x.len() as u32).to_le_bytes().to_vec();
        v.extend_from_slice(x.as_bytes());
        v
    }

    const PL: [(&str, i32); 2] = [("Oliver_Peter", 1), ("nam_cheese", 22)];

    /// Server giả. mode: 0 = trả lời đủ, 1 = im lặng với 'c', 2 = im lặng với cả 'c' và 'd'.
    fn fake(mode: u8) -> u16 {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = s.local_addr().unwrap().port();
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                let Ok((n, a)) = s.recv_from(&mut buf) else { break };
                let d = &buf[..n];
                let mut body: Vec<u8> = vec![];
                match d[10] {
                    b'i' => {
                        body.push(0);
                        body.extend(63u16.to_le_bytes());
                        body.extend(300u16.to_le_bytes());
                        body.extend(s32("GTA Online Viet Nam | GVN"));
                        body.extend(s32("GVN Online"));
                        body.extend(s32("VietNam"));
                    }
                    b'r' => {
                        body.extend(1u16.to_le_bytes());
                        body.extend(s8("mapname"));
                        body.extend(s8("San Fierro"));
                    }
                    b'd' if mode < 2 => {
                        body.extend(2u16.to_le_bytes());
                        for (i, (nm, sc)) in PL.iter().enumerate() {
                            body.push(i as u8);
                            body.extend(s8(nm));
                            body.extend(sc.to_le_bytes());
                            body.extend((40u32 + i as u32).to_le_bytes());
                        }
                    }
                    b'c' if mode == 0 => {
                        body.extend(2u16.to_le_bytes());
                        for (nm, sc) in PL.iter() {
                            body.extend(s8(nm));
                            body.extend(sc.to_le_bytes());
                        }
                    }
                    b'p' => body.extend_from_slice(&d[11..15]),
                    _ => continue,
                }
                let mut out = d[..11].to_vec();
                out.extend(body);
                s.send_to(&out, a).unwrap();
            }
        });
        port
    }

    fn quick() -> Opts {
        Opts { timeout: Duration::from_millis(150), retries: 0 }
    }

    #[test]
    fn status_ok() {
        let st = query_all("127.0.0.1", fake(0), None).unwrap();
        assert_eq!((st.players, st.max_players), (63, 300));
        assert_eq!(st.hostname, "GTA Online Viet Nam | GVN");
        assert_eq!(st.language, "VietNam");
        assert_eq!(st.rules["mapname"], "San Fierro");
        assert!(st.ping_ms >= 0.0);
    }

    #[test]
    fn players_names_and_scores_from_c() {
        let p = query_players("127.0.0.1", fake(0), None).unwrap();
        assert!(!p.detailed && p.error.is_none());
        assert_eq!(p.players.len(), 2);
        assert_eq!((p.players[1].name.as_str(), p.players[1].score), ("nam_cheese", 22));
    }

    #[test]
    fn players_fall_back_to_d_when_c_is_silent() {
        let t = resolve("127.0.0.1", fake(1)).unwrap();
        let p = players_with(&t, WINDOWS_1252, quick(), quick());
        assert!(p.detailed && p.error.is_none());
        assert_eq!(
            p.players[1],
            Player { id: Some(1), name: "nam_cheese".into(), score: 22, ping: Some(41) }
        );
    }

    #[test]
    fn players_error_is_friendly_when_both_silent() {
        let t = resolve("127.0.0.1", fake(2)).unwrap();
        let p = players_with(&t, WINDOWS_1252, quick(), quick());
        assert!(p.players.is_empty());
        let e = p.error.unwrap();
        assert!(e.contains("hết thời gian chờ") && !e.contains("os error"), "{e}");
    }

    #[test]
    fn truncated_packet_errors() {
        let mut r = Reader::new(&[1, 0], WINDOWS_1252);
        assert!(r.u32().is_err());
    }
}
