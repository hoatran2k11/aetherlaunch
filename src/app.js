(() => {
'use strict';

// ---------- Cầu nối Tauri (withGlobalTauri: true) ----------
const T = window.__TAURI__ || {};
const invoke = T.core ? T.core.invoke : async () => { throw new Error('Hãy chạy ứng dụng qua Tauri (npm run tauri dev)'); };
const win = T.window && T.window.getCurrentWindow ? T.window.getCurrentWindow() : null;

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

// ---------- Icon Material (inline SVG) ----------
const ICONS = {
  play: 'M8 5v14l11-7z',
  dns: 'M20 13H4c-.55 0-1 .45-1 1v6c0 .55.45 1 1 1h16c.55 0 1-.45 1-1v-6c0-.55-.45-1-1-1zM7 19c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zM20 3H4c-.55 0-1 .45-1 1v6c0 .55.45 1 1 1h16c.55 0 1-.45 1-1V4c0-.55-.45-1-1-1zM7 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2z',
  settings: 'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 0 0 .12-.61l-1.92-3.32a.49.49 0 0 0-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 0 0-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96a.49.49 0 0 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58a.49.49 0 0 0-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z',
  info: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z',
  refresh: 'M17.65 6.35A7.958 7.958 0 0 0 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0 1 12 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z',
  folder: 'M20 6h-8l-2-2H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2zm0 12H4V8h16v10z',
  delete: 'M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z',
  add: 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z',
  close: 'M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
  min: 'M6 19h12v2H6z',
  max: 'M4 4h16v16H4V4zm2 2v12h12V6H6z',
  edit: 'M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a.996.996 0 0 0 0-1.41l-2.34-2.34a.996.996 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z',
  expand: 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z',
  check: 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
  person: 'M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z',
};
function paintIcons(root = document) {
  $$('[data-i]', root).forEach(el => {
    if (!el.firstChild && ICONS[el.dataset.i]) el.innerHTML = `<svg viewBox="0 0 24 24"><path d="${ICONS[el.dataset.i]}"/></svg>`;
  });
}
paintIcons();
// Chỉ ghi DOM khi nội dung thật sự đổi -> không dựng lại / nhấp nháy khi chuyển tab
function setHtml(el, html) { if (el.__h === html) return false; el.__h = html; el.innerHTML = html; return true; }

// ---------- Logo / avatar: link cứng trong index.html, lỗi thì quay về chữ cái ----------
function imgFallback(imgId, txtId) {
  const img = $('#' + imgId), txt = $('#' + txtId);
  const fail = () => { img.hidden = true; txt.hidden = false; };
  img.addEventListener('error', fail);
  if (img.complete && img.naturalWidth === 0) fail();
}
imgFallback('logoImg', 'logoTxt'); imgFallback('avatarImg', 'avatarTxt');

// ---------- Cấu hình (localStorage) ----------
// Server giờ là danh sách riêng: { name, host, port, password, encoding }, chọn bằng cfg.selected.
const KEY = 'samp-launcher.v1';
const ENC = 'windows-1252';
const DEFAULTS = {
  gtaPath: '', askedGame: false, playerName: 'Max_Bennett', selected: 0,
  servers: [{ name: 'GTA Viet Nam', host: 'sv.gtaviet.net', port: 7777, password: '', encoding: ENC }],
};
function load() {
  let raw = {};
  try { raw = JSON.parse(localStorage.getItem(KEY) || '{}') || {}; } catch { raw = {}; }
  const c = { ...DEFAULTS, ...raw };
  c.servers = (Array.isArray(raw.servers) ? raw.servers : DEFAULTS.servers).map(sv => ({
    name: sv.name || '', host: sv.host, port: Number(sv.port) || 7777,
    password: sv.password || '', encoding: sv.encoding || raw.encoding || ENC,
  }));
  // Chuyển cấu hình cũ (host/port/password nằm trong Cài đặt) vào danh sách server
  if (raw.host) {
    let i = c.servers.findIndex(s => s.host === raw.host && s.port === (Number(raw.port) || 7777));
    if (i < 0) { c.servers.push({ name: '', host: raw.host, port: Number(raw.port) || 7777, password: raw.password || '', encoding: raw.encoding || ENC }); i = c.servers.length - 1; }
    else if (raw.password && !c.servers[i].password) c.servers[i].password = raw.password;
    c.selected = i;
  }
  ['host', 'port', 'password', 'encoding', 'logoUrl', 'avatarUrl'].forEach(k => delete c[k]);
  c.selected = Math.min(Math.max(Number(c.selected) || 0, 0), Math.max(c.servers.length - 1, 0));
  return c;
}
let cfg = load();
const save = () => localStorage.setItem(KEY, JSON.stringify(cfg));
save();

const cur = () => cfg.servers[cfg.selected] || null;
const label = sv => sv.name || `${sv.host}:${sv.port}`;
const srvKey = sv => `${sv.host}:${sv.port}`;
const validHost = h => /^[A-Za-z0-9.\-_]+$/.test(h) && h.length <= 253;
const validPort = p => Number.isInteger(p) && p > 0 && p < 65536;
const normName = n => n.trim().split(/\s+/).join('_');
const validName = n => n.includes('_') && /^[A-Za-z0-9_\[\]()$@.=]{3,20}$/.test(n);
const pingClass = ms => ms < 80 ? 'good' : ms < 160 ? 'mid' : 'bad';

function toast(msg, kind = 'ok') {
  const t = document.createElement('div');
  t.className = 'toast ' + (kind === 'err' ? 'err' : '');
  t.textContent = msg; $('#toasts').append(t);
  setTimeout(() => t.classList.add('out'), 10000); setTimeout(() => t.remove(), 10400);
}

// ---------- Cửa sổ ----------
$('#wMin').onclick = () => win && win.minimize();
$('#wMax').onclick = () => win && win.toggleMaximize();
$('#wClose').onclick = () => win && win.close();

// ---------- Tabs ----------
let tab = 'play';
function go(name) {
  tab = name;
  $$('.nav button').forEach(b => b.classList.toggle('active', b.dataset.tab === name));
  $$('.view').forEach(v => v.classList.toggle('show', v.id === 'tab-' + name));
  toggleDd(false);
  if (name === 'play' && Date.now() - lastFetch > 4000) refreshStatus(true);
  if (name === 'servers') queryServers();
  if (name === 'settings') fillSettings();
}
$$('.nav button').forEach(b => b.onclick = () => go(b.dataset.tab));
$('#btnManage').onclick = () => go('servers');

// =====================================================================
// PLAY
// =====================================================================
let busy = false, reqId = 0, lastFetch = 0, lastStatus = null;
let lastPlayers = [], lastError = null, plLoading = false, plReq = 0;

// Dropdown chọn server nhanh (tự vẽ để không lệch khung với danh sách dài)
function renderServerSelect() {
  const sv = cur(), btn = $('#ddBtn');
  $('#sideAddr').textContent = sv ? srvKey(sv) : '—';
  $('#ddLabel').textContent = sv ? label(sv) : 'Chưa có server';
  btn.title = sv ? `${label(sv)} — ${srvKey(sv)}` : 'Chưa có server';
  btn.disabled = !cfg.servers.length;
  $('#ddMenu').innerHTML = cfg.servers.map((s, i) =>
    `<button type="button" class="dd-item${i === cfg.selected ? ' on' : ''}" role="option" aria-selected="${i === cfg.selected}" data-idx="${i}" title="${esc(label(s))} — ${esc(srvKey(s))}">
       <span class="dd-txt"><b>${esc(label(s))}</b><small>${esc(srvKey(s))}</small></span>
       <span class="ic dd-check" data-i="check"></span></button>`).join('');
  paintIcons($('#ddMenu'));
}
function toggleDd(open) {
  const dd = $('#srvDd'), want = open ?? !dd.classList.contains('open');
  dd.classList.toggle('open', want && !$('#ddBtn').disabled);
  $('#ddBtn').setAttribute('aria-expanded', String(dd.classList.contains('open')));
  if (dd.classList.contains('open')) { const on = $('.dd-item.on', dd); if (on) on.scrollIntoView({ block: 'nearest' }); }
}
$('#ddBtn').onclick = e => { e.stopPropagation(); toggleDd(); };
$('#ddMenu').onclick = e => {
  const it = e.target.closest('.dd-item'); if (!it) return;
  toggleDd(false);
  const i = Number(it.dataset.idx); if (i !== cfg.selected) selectServer(i);
};
document.addEventListener('click', e => { if (!e.target.closest('#srvDd')) toggleDd(false); });
document.addEventListener('keydown', e => { if (e.key === 'Escape') toggleDd(false); });

function selectServer(i) {
  if (!cfg.servers[i]) return;
  cfg.selected = i; save(); resetServerView();
  renderServerSelect(); paintAllCards(); refreshStatus(false, true);
}
function resetServerView() { lastStatus = null; lastPlayers = []; lastError = null; plLoading = false; plReq++; renderPlayers(); }

async function refreshStatus(quiet = false, force = false) {
  const sv = cur();
  if (!sv) return renderNoServer();
  if (busy && !force) return;
  const my = ++reqId; busy = true;
  $('#btnRefresh').classList.add('spin');
  if (!quiet) { $('#srvState').textContent = 'Đang truy vấn…'; $('#srvDot').className = 'state'; }
  try {
    const s = await invoke('query_server', { host: sv.host, port: sv.port, encoding: sv.encoding });
    if (my !== reqId) return;
    lastFetch = Date.now(); renderStatus(s);
  } catch (e) { if (my === reqId) renderOffline(e, sv); }
  finally { if (my === reqId) { busy = false; $('#btnRefresh').classList.remove('spin'); } }
}

function setState(on) {
  $('#srvDot').className = 'state ' + (on ? 'on' : 'off');
  $('#sideState').className = 'state ' + (on ? 'on' : 'off');
  $('#srvState').textContent = on ? 'Online' : 'Offline';
}
function clearStats() {
  ['stAddr', 'stPlayers', 'stPing', 'stLock'].forEach(id => $('#' + id).textContent = '—');
  $('#stBar').style.width = '0'; $('#srvChips').innerHTML = '';
}

function renderStatus(s) {
  setState(true);
  $('#srvName').textContent = s.hostname || '—';
  $('#srvMode').textContent = [s.mode, s.language].filter(Boolean).join(' · ') || '—';
  $('#stAddr').textContent = `${s.ip}:${s.port}`;
  $('#stPlayers').textContent = `${s.players} / ${s.maxPlayers}`;
  $('#stBar').style.width = (s.maxPlayers ? Math.min(100, s.players / s.maxPlayers * 100) : 0) + '%';
  const ms = Math.round(s.pingMs);
  $('#stPing').innerHTML = `<span class="${pingClass(ms)}">${ms} ms</span>`;
  $('#stLock').textContent = s.password ? '🔒 Có' : 'Không';
  const r = s.rules || {};
  setHtml($('#srvChips'), ['version', 'mapname', 'worldtime', 'weather', 'weburl'].filter(k => r[k])
    .map(k => `<span class="chip">${esc(k === 'weburl' || k === 'version' ? r[k] : `${k}: ${r[k]}`)}</span>`).join(''));
  lastStatus = s;
  loadPlayers(s); // chạy nền, không chặn thẻ trạng thái
}

function renderOffline(e, sv) {
  setState(false);
  $('#srvName').textContent = srvKey(sv);
  $('#srvMode').textContent = String(e);
  clearStats(); lastStatus = null; lastPlayers = []; lastError = 'Không truy vấn được server'; plLoading = false; renderPlayers();
}
function renderNoServer() {
  $('#srvDot').className = 'state'; $('#sideState').className = 'state'; $('#srvState').textContent = 'Chưa có server';
  $('#srvName').textContent = '—'; $('#srvMode').textContent = 'Hãy thêm server ở tab Danh sách server';
  clearStats(); lastStatus = null; lastPlayers = []; lastError = 'Chưa có server nào'; plLoading = false; renderPlayers();
}

// ---------- Danh sách người chơi (chỉ Tên + Điểm, lấy bằng opcode 'c', dự phòng 'd') ----------
async function loadPlayers(s) {
  const sv = cur(); if (!sv) return;
  const my = ++plReq;
  if (s.players === 0) { lastPlayers = []; lastError = 'Server đang trống'; plLoading = false; return renderPlayers(); }
  if (s.players > 100) { lastPlayers = []; lastError = 'Server có hơn 100 người nên SA-MP không cung cấp danh sách người chơi'; plLoading = false; return renderPlayers(); }
  if (!lastPlayers.length) { lastError = null; plLoading = true; renderPlayers(); }
  try {
    const r = await invoke('query_players', { host: sv.host, port: sv.port, encoding: sv.encoding });
    if (my !== plReq) return;
    lastPlayers = r.players; lastError = r.error || null;
  } catch (e) { if (my !== plReq) return; lastPlayers = []; lastError = String(e); }
  plLoading = false; renderPlayers();
}

function renderPlayers() {
  const q = $('#plSearch').value.trim().toLowerCase();
  const list = lastPlayers.filter(p => p.name.toLowerCase().includes(q));
  $('#plCount').textContent = lastPlayers.length ? `(${lastPlayers.length})` : '';
  const body = $('#plBody');
  if (list.length) {
    setHtml(body, list.map(p => `<div class="pl-row"><span class="nm">${esc(p.name)}</span><span class="sc">${p.score}</span></div>`).join(''));
    return;
  }
  const msg = plLoading ? 'Đang tải danh sách người chơi…' : (lastError || (q ? 'Không tìm thấy người chơi' : 'Chưa có dữ liệu'));
  const canRetry = !plLoading && lastError && lastStatus && lastStatus.players > 0 && lastStatus.players <= 100;
  setHtml(body, `<div class="empty">${esc(msg)}${canRetry ? '<div><button id="plRetry" class="btn">Thử lại</button></div>' : ''}</div>`);
}
$('#plSearch').oninput = renderPlayers;
$('#plBody').onclick = e => { if (e.target.closest('#plRetry') && lastStatus) loadPlayers(lastStatus); };
$('#btnRefresh').onclick = () => refreshStatus();
setInterval(() => { if (tab === 'play' && !document.hidden && cur()) refreshStatus(true); }, 10000);

// ---------- Khởi chạy game ----------
$('#btnPlay').onclick = async () => {
  const sv = cur();
  if (!sv) { toast('Chưa có server – hãy thêm ở tab Danh sách server', 'err'); return go('servers'); }
  if (!cfg.gtaPath) { toast('Chưa chọn gta_sa.exe – mở Cài đặt', 'err'); return go('settings'); }
  if (lastStatus && lastStatus.password && !sv.password) {
    toast('Server này cần mật khẩu – bấm Sửa trong Danh sách server để nhập', 'err'); return go('servers');
  }
  const b = $('#btnPlay'); b.disabled = true; b.classList.add('loading');
  try {
    const m = await invoke('launch_game', { exePath: cfg.gtaPath, name: cfg.playerName, host: sv.host, port: sv.port, password: sv.password || null });
    toast(m);
  } catch (e) { toast(String(e), 'err'); }
  finally { b.disabled = false; b.classList.remove('loading'); }
};

// =====================================================================
// SERVERS
// =====================================================================
const srvInfo = {}; // "host:port" -> { s } | { err: true }

function srvInner(i, sv) {
  const info = srvInfo[srvKey(sv)], s = info && info.s;
  const name = esc(sv.name || (s && s.hostname) || sv.host);
  const tag = i === cfg.selected ? '<span class="tag">Đang chọn</span>' : '';
  const stats = s
    ? `<div class="num"><b>${s.players}/${s.maxPlayers}</b><small>Người chơi</small></div><div class="num"><b class="${pingClass(s.pingMs)}">${Math.round(s.pingMs)}</b><small>Ping</small></div>`
    : `<div class="num"><b>${info && info.err ? '✕' : '…'}</b><small>${info && info.err ? 'Offline' : 'Đang tải'}</small></div><div class="num"></div>`;
  return `<div><div class="nm">${name}${tag}${sv.password ? ' <span class="pw" title="Đã lưu mật khẩu">🔒</span>' : ''}</div>
      <div class="ad">${esc(srvKey(sv))}${s && s.mode ? ' · ' + esc(s.mode) : ''}</div></div>
    ${stats}
    <div class="btns">
      <button class="btn" data-act="pick" data-idx="${i}"><span class="ic" data-i="play"></span>Chọn</button>
      <button class="btn sq" data-act="edit" data-idx="${i}" title="Sửa"><span class="ic" data-i="edit"></span></button>
      <button class="btn sq danger" data-act="del" data-idx="${i}" title="Xoá"><span class="ic" data-i="delete"></span></button>
    </div>`;
}
// Khung thẻ chỉ dựng khi danh sách đổi; việc cập nhật số liệu chỉ thay nội dung bên trong
// để lớp kính không bị tạo lại (tránh blur nhấp nháy).
function buildServerList() {
  const box = $('#serverList');
  if (!cfg.servers.length) { box.innerHTML = '<div class="glass pad empty">Chưa có server nào. Bấm “Thêm server” để bắt đầu.</div>'; return; }
  box.innerHTML = cfg.servers.map((sv, i) => `<div class="glass srv${i === cfg.selected ? ' sel' : ''}" id="sv-${i}" style="--n:${i}">${srvInner(i, sv)}</div>`).join('');
  paintIcons(box);
}
function paintCard(i) {
  const el = $('#sv-' + i), sv = cfg.servers[i]; if (!el || !sv) return;
  if (setHtml(el, srvInner(i, sv))) paintIcons(el);
  el.classList.toggle('sel', i === cfg.selected);
}
function paintAllCards() { cfg.servers.forEach((_, i) => paintCard(i)); }
let lastSrvQuery = 0;
async function queryServers(force = false) {
  if (!force && Date.now() - lastSrvQuery < 4000) return; // tránh dồn dập khi bấm qua lại giữa các tab
  lastSrvQuery = Date.now();
  await Promise.all(cfg.servers.map(async (sv, i) => {
    try { srvInfo[srvKey(sv)] = { s: await invoke('query_server', { host: sv.host, port: sv.port, encoding: sv.encoding }) }; }
    catch { srvInfo[srvKey(sv)] = { err: true }; }
    paintCard(i);
  }));
}

$('#serverList').onclick = e => {
  const b = e.target.closest('button[data-act]'); if (!b) return;
  const i = Number(b.dataset.idx), sv = cfg.servers[i]; if (!sv) return;
  if (b.dataset.act === 'pick') { selectServer(i); toast(`Đã chọn ${label(sv)}`); go('play'); }
  else if (b.dataset.act === 'edit') openModal(i);
  else {
    const wasSel = i === cfg.selected;
    cfg.servers.splice(i, 1);
    if (cfg.selected > i) cfg.selected--;
    cfg.selected = Math.min(Math.max(cfg.selected, 0), Math.max(cfg.servers.length - 1, 0));
    save(); buildServerList(); renderServerSelect();
    if (wasSel) { resetServerView(); refreshStatus(false, true); }
    toast(`Đã xoá ${label(sv)}`);
  }
};

// ---------- Modal thêm / sửa server ----------
let editIdx = -1;
// Hiện/ẩn modal dùng chung: display trước rồi mới thêm .open ở khung kế tiếp; ẩn hẳn (display:none) sau khi fade xong
function showBack(el) {
  clearTimeout(el.__t); el.classList.add('show'); el.setAttribute('aria-hidden', 'false');
  requestAnimationFrame(() => requestAnimationFrame(() => el.classList.add('open')));
}
function hideBack(el) {
  el.classList.remove('open'); el.setAttribute('aria-hidden', 'true');
  clearTimeout(el.__t); el.__t = setTimeout(() => el.classList.remove('show'), 260);
}
function openModal(i = -1) {
  editIdx = i;
  const sv = cfg.servers[i] || { name: '', host: '', port: '', password: '', encoding: ENC };
  $('#mTitle').textContent = i >= 0 ? 'Sửa server' : 'Thêm server';
  $('#mName').value = sv.name; $('#mHost').value = sv.host; $('#mPort').value = sv.port;
  $('#mPass').value = sv.password; $('#mEnc').value = sv.encoding;
  showBack($('#modal'));
  setTimeout(() => $('#mHost').focus(), 60);
}
function closeModal() { hideBack($('#modal')); }
function saveModal() {
  const host = $('#mHost').value.trim(), port = Number($('#mPort').value.trim() || 7777), pass = $('#mPass').value;
  if (!validHost(host) || !validPort(port)) return toast('IP/domain hoặc port không hợp lệ', 'err');
  if (!/^[^\s"]{0,64}$/.test(pass)) return toast('Mật khẩu không được chứa dấu cách hoặc dấu nháy', 'err');
  const sv = { name: $('#mName').value.trim(), host, port, password: pass, encoding: $('#mEnc').value };
  const wasEmpty = cfg.servers.length === 0;
  const touchesSel = wasEmpty || (editIdx >= 0 && editIdx === cfg.selected);
  if (editIdx >= 0) cfg.servers[editIdx] = sv; else cfg.servers.push(sv);
  if (wasEmpty) cfg.selected = 0;
  save(); buildServerList(); renderServerSelect(); closeModal(); queryServers(true);
  if (touchesSel) { resetServerView(); refreshStatus(false, true); }
  toast(editIdx >= 0 ? 'Đã cập nhật server' : 'Đã thêm server');
}
$('#btnAddSrv').onclick = () => openModal(-1);
$('#mCancel').onclick = closeModal;
$('#mSave').onclick = saveModal;
$('#modal').onmousedown = e => { if (e.target.id === 'modal') closeModal(); };
document.addEventListener('keydown', e => {
  const m = $$('.modal-back.show').pop(); if (!m) return;
  const isSrv = m.id === 'modal';
  if (e.key === 'Escape') (isSrv ? closeModal : skipGame)();
  if (e.key === 'Enter' && e.target.matches('.modal-back input')) (isSrv ? saveModal : confirmGame)();
});

// =====================================================================
// SETTINGS (chỉ còn đường dẫn game + tên nhân vật)
// =====================================================================
function fillSettings() { $('#setPath').value = cfg.gtaPath; $('#setName').value = cfg.playerName; updateNameHint(); checkGame(); }
function updateNameHint() { $('#nameHint').classList.toggle('bad', !validName(normName($('#setName').value))); }
async function checkGame() {
  const p = $('#setPath').value.trim(), box = $('#gameCheck'); box.innerHTML = '';
  if (!p) return;
  try {
    const r = await invoke('check_game', { exePath: p });
    box.innerHTML = `<span class="badge ${r.exe ? 'ok' : 'no'}">${r.exe ? '✓' : '✕'} gta_sa.exe</span><span class="badge ${r.sampDll ? 'ok' : 'no'}">${r.sampDll ? '✓' : '✕'} samp.dll</span>`;
  } catch { /* chạy ngoài Tauri */ }
}
$('#setName').oninput = updateNameHint;
$('#setPath').onchange = checkGame;
async function pickGameExe() {
  try { return await T.dialog.open({ multiple: false, title: 'Chọn gta_sa.exe', filters: [{ name: 'gta_sa.exe', extensions: ['exe'] }] }); }
  catch (e) { toast(String(e), 'err'); return null; }
}
$('#btnBrowse').onclick = async () => { const f = await pickGameExe(); if (f) { $('#setPath').value = f; checkGame(); } };

// =====================================================================
// MODAL HỎI VỊ TRÍ GTA_SA.EXE (lần đầu mở app)
// =====================================================================
let gmReq = 0;
function gameBadges(r) {
  return `<span class="badge ${r.exe ? 'ok' : 'no'}">${r.exe ? '✓' : '✕'} gta_sa.exe</span><span class="badge ${r.sampDll ? 'ok' : 'no'}">${r.sampDll ? '✓' : '✕'} samp.dll</span>`;
}
let gmExeOk = false;
// Nút Xong chỉ bật khi file gta_sa.exe hợp lệ VÀ tên nhân vật hợp lệ
function gmUpdate() {
  const nameOk = validName(normName($('#gmName').value));
  $('#gmNameHint').classList.toggle('bad', !nameOk);
  $('#gmOk').disabled = !(gmExeOk && nameOk);
}
async function gmCheck() {
  const p = $('#gmPath').value.trim(), my = ++gmReq;
  gmExeOk = false; gmUpdate();
  if (!p) { $('#gmCheck').innerHTML = ''; return; }
  try {
    const r = await invoke('check_game', { exePath: p });
    if (my !== gmReq) return;
    $('#gmCheck').innerHTML = gameBadges(r) + (r.exe && !r.sampDll ? '<span class="hint">Thiếu samp.dll – hãy cài SA-MP/open.mp client vào thư mục game.</span>' : '');
    gmExeOk = !!r.exe; gmUpdate();
  } catch { if (my === gmReq) $('#gmCheck').innerHTML = ''; }
}
function openGameModal() {
  $('#gmPath').value = cfg.gtaPath || ''; $('#gmName').value = cfg.playerName; $('#gmCheck').innerHTML = '';
  gmExeOk = false; gmUpdate(); showBack($('#gameModal')); gmCheck();
}
function finishGameModal() { cfg.askedGame = true; save(); hideBack($('#gameModal')); }
function skipGame() { finishGameModal(); toast('Bạn có thể chọn gta_sa.exe và đổi tên nhân vật sau trong Cài đặt'); }
function confirmGame() {
  if ($('#gmOk').disabled) return;
  cfg.gtaPath = $('#gmPath').value.trim(); cfg.playerName = normName($('#gmName').value);
  finishGameModal(); toast('Đã lưu. Bạn có thể đổi tên nhân vật trong Cài đặt bất cứ lúc nào.');
}
$('#gmBrowse').onclick = async () => { const f = await pickGameExe(); if (f) { $('#gmPath').value = f; gmCheck(); } };
$('#gmPath').oninput = gmCheck;
$('#gmName').oninput = gmUpdate;
$('#gmSkip').onclick = skipGame;
$('#gmOk').onclick = confirmGame;
$('#gameModal').onmousedown = e => { if (e.target.id === 'gameModal') skipGame(); };
$('#btnSave').onclick = () => {
  const name = normName($('#setName').value);
  if (!validName(name)) return toast('Tên người chơi phải có dấu gạch dưới (_), VD: Max_Bennett', 'err');
  cfg.gtaPath = $('#setPath').value.trim(); cfg.playerName = name;
  save(); toast('Đã lưu cài đặt'); go('play');
};

// ---------- Khởi động ----------
renderServerSelect(); buildServerList(); renderPlayers();
refreshStatus();
if (!cfg.gtaPath && !cfg.askedGame) setTimeout(openGameModal, 350); // lần đầu mở app
})();
