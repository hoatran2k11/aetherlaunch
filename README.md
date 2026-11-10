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
Yêu cầu: Windows 10/11 (WebView2), Rust (bản mới: Tauri 2.12 cần Rust ≥ 1.90, chạy `rustup update`), Node.js, MSVC Build Tools.

## Build lại helper (tuỳ chọn)
Mở "x86 Native Tools Command Prompt" và chạy `helper\build_helper.bat`.
Helper PHẢI là 32-bit vì gta_sa.exe là 32-bit.

## Ghi chú
- Tên người chơi BẮT BUỘC có dấu gạch dưới (vd `Max_Bennett`); dấu cách tự đổi thành `_`.
- Server > 100 người không trả danh sách người chơi (giới hạn của SA-MP).
- Nếu gta_sa.exe cần quyền Administrator, hãy chạy launcher bằng Run as administrator.

## Forum
- Tab **Forum**: mỗi server có danh sách link forum riêng (thêm / sửa / xoá, xoá có nút Hoàn tác).
- Link mở trong cửa sổ trình duyệt tích hợp (lệnh Rust `open_forum`, chỉ nhận http/https). Cửa sổ này không có quyền gọi API Tauri.
- Tối đa 3 link đầu của server đang chọn hiện thành nút nhanh ngay trên trang Chơi ngay.

## Discord Rich Presence

Hiển thị trên Discord của bạn: đang ở launcher hay đang chơi, tên server, tên nhân vật, số người chơi (x of y), thời gian đã chơi, địa chỉ server và nút tác giả.

### 1. Tạo ứng dụng trên Discord Developer Portal
1. Vào https://discord.com/developers/applications và đăng nhập, bấm **New Application**.
2. Đặt tên (VD: `AetherLaunch`) rồi đồng ý điều khoản. **Tên này là dòng in đậm** hiện trên Discord ("Đang chơi AetherLaunch").
3. Ở **General Information**: chọn ảnh **App Icon** (512×512), rồi bấm **Copy** ở mục **Application ID**.
4. Vào **Rich Presence → Art Assets** → **Add Image(s)**: tải 2 ảnh, đặt tên (key) đúng như dưới đây (chữ thường, không dấu cách):
   - `aether_logo`: logo AetherLaunch (ảnh lớn, nên 1024×1024, tối thiểu 512×512)
   - `gta_sa`: biểu tượng GTA SA (ảnh nhỏ góc dưới)
   Ảnh có thể mất vài phút mới hiện trên Discord.
5. Không cần Bot, OAuth2, Client Secret hay gửi duyệt/xác minh: Rich Presence chỉ cần Application ID.

### 2. Cấu hình `.env`
```
copy .env.example .env      (Windows)   hoặc   cp .env.example .env
```
Mở `.env`, dán Application ID vào `DISCORD_CLIENT_ID=`; các biến còn lại (tên ảnh, chữ rê chuột) xem chú thích trong file.
- Giá trị có dấu cách **không cần dấu nháy** (VD: `DISCORD_LARGE_TEXT=AetherLaunch của tôi`). Dòng bắt đầu bằng `#` là chú thích.
- Chạy `npm run tauri dev`: app đọc `.env` mỗi lần khởi động (đổi `.env` thì mở lại app).
- Chạy `npm run tauri build`: giá trị `DISCORD_*` được nhúng vào file .exe, người dùng không cần file `.env`. Đổi `.env` thì build lại.
- Muốn ghi đè khi chạy: đặt file `.env` cạnh file .exe hoặc đặt biến môi trường thật (được ưu tiên hơn giá trị nhúng).

### 3. Kiểm tra
1. Mở **ứng dụng Discord trên máy tính** (bản web/điện thoại không dùng được) và bật *User Settings → Activity Privacy → Share my activity / Display current activity as a status message*.
2. Mở AetherLaunch → **Cài đặt**: dòng dưới công tắc *Discord Rich Presence* phải báo **Đã kết nối Discord**.
3. Thẻ Discord có đúng **1 nút cố định**: *Created by Max Bennett* (nhãn và link nằm cứng trong `src/app.js`, hằng số `AUTHOR_BUTTON`; sửa ở đó nếu muốn đổi). Dòng trạng thái ở Cài đặt ghi *Đang gửi 1 nút: …*, đó là nút launcher thật sự gửi cho Discord.
   **Lưu ý:** Discord không hiện nút trên thẻ của chính bạn, chỉ người khác thấy. Hãy nhờ bạn bè hoặc dùng tài khoản phụ để xem, hoặc dựa vào dòng *Đang gửi 1 nút* để biết launcher đã gửi chưa.

### Xử lý sự cố
| Hiện tượng | Cách xử lý |
|---|---|
| Hint báo *Chưa cấu hình / không hợp lệ DISCORD_CLIENT_ID* | Kiểm tra `.env` (17–20 chữ số, không để chữ `your_application_id`), mở lại app |
| *Chưa thấy Discord đang chạy* | Mở Discord desktop. Nếu launcher chạy bằng *Run as administrator* thì Discord cũng cần cùng quyền (hai bên khác quyền thường không kết nối được) |
| Không thấy nút trên thẻ | Nếu bạn đang nhìn thẻ của chính mình thì đây là bình thường (Discord không hiện nút cho chủ thẻ), hãy nhờ người khác xem. Nếu vẫn không thấy, xem dòng *Đang gửi 1 nút* ở Cài đặt: không thấy dòng đó nghĩa là đang chạy bản cũ (thay file mới rồi mở lại / build lại app) |
| Có trạng thái nhưng không có ảnh | Sai tên key ảnh, hoặc ảnh mới tải còn đang xử lý; kiểm tra tên trong Art Assets khớp `DISCORD_LARGE_IMAGE` / `DISCORD_SMALL_IMAGE` |
| Hint báo *Discord từ chối trạng thái: …* | Đọc nội dung lỗi (thường do link nút sai); trạng thái được gửi lại ở lần cập nhật sau |
| Không muốn hiện | Tắt công tắc *Discord Rich Presence* ở Cài đặt rồi Lưu: trạng thái bị xoá ngay |

Ghi chú: Discord giới hạn tần suất cập nhật nên launcher chỉ gửi khi nội dung đổi và cách nhau tối thiểu 15 giây. Tên nhân vật và địa chỉ server hiển thị công khai khi đang chơi, hãy tắt công tắc nếu không muốn chia sẻ.
