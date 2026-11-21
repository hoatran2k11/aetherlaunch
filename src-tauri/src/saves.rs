//! Quản lý file save của chế độ chơi offline (GTA:SA Single-player).
//!
//! GTA:SA PC lưu tối đa 8 slot: `GTASAsf1.b` … `GTASAsf8.b` trong
//! `Documents\GTA San Andreas User Files`. Mỗi file đúng 0x31800 byte, bắt đầu bằng "BLOCK" và
//! kết thúc bằng checksum 4 byte (tổng các byte phía trước, little-endian) – game từ chối save sai checksum.
//!
//! Nguyên tắc an toàn:
//! - Chỉ chạm vào đúng 8 tên file trên + các thư mục sao lưu do module này tạo; không nhận tên file từ người dùng.
//! - Trước mọi thao tác có thể làm mất save (khôi phục, nhập đè, xoá) đều tự tạo bản sao lưu "safety".
//! - Ghi file bằng cách ghi tạm rồi đổi tên (không bao giờ để save nửa vời).
//! - Nhập file chỉ nhận save hợp lệ (đúng dung lượng, "BLOCK", đúng checksum).
//!
//! Module không phụ thuộc Tauri để kiểm thử độc lập; chặn thao tác khi game đang chạy nằm ở `main.rs`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SLOTS: u8 = 8;
pub const SAVE_SIZE: usize = 0x31800;
const MAX_IMPORT: u64 = 1 << 20;
/// "BLOCK" (5 byte) + version dword (4 byte) rồi tới tên save char[100] (theo mô tả trên Grand Theft Wiki)
const NAME_OFFSET: usize = 9;
const NAME_BYTES: usize = 100;
const META: &str = "meta.json";

// ---------------------------------------------------------------------
// Kiểu dữ liệu trả về frontend
// ---------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub slot: u8,
    pub exists: bool,
    /// File đúng chuẩn save GTA:SA (đúng dung lượng, "BLOCK", đúng checksum)
    pub valid: bool,
    /// Lý do không hợp lệ (rỗng nếu hợp lệ)
    pub problem: String,
    /// Tên hiển thị trong game (đọc theo kiểu best-effort, có thể rỗng)
    pub name: String,
    pub size: u64,
    pub modified_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Meta {
    label: String,
    /// "manual" | "auto" | "safety"
    kind: String,
    created_ms: u64,
    slots: Vec<u8>,
    /// SHA-256 nội dung các slot lúc sao lưu, dùng để khỏi sao lưu trùng
    fingerprint: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub created_ms: u64,
    pub slots: Vec<u8>,
    pub size: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored: Vec<u8>,
}

// ---------------------------------------------------------------------
// Kiểm tra / đọc file save
// ---------------------------------------------------------------------

pub fn slot_file(slot: u8) -> Result<String, String> {
    if (1..=SLOTS).contains(&slot) {
        Ok(format!("GTASAsf{slot}.b"))
    } else {
        Err(format!("Slot phải từ 1 đến {SLOTS}"))
    }
}

fn slot_path(dir: &Path, slot: u8) -> Result<PathBuf, String> {
    Ok(dir.join(slot_file(slot)?))
}

pub fn validate_save(data: &[u8]) -> Result<(), String> {
    if data.len() != SAVE_SIZE {
        return Err(format!("Dung lượng {} byte, file save GTA:SA phải đúng {SAVE_SIZE} byte", data.len()));
    }
    if &data[..5] != b"BLOCK" {
        return Err("Không phải file save GTA:SA (thiếu dấu hiệu BLOCK)".into());
    }
    let sum = data[..SAVE_SIZE - 4].iter().fold(0u32, |a, &b| a.wrapping_add(b as u32));
    let stored = u32::from_le_bytes(data[SAVE_SIZE - 4..].try_into().unwrap());
    if sum != stored {
        return Err("Sai checksum – file save bị hỏng hoặc đã bị sửa không đúng cách".into());
    }
    Ok(())
}

/// Đọc tên save: 100 byte sau header, thường là UTF-16LE, nếu không thì coi là 1 byte/ký tự.
/// Chỉ để hiển thị – sai/trống thì UI dùng "Slot N".
pub fn save_name(data: &[u8]) -> String {
    let Some(raw) = data.get(NAME_OFFSET..NAME_OFFSET + NAME_BYTES) else { return String::new() };
    let wide = raw.len() >= 4 && raw[0] != 0 && raw[1] == 0 && raw[2] != 0 && raw[3] == 0;
    let s: String = if wide {
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
        String::from_utf16_lossy(&units)
    } else {
        raw.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
    };
    let clean: String = s.chars().filter(|c| !c.is_control()).collect();
    let clean = clean.trim();
    if clean.is_empty() || clean.contains('\u{FFFD}') {
        return String::new();
    }
    clean.chars().take(50).collect()
}

fn modified_ms(m: &fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn list_slots(dir: &Path) -> Vec<Slot> {
    (1..=SLOTS)
        .map(|n| {
            let mut s = Slot { slot: n, ..Default::default() };
            let Ok(p) = slot_path(dir, n) else { return s };
            let Ok(m) = fs::metadata(&p) else { return s };
            if !m.is_file() {
                return s;
            }
            s.exists = true;
            s.size = m.len();
            s.modified_ms = modified_ms(&m);
            if m.len() > MAX_IMPORT {
                s.problem = "File quá lớn so với một save GTA:SA".into();
                return s;
            }
            match fs::read(&p) {
                Ok(data) => match validate_save(&data) {
                    Ok(()) => {
                        s.valid = true;
                        s.name = save_name(&data);
                    }
                    Err(e) => s.problem = e,
                },
                Err(e) => s.problem = format!("Không đọc được file: {e}"),
            }
            s
        })
        .collect()
}

// ---------------------------------------------------------------------
// Thời gian + id sao lưu
// ---------------------------------------------------------------------

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Giây Unix -> (năm, tháng, ngày, giờ, phút, giây) theo UTC (thuật toán civil-from-days của H. Hinnant).
fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, (rem % 3600 / 60) as u32, (rem % 60) as u32)
}

fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    (15..=24).contains(&b.len()) && b[8] == b'-' && b.iter().all(|c| c.is_ascii_digit() || *c == b'-') && b[..8].iter().all(|c| c.is_ascii_digit())
}

fn unique_id(root: &Path, ms: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(ms / 1000);
    let base = format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}");
    let mut id = base.clone();
    let mut n = 1;
    while root.join(&id).exists() {
        id = format!("{base}-{n:03}");
        n += 1;
    }
    id
}

fn clean_label(l: &str) -> String {
    let t: String = l.chars().filter(|c| !c.is_control()).collect();
    t.trim().chars().take(80).collect()
}

// ---------------------------------------------------------------------
// Sao lưu
// ---------------------------------------------------------------------

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("b.tmp-aether");
    let r = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    r.map_err(|e| format!("Không ghi được {}: {e}", path.display()))
}

/// Hash nội dung tất cả slot đang có (rỗng nếu chưa có save nào).
fn fingerprint(dir: &Path) -> String {
    let mut h = Sha256::new();
    let mut any = false;
    for n in 1..=SLOTS {
        if let Ok(data) = slot_path(dir, n).and_then(|p| fs::read(p).map_err(|e| e.to_string())) {
            any = true;
            h.update([n]);
            h.update((data.len() as u64).to_le_bytes());
            h.update(&data);
        }
    }
    if !any {
        return String::new();
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn read_meta(root: &Path, id: &str) -> Option<Meta> {
    serde_json::from_slice(&fs::read(root.join(id).join(META)).ok()?).ok()
}

pub fn list_backups(root: &Path) -> Vec<BackupInfo> {
    let Ok(rd) = fs::read_dir(root) else { return vec![] };
    let mut v: Vec<BackupInfo> = rd
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            if !valid_id(&id) || !e.path().is_dir() {
                return None;
            }
            let m = read_meta(root, &id)?;
            let size = m.slots.iter().filter_map(|&n| fs::metadata(e.path().join(slot_file(n).ok()?)).ok()).map(|x| x.len()).sum();
            Some(BackupInfo { id, label: m.label, kind: m.kind, created_ms: m.created_ms, slots: m.slots, size })
        })
        .collect();
    v.sort_by(|a, b| b.created_ms.cmp(&a.created_ms).then_with(|| b.id.cmp(&a.id)));
    v
}

pub fn create_backup(dir: &Path, root: &Path, label: &str, kind: &str) -> Result<Option<BackupInfo>, String> {
    if !matches!(kind, "manual" | "auto" | "safety") {
        return Err("Loại sao lưu không hợp lệ".into());
    }
    let present: Vec<u8> = (1..=SLOTS).filter(|&n| slot_path(dir, n).map(|p| p.is_file()).unwrap_or(false)).collect();
    if present.is_empty() {
        return Ok(None);
    }
    fs::create_dir_all(root).map_err(|e| format!("Không tạo được thư mục sao lưu: {e}"))?;
    let created = now_ms();
    let id = unique_id(root, created);
    let dest = root.join(&id);
    let r = (|| -> Result<(), String> {
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        for &n in &present {
            let data = fs::read(slot_path(dir, n)?).map_err(|e| format!("Không đọc được slot {n}: {e}"))?;
            fs::write(dest.join(slot_file(n)?), data).map_err(|e| format!("Không ghi được bản sao lưu: {e}"))?;
        }
        let meta = Meta { label: clean_label(label), kind: kind.into(), created_ms: created, slots: present.clone(), fingerprint: fingerprint(dir) };
        // meta.json ghi cuối: bản sao lưu dang dở (thiếu meta) không bao giờ hiện trong danh sách
        fs::write(dest.join(META), serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    })();
    if let Err(e) = r {
        let _ = fs::remove_dir_all(&dest);
        return Err(format!("Sao lưu thất bại: {e}"));
    }
    Ok(list_backups(root).into_iter().find(|b| b.id == id))
}

/// Giữ tối đa `keep` bản tự động / an toàn mới nhất (bản thủ công không bao giờ bị xoá), trừ `protect`.
fn prune(root: &Path, keep: usize, protect: Option<&str>) {
    let rest: Vec<BackupInfo> = list_backups(root).into_iter().filter(|b| b.kind != "manual").skip(keep).collect();
    for b in rest {
        if Some(b.id.as_str()) != protect && valid_id(&b.id) {
            let _ = fs::remove_dir_all(root.join(&b.id));
        }
    }
}

/// Sao lưu tự động / an toàn: bỏ qua nếu chưa có save hoặc nội dung đã khớp bản sao lưu mới nhất.
fn smart_backup(dir: &Path, root: &Path, label: &str, kind: &str, keep: usize, protect: Option<&str>) -> Result<Option<BackupInfo>, String> {
    let fp = fingerprint(dir);
    if fp.is_empty() {
        return Ok(None);
    }
    if let Some(last) = list_backups(root).first() {
        if read_meta(root, &last.id).map(|m| m.fingerprint == fp).unwrap_or(false) {
            return Ok(None);
        }
    }
    let b = create_backup(dir, root, label, kind)?;
    prune(root, keep, protect);
    Ok(b)
}

pub fn auto_backup(dir: &Path, root: &Path, keep: usize) -> Result<Option<BackupInfo>, String> {
    smart_backup(dir, root, "Tự động trước khi chơi", "auto", keep, None)
}

pub fn manual_backup(dir: &Path, root: &Path, label: &str, keep: usize) -> Result<BackupInfo, String> {
    let label = if clean_label(label).is_empty() { "Sao lưu thủ công" } else { label };
    let b = create_backup(dir, root, label, "manual")?.ok_or("Chưa có file save nào để sao lưu")?;
    prune(root, keep, Some(&b.id));
    Ok(b)
}

// ---------------------------------------------------------------------
// Khôi phục / nhập / xuất / xoá
// ---------------------------------------------------------------------

pub fn restore_backup(dir: &Path, root: &Path, id: &str, only: Option<&[u8]>, keep: usize) -> Result<RestoreResult, String> {
    if !valid_id(id) {
        return Err("Mã bản sao lưu không hợp lệ".into());
    }
    let meta = read_meta(root, id).ok_or("Không tìm thấy bản sao lưu này")?;
    let targets: Vec<u8> = match only {
        Some(list) => {
            for n in list {
                slot_file(*n)?;
                if !meta.slots.contains(n) {
                    return Err(format!("Bản sao lưu này không có slot {n}"));
                }
            }
            list.to_vec()
        }
        None => meta.slots.clone(),
    };
    if targets.is_empty() {
        return Err("Không có slot nào để khôi phục".into());
    }
    // Đọc + kiểm tra hết trước khi đụng vào save hiện tại
    let mut loaded = Vec::new();
    for &n in &targets {
        let data = fs::read(root.join(id).join(slot_file(n)?)).map_err(|e| format!("Không đọc được slot {n} trong bản sao lưu: {e}"))?;
        validate_save(&data).map_err(|e| format!("Bản sao lưu bị hỏng ở slot {n}: {e}"))?;
        loaded.push((n, data));
    }
    fs::create_dir_all(dir).map_err(|e| format!("Không tạo được thư mục save: {e}"))?;
    smart_backup(dir, root, "Trước khi khôi phục", "safety", keep, Some(id))?;
    for (n, data) in &loaded {
        write_atomic(&slot_path(dir, *n)?, data)?;
    }
    Ok(RestoreResult { restored: targets })
}

pub fn delete_backup(root: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("Mã bản sao lưu không hợp lệ".into());
    }
    let p = root.join(id);
    if !p.join(META).is_file() {
        return Err("Không tìm thấy bản sao lưu này".into());
    }
    fs::remove_dir_all(&p).map_err(|e| format!("Không xoá được: {e}"))
}

pub fn delete_slot(dir: &Path, root: &Path, slot: u8, keep: usize) -> Result<(), String> {
    let p = slot_path(dir, slot)?;
    if !p.is_file() {
        return Err(format!("Slot {slot} đang trống"));
    }
    smart_backup(dir, root, &format!("Trước khi xoá slot {slot}"), "safety", keep, None)?;
    fs::remove_file(&p).map_err(|e| format!("Không xoá được slot {slot}: {e}"))
}

pub fn import_save(dir: &Path, root: &Path, slot: u8, src: &Path, keep: usize) -> Result<(), String> {
    let p = slot_path(dir, slot)?;
    let m = fs::metadata(src).map_err(|e| format!("Không đọc được file: {e}"))?;
    if !m.is_file() || m.len() > MAX_IMPORT {
        return Err("File không hợp lệ hoặc quá lớn so với một save GTA:SA".into());
    }
    let data = fs::read(src).map_err(|e| format!("Không đọc được file: {e}"))?;
    validate_save(&data)?;
    fs::create_dir_all(dir).map_err(|e| format!("Không tạo được thư mục save: {e}"))?;
    if p.is_file() {
        smart_backup(dir, root, &format!("Trước khi nhập đè slot {slot}"), "safety", keep, None)?;
    }
    write_atomic(&p, &data)
}

pub fn export_save(dir: &Path, slot: u8, dest: &Path) -> Result<(), String> {
    let p = slot_path(dir, slot)?;
    if !p.is_file() {
        return Err(format!("Slot {slot} đang trống"));
    }
    if dest.is_dir() {
        return Err("Đường dẫn đích là một thư mục".into());
    }
    if dest.exists() && fs::canonicalize(dest).ok() == fs::canonicalize(&p).ok() {
        return Err("Không thể xuất đè lên chính file save".into());
    }
    fs::copy(&p, dest).map(|_| ()).map_err(|e| format!("Không xuất được file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("aether-saves-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// Save giả hợp lệ: tên UTF-16LE hoặc ASCII, phần thân lặp theo `fill`
    fn make_save(name: &str, wide: bool, fill: u8) -> Vec<u8> {
        let mut d = vec![fill; SAVE_SIZE];
        d[..5].copy_from_slice(b"BLOCK");
        d[5..9].copy_from_slice(&[1, 2, 3, 4]);
        let mut nm = vec![0u8; NAME_BYTES];
        if wide {
            for (i, u) in name.encode_utf16().take(50).enumerate() {
                nm[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
            }
        } else {
            nm[..name.len()].copy_from_slice(name.as_bytes());
        }
        d[NAME_OFFSET..NAME_OFFSET + NAME_BYTES].copy_from_slice(&nm);
        let sum = d[..SAVE_SIZE - 4].iter().fold(0u32, |a, &b| a.wrapping_add(b as u32));
        d[SAVE_SIZE - 4..].copy_from_slice(&sum.to_le_bytes());
        d
    }

    #[test]
    fn validation() {
        let ok = make_save("Los Santos", true, 7);
        assert!(validate_save(&ok).is_ok());
        let mut bad = ok.clone();
        bad[1000] ^= 1;
        assert!(validate_save(&bad).unwrap_err().contains("checksum"));
        let mut nomagic = ok.clone();
        nomagic[0] = b'X';
        assert!(validate_save(&nomagic).is_err());
        assert!(validate_save(&ok[..SAVE_SIZE - 1]).unwrap_err().contains("Dung lượng"));
        assert!(validate_save(&[]).is_err());
    }

    #[test]
    fn names() {
        assert_eq!(save_name(&make_save("Los Santos", true, 0)), "Los Santos");
        assert_eq!(save_name(&make_save("San Fierro", false, 0)), "San Fierro");
        assert_eq!(save_name(&make_save("Việt", true, 0)), "Việt");
        assert_eq!(save_name(&make_save("", true, 0)), "");
        let mut d = make_save("ab", false, 0);
        d[NAME_OFFSET + 1] = 0x07; // ký tự điều khiển bị lọc
        assert_eq!(save_name(&d), "a");
        assert_eq!(save_name(&[0u8; 4]), "");
    }

    #[test]
    fn civil_dates_and_ids() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_700_000_000), (2023, 11, 14, 22, 13, 20));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0)); // năm nhuận chia hết cho 400
        for ok in ["20231114-221320", "20231114-221320-001", "20231114-221320-999"] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "..", "../x", "20231114-22132", "2023111-4221320", "20231114_221320", "20231114-221320/../..", "x0231114-221320", "20231114-221320-"] {
            if bad == "20231114-221320-" {
                continue; // đuôi '-' vẫn là chuỗi số + '-', vô hại vì không chứa '.' hay '/'
            }
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn slots_listing() {
        let dir = tmp("list");
        fs::write(dir.join("GTASAsf1.b"), make_save("Los Santos", true, 1)).unwrap();
        fs::write(dir.join("GTASAsf3.b"), b"junk").unwrap();
        fs::write(dir.join("GTASAsf9.b"), make_save("x", true, 1)).unwrap(); // ngoài phạm vi
        let s = list_slots(&dir);
        assert_eq!(s.len(), 8);
        assert!(s[0].exists && s[0].valid && s[0].name == "Los Santos" && s[0].size == SAVE_SIZE as u64);
        assert!(s[2].exists && !s[2].valid && !s[2].problem.is_empty());
        assert!(!s[1].exists && !s[7].exists);
        assert_eq!(slot_file(0).unwrap_err(), "Slot phải từ 1 đến 8");
        assert!(slot_file(9).is_err() && slot_file(8).is_ok());
    }

    #[test]
    fn backup_restore_roundtrip_with_safety() {
        let (dir, root) = (tmp("rt-saves"), tmp("rt-bak"));
        let a1 = make_save("Mission A", true, 1);
        let a2 = make_save("Mission B", true, 2);
        fs::write(dir.join("GTASAsf1.b"), &a1).unwrap();
        fs::write(dir.join("GTASAsf2.b"), &a2).unwrap();
        let b = manual_backup(&dir, &root, "  Trước khi vào CJ\u{7}  ", 10).unwrap();
        assert_eq!((b.kind.as_str(), b.slots.clone(), b.label.as_str()), ("manual", vec![1, 2], "Trước khi vào CJ"));
        assert_eq!(b.size, 2 * SAVE_SIZE as u64);
        // đổi save hiện tại
        let a1b = make_save("Mission C", true, 9);
        fs::write(dir.join("GTASAsf1.b"), &a1b).unwrap();
        fs::remove_file(dir.join("GTASAsf2.b")).unwrap();
        // khôi phục riêng slot 2
        let r = restore_backup(&dir, &root, &b.id, Some(&[2]), 10).unwrap();
        assert_eq!(r.restored, vec![2]);
        assert_eq!(fs::read(dir.join("GTASAsf2.b")).unwrap(), a2);
        assert_eq!(fs::read(dir.join("GTASAsf1.b")).unwrap(), a1b, "slot 1 không bị đụng tới");
        // đã có bản safety của trạng thái trước khi khôi phục
        let all = list_backups(&root);
        assert!(all.iter().any(|x| x.kind == "safety" && x.label == "Trước khi khôi phục"), "{all:?}");
        // khôi phục tất cả
        restore_backup(&dir, &root, &b.id, None, 10).unwrap();
        assert_eq!(fs::read(dir.join("GTASAsf1.b")).unwrap(), a1);
        // slot không có trong bản sao lưu
        assert!(restore_backup(&dir, &root, &b.id, Some(&[5]), 10).unwrap_err().contains("không có slot 5"));
        assert!(restore_backup(&dir, &root, &b.id, Some(&[0]), 10).is_err());
        assert!(restore_backup(&dir, &root, "../etc", None, 10).is_err());
        assert!(!dir.join("GTASAsf1.b.tmp-aether").exists() && !dir.join("GTASAsf1.b.tmp-aether").exists());
    }

    #[test]
    fn corrupted_backup_is_refused_without_touching_saves() {
        let (dir, root) = (tmp("cor-saves"), tmp("cor-bak"));
        let a1 = make_save("Keep me", true, 3);
        fs::write(dir.join("GTASAsf1.b"), &a1).unwrap();
        fs::write(dir.join("GTASAsf2.b"), make_save("Two", true, 4)).unwrap();
        let b = manual_backup(&dir, &root, "x", 10).unwrap();
        let mut bad = fs::read(root.join(&b.id).join("GTASAsf2.b")).unwrap();
        bad[500] ^= 0xFF;
        fs::write(root.join(&b.id).join("GTASAsf2.b"), bad).unwrap();
        fs::write(dir.join("GTASAsf1.b"), make_save("Current", true, 8)).unwrap();
        let err = restore_backup(&dir, &root, &b.id, None, 10).unwrap_err();
        assert!(err.contains("slot 2") && err.contains("hỏng"), "{err}");
        assert_eq!(save_name(&fs::read(dir.join("GTASAsf1.b")).unwrap()), "Current", "save hiện tại giữ nguyên");
    }

    #[test]
    fn auto_backup_dedupes_and_prunes_but_keeps_manual() {
        let (dir, root) = (tmp("auto-saves"), tmp("auto-bak"));
        assert!(auto_backup(&dir, &root, 3).unwrap().is_none(), "chưa có save -> không sao lưu");
        fs::write(dir.join("GTASAsf1.b"), make_save("v0", true, 0)).unwrap();
        let manual = manual_backup(&dir, &root, "giữ mãi", 3).unwrap();
        assert!(auto_backup(&dir, &root, 3).unwrap().is_none(), "nội dung trùng bản mới nhất -> bỏ qua");
        for i in 1..=6u8 {
            fs::write(dir.join("GTASAsf1.b"), make_save("v", true, i)).unwrap();
            assert!(auto_backup(&dir, &root, 3).unwrap().is_some(), "lần {i}");
            assert!(auto_backup(&dir, &root, 3).unwrap().is_none(), "lặp lần {i} phải bỏ qua");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let all = list_backups(&root);
        assert_eq!(all.iter().filter(|b| b.kind == "auto").count(), 3, "{all:?}");
        assert!(all.iter().any(|b| b.id == manual.id), "bản thủ công không bị prune");
        assert_eq!(all[0].kind, "auto");
        // khôi phục bản thủ công cũ nhất không bị prune mất bản đang khôi phục
        restore_backup(&dir, &root, &manual.id, None, 1).unwrap();
        assert!(list_backups(&root).iter().any(|b| b.id == manual.id));
    }

    #[test]
    fn delete_slot_makes_safety_copy() {
        let (dir, root) = (tmp("del-saves"), tmp("del-bak"));
        let data = make_save("Precious", true, 5);
        fs::write(dir.join("GTASAsf4.b"), &data).unwrap();
        delete_slot(&dir, &root, 4, 10).unwrap();
        assert!(!dir.join("GTASAsf4.b").exists());
        let b = list_backups(&root);
        assert_eq!((b.len(), b[0].kind.as_str(), b[0].label.as_str()), (1, "safety", "Trước khi xoá slot 4"));
        restore_backup(&dir, &root, &b[0].id, None, 10).unwrap();
        assert_eq!(fs::read(dir.join("GTASAsf4.b")).unwrap(), data, "xoá nhầm vẫn lấy lại được");
        assert!(delete_slot(&dir, &root, 7, 10).unwrap_err().contains("trống"));
        assert!(delete_slot(&dir, &root, 12, 10).is_err());
        delete_backup(&root, &b[0].id).unwrap();
        assert!(list_backups(&root).is_empty());
        assert!(delete_backup(&root, "..").is_err());
        assert!(delete_backup(&root, "20000101-000000").unwrap_err().contains("Không tìm thấy"));
    }

    #[test]
    fn import_export() {
        let (dir, root, ext) = (tmp("ie-saves"), tmp("ie-bak"), tmp("ie-ext"));
        let good = make_save("Imported", true, 6);
        let src = ext.join("my save.b");
        fs::write(&src, &good).unwrap();
        import_save(&dir, &root, 2, &src, 10).unwrap();
        assert_eq!(fs::read(dir.join("GTASAsf2.b")).unwrap(), good);
        assert!(list_backups(&root).is_empty(), "slot trống: không cần sao lưu an toàn");
        // nhập đè -> có safety
        let good2 = make_save("Second", true, 7);
        fs::write(&src, &good2).unwrap();
        import_save(&dir, &root, 2, &src, 10).unwrap();
        assert_eq!(list_backups(&root)[0].label, "Trước khi nhập đè slot 2");
        // file sai bị từ chối, save hiện tại giữ nguyên
        fs::write(&src, b"not a save").unwrap();
        assert!(import_save(&dir, &root, 2, &src, 10).is_err());
        let mut corrupt = good.clone();
        corrupt[77] ^= 1;
        fs::write(&src, corrupt).unwrap();
        assert!(import_save(&dir, &root, 2, &src, 10).unwrap_err().contains("checksum"));
        assert!(import_save(&dir, &root, 0, &src, 10).is_err());
        assert!(import_save(&dir, &root, 1, &ext.join("missing.b"), 10).is_err());
        assert_eq!(fs::read(dir.join("GTASAsf2.b")).unwrap(), good2);
        // xuất
        let out = ext.join("exported.b");
        export_save(&dir, 2, &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), good2);
        assert!(export_save(&dir, 3, &out).unwrap_err().contains("trống"));
        assert!(export_save(&dir, 2, &ext).is_err(), "đích là thư mục");
        assert!(export_save(&dir, 2, &dir.join("GTASAsf2.b")).is_err(), "không xuất đè lên chính nó");
    }
}
