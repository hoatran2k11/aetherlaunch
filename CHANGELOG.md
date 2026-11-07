# Changelog

Tất cả thay đổi đáng chú ý của **AetherLaunch** được ghi lại ở đây.
Định dạng theo [Keep a Changelog](https://keepachangelog.com/vi/1.1.0/), đánh số phiên bản theo [SemVer](https://semver.org/lang/vi/):
thêm tính năng thì tăng số giữa (`0.6.0` → `0.7.0`), sửa lỗi nhỏ thì tăng số cuối (`0.6.0` → `0.6.1`).

## [0.6.0] - 2026-11-07

### Thêm
- **Discord Rich Presence**: hiển thị trên Discord đang ở launcher hay đang chơi, tên server, tên nhân vật, số người chơi (x of y), thời gian, địa chỉ server và nút mở forum.
  - Tự về trạng thái "Đang ở launcher" khi thoát game (dò `gta_sa.exe` mỗi 8 giây, hai lần liên tiếp không thấy).
  - Công tắc bật/tắt trong **Cài đặt**, kèm dòng báo trạng thái kết nối (đã kết nối / chưa thấy Discord / thiếu cấu hình).
  - Cấu hình bằng `.env` (`DISCORD_CLIENT_ID`, tên ảnh lớn/nhỏ, chữ rê chuột, một nút cố định); giá trị được nhúng vào bản build, có thể ghi đè bằng `.env` cạnh file `.exe` hoặc biến môi trường.
  - Chỉ gửi khi nội dung đổi và cách nhau tối thiểu 15 giây (giới hạn của Discord); tự kết nối lại khi Discord khởi động lại; hiện rõ lý do khi Discord từ chối trạng thái.
  - Thêm `.env.example` và hướng dẫn tạo ứng dụng trên Discord Developer Portal trong README.
- **Tab Forum**: lưu link forum riêng cho từng server, mở trong cửa sổ trình duyệt tích hợp của launcher (đăng nhập được giữ lại).
  - Tối đa 3 link đầu của server đang chọn hiện thành nút mở nhanh trên trang **Chơi ngay**.
  - Gợi ý link từ `weburl` mà server khai báo trong rules, thêm bằng một lần bấm.
  - Xoá link có nút **Hoàn tác** trong thông báo.
- **Thiết lập lần đầu**: khi mở app lần đầu hiện hộp thoại chọn vị trí `gta_sa.exe` (kiểm tra luôn `samp.dll`) và đặt tên nhân vật; có thể bấm **Để sau** (app ghi nhớ, không hỏi lại) và chỉnh trong **Cài đặt** bất cứ lúc nào.
- Cửa sổ tự **phóng to** khi mở.
- Thông báo (toast) hỗ trợ nút hành động và thời gian hiện tuỳ chỉnh.

### Thay đổi
- Logo ở sidebar hiển thị đúng ảnh gốc: khung không còn bo góc, cắt hay đổ nền chồng lên ảnh đã bo sẵn (khi ảnh lỗi vẫn có chữ AL dự phòng với khung gradient).
- Mỗi server lưu thêm danh sách link forum; sửa server không làm mất link forum.
- Đóng cửa sổ chính thì thoát hẳn ứng dụng, kể cả cửa sổ forum đang mở.
- Yêu cầu Rust từ 1.90 trở lên để tự build (Tauri 2.12); thêm thư viện `discord-rich-presence` và `dotenvy`.

### Bảo mật
- Cửa sổ forum chỉ mở link `http://` và `https://`; từ chối `javascript:`, `file:`, dạng `user@host`, khoảng trắng và link quá dài. Cửa sổ này không nằm trong capabilities nên trang forum không gọi được API của Tauri.
- Link nút của Discord được kiểm tra cùng quy tắc; `.env` được `.gitignore` bỏ qua (chỉ giữ `.env.example`) và README nhắc không đưa Client Secret hay Bot Token vào `.env`.

## [0.5.9] - 2026-11-02

Mốc phiên bản đầu tiên được ghi lại: gom toàn bộ quá trình phát triển từ lúc khởi tạo (số phiên bản nội bộ lúc đó là `0.1.0`) đến lần đổi số sang `0.5.9`. Chưa có bản phát hành nào trước mốc này.

### Thêm
- Launcher SA-MP bằng **Tauri v2**, giao diện phong cách anime kết hợp Material và Liquid Glass (kính mờ, viền sáng), ảnh nền phủ lớp tối nhẹ, hiệu ứng chuyển tab và hover mượt.
- Bốn tab: **Chơi ngay**, **Danh sách server**, **Cài đặt**, **Thông tin** (card kính mờ có glow, hiển thị Max Bennett / aoh20119).
- **Thông tin server thời gian thực** qua UDP theo giao thức query SA-MP: tên server, chế độ, ngôn ngữ, số người chơi hiện tại/tối đa, ping, mật khẩu, rules (phiên bản, map, thời gian, thời tiết, website) và danh sách người chơi (tên, điểm); tự làm mới mỗi 10 giây.
- **Nút Chơi ngay** gọi helper x86 (`samp_helper.exe`, dựa trên ý tưởng của `sampcmd`): tạo `gta_sa.exe` ở trạng thái tạm dừng, nạp `samp.dll` bằng remote thread, kiểm tra kết quả nạp rồi mới chạy; lỗi được dịch ra tiếng Việt (thiếu file, cần quyền Administrator, sai kiến trúc...).
- **Quản lý nhiều server**: thêm/sửa/xoá bằng hộp thoại (tên gợi nhớ, IP/domain, port, mật khẩu, mã hoá ký tự Windows-1252/1258/1250/1251), chọn nhanh bằng dropdown ngay trên trang Chơi ngay.
- **Cài đặt**: đường dẫn `gta_sa.exe` (chọn bằng hộp thoại, kiểm tra `gta_sa.exe` và `samp.dll`) và tên nhân vật; lưu cục bộ.
- Tên nhân vật bắt buộc có dấu gạch dưới `_` (dấu cách tự đổi thành `_`), mặc định `Max_Bennett`.
- Đặt tên ứng dụng **AetherLaunch**, logo và avatar bằng ảnh riêng.
- Ảnh tĩnh (`app-icon.png`, `avatar.jpg`, `background.jpg`) nằm trong `src/resources/`, tải offline, không phụ thuộc link ngoài; CSP siết lại chỉ cho ảnh nội bộ.
- Thông báo (toast) hiện 10 giây; cấu hình cũ tự được chuyển sang định dạng mới.
- `.gitignore`, bộ cài NSIS, cửa sổ không viền với thanh tiêu đề tự vẽ.

### Thay đổi
- Cấu hình server (IP, port, mật khẩu, mã hoá) rút khỏi **Cài đặt**, tập trung ở **Danh sách server** và **Chơi ngay**; tab Cài đặt chỉ còn đường dẫn game và tên nhân vật.
- Bảng người chơi chỉ còn hai cột **Tên** và **Điểm**, header cố định, chỉ vùng danh sách bên dưới mới cuộn.
- Bỏ dòng "Đã khởi chạy GTA:SA…" cạnh nút Chơi ngay (kết quả vẫn báo bằng thông báo).
- Form thêm/sửa server chuyển sang hộp thoại; ô nhập, dropdown và nút cùng chiều cao, canh lề thống nhất; nút Lưu của Cài đặt đồng bộ kiểu với các nút khác.
- Lấy danh sách người chơi bằng opcode `c` (tên + điểm) trước, `d` làm dự phòng.

### Sửa lỗi
- Timeout khi lấy danh sách người chơi (lỗi `10060`): các truy vấn UDP chạy tuần tự có nghỉ giữa các gói thay vì dồn dập, danh sách người chơi tải riêng để không chặn thẻ trạng thái, thông báo lỗi tiếng Việt kèm nút **Thử lại**, và giải thích riêng cho server từ 101 người (SA-MP không cung cấp danh sách).
- Blur kính bị nhấp nháy hoặc bật đột ngột khi chuyển tab; blur của tab cũ đè lên tab mới (do `visibility: hidden` giữ lại `backdrop-filter`): hiệu ứng chuyển nằm ở từng thẻ thay vì cả khối tab, tab ẩn bằng `display: none`, modal xử lý tương tự.
- Thanh cuộn nhấp nháy và nội dung giật khi vào tab, nội dung bị ghi đè lúc chuyển tab: giảm độ trượt, cố định khoảng trống thanh cuộn, chuyển tab chỉ làm mới âm thầm, chỉ ghi lại giao diện khi dữ liệu thật sự đổi, không truy vấn dồn dập khi bấm qua lại.
- Danh sách người chơi cuộn đè lên tiêu đề bảng.
- Chọn server khác lúc đang truy vấn có thể hiện nhầm dữ liệu của server cũ.

<details>
<summary>Dòng thời gian phát triển trước mốc 0.5.9 (số phiên bản nội bộ 0.1.0)</summary>

- **01/11**: khởi tạo dự án, giao diện bốn tab, truy vấn server, helper x86 và nút Chơi ngay; yêu cầu tên có `_`; đổi tên AetherLaunch; logo/avatar bằng link ngoài rồi chuyển sang link cố định; thêm `.gitignore`.
- **02/11**: tái cấu trúc UI (server tách khỏi Cài đặt, hộp thoại thêm server, dropdown nhanh, bảng người chơi hai cột); sửa lỗi blur khi chuyển tab; căn chỉnh form, nút và dropdown; đưa ảnh tĩnh về thư mục local; sửa thanh cuộn nhấp nháy; ẩn tab bằng `display: none`; bỏ dòng thông báo cạnh nút Chơi; toast 10 giây; đổi số phiên bản sang `0.5.9`.

</details>
