# AetherLaunch (Tauri v2)

Anime + Material + Liquid Glass launcher cho SA-MP. Tác giả: Max Bennett (aoh20119).

## Cấu trúc
```
aetherlaunch/
├─ package.json            # chỉ cần @tauri-apps/cli
├─ app-icon.png            # icon nguồn -> chạy `npm run tauri icon app-icon.png`
├─ helper/
│  ├─ samp_helper.cpp      # helper x86: chạy gta_sa.exe suspended + inject samp.dll (theo sampcmd)
│  └─ build_helper.bat     # build bằng MSVC x86
├─ src/                    # Frontend thuần HTML/CSS/JS (không cần bundler)
│  ├─ resources/           # ẢNH TĨNH: app-icon.png, avatar.jpg, background.jpg (chép đè bằng ảnh thật)
│  ├─ index.html  style.css  app.js
└─ src-tauri/
   ├─ Cargo.toml  build.rs  tauri.conf.json
   ├─ capabilities/default.json
   ├─ resources/samp_helper.exe   # helper x86 (đi kèm bản build sẵn từ MinGW)
   └─ src/
      ├─ main.rs           # đăng ký command: query_server, check_game, launch_game
      ├─ query.rs          # SA-MP UDP query (i/r/d/p) + unit test
      └─ launch.rs         # kiểm tra input, gọi helper, dịch mã lỗi
```

## Chạy
```
npm install
npm run tauri icon app-icon.png     # tạo src-tauri/icons (chỉ cần 1 lần)
npm run tauri dev
npm run tauri build                 # ra bộ cài NSIS
```
Yêu cầu: Windows 10/11 (WebView2), Rust, Node.js, MSVC Build Tools.

## Build lại helper (tuỳ chọn)
Mở "x86 Native Tools Command Prompt" và chạy `helper\build_helper.bat`.
Helper PHẢI là 32-bit vì gta_sa.exe là 32-bit.

## Ghi chú
- Tên người chơi BẮT BUỘC có dấu gạch dưới (vd `Max_Bennett`); dấu cách tự đổi thành `_`.
- Server > 100 người không trả danh sách người chơi (giới hạn của SA-MP).
- Nếu gta_sa.exe cần quyền Administrator, hãy chạy launcher bằng Run as administrator.
