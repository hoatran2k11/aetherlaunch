# Deploy server tải dữ liệu game

Launcher tải dữ liệu từ **một server file tĩnh** (không cần backend, không cần database). Server chỉ cần hai thứ:

```
https://data.yourdomain.com/manifest.json          <- danh sách file + SHA-256 + dung lượng
https://data.yourdomain.com/v1.0.0/gta_sa.exe      <- các file dữ liệu
https://data.yourdomain.com/v1.0.0/models/xxx.dff
```

Launcher đọc `manifest.json`, so từng file với bản trên máy người chơi (SHA-256), chỉ tải file thiếu hoặc sai, tải tiếp được khi đứt mạng (HTTP Range) và tự bỏ qua file đã đúng. Chạy lại = cập nhật / sửa file hỏng.

> **Bản quyền:** chỉ đưa lên server những file bạn có quyền phân phối: client SA-MP / open.mp, mod, map, model, âm thanh… của chính bạn. File gốc của GTA San Andreas (`gta3.img`, `gta_int.img`, `player.img`, `.exe` gốc…) thuộc Rockstar / Take-Two, đưa lên server công khai là vi phạm bản quyền và server có thể bị gỡ. Người chơi đã có game thì dùng nút **"Đã có game trên máy"** và chọn thư mục game của họ.

---

## 1. Chuẩn bị thư mục dữ liệu

Tạo thư mục chứa đúng cấu trúc sẽ được chép vào máy người chơi:

```
pack/
├─ gta_sa.exe        (cần có để launcher tự nhận game sau khi tải xong)
├─ samp.dll
├─ models/...
└─ ...
```

Tên file không được chứa `\ : * ? " < > |`, không có `..`, không kết thúc bằng dấu chấm hoặc dấu cách (script sẽ cảnh báo và bỏ qua file vi phạm).

## 2. Tạo manifest

Cần Node.js 18+. Từ thư mục dự án launcher:

```bash
node tools/make-manifest.mjs ./pack --version 1.0.0 --name "GTA Viet Nam Pack" \
     --base-url https://data.yourdomain.com/v1.0.0/ --out ./manifest.json
```

- `--version`: đổi mỗi lần phát hành bản mới (launcher so sánh chuỗi này để báo "Có bản cập nhật").
- `--base-url`: thư mục chứa các file trên server. Bỏ đi nếu `manifest.json` nằm cùng thư mục với file.
- Mỗi file có thể có thêm trường `"url"` riêng nếu bạn muốn host một file ở chỗ khác (CDN khác, GitHub Releases…).

## 3. Chọn nơi host

| Cách | Hợp với | Ghi chú |
|---|---|---|
| **Cloudflare R2** (khuyên dùng cho gói lớn) | Nhiều người tải, ngân sách thấp | Không tính phí băng thông ra (egress), 10 GB đầu miễn phí. Gắn domain riêng và bật cache của Cloudflare. |
| **VPS + Caddy** (có sẵn trong `server/`) | Gói nhỏ / vừa, muốn tự chủ | HTTPS tự động. Băng thông VPS là giới hạn: 2 GB × 1000 người = 2 TB. |
| **Backblaze B2 + Cloudflare** | Gói lớn, đã quen B2 | Tải qua Cloudflare thì không mất phí egress. |

Mọi cách đều phải trả file qua **HTTPS** (launcher từ chối http, trừ `localhost` để thử nghiệm) và hỗ trợ **Range** (đã có sẵn ở các cách trên).

### 3a. VPS + Caddy (nhanh nhất)

1. Mua VPS Linux (Ubuntu 24.04), cài Docker, trỏ DNS `data.yourdomain.com` về IP VPS.
2. Chép thư mục `server/` lên VPS, sửa `data.example.com` trong `Caddyfile` thành domain của bạn.
3. Chép dữ liệu vào `server/data/`:
   ```
   server/data/manifest.json
   server/data/v1.0.0/...        (nội dung thư mục pack/)
   ```
   Ví dụ từ máy của bạn: `rsync -avP ./pack/ user@vps:~/server/data/v1.0.0/` rồi `scp manifest.json user@vps:~/server/data/`
4. Chạy: `docker compose up -d`. Caddy tự xin chứng chỉ HTTPS (cổng 80 và 443 phải mở).

Nếu dùng nginx thay cho Caddy, cần ít nhất: `sendfile on;`, không bật `gzip` cho file game, và `add_header Cache-Control "no-cache"` riêng cho `/manifest.json`. nginx hỗ trợ Range mặc định cho file tĩnh.

### 3b. Cloudflare R2

1. Cloudflare Dashboard → R2 → tạo bucket (vd `gtavn-data`).
2. Bucket → Settings → **Custom Domains** → thêm `data.yourdomain.com` (domain phải dùng DNS của Cloudflare).
3. Upload bằng `rclone` (cấu hình remote kiểu S3 với endpoint R2):
   ```bash
   rclone copy ./pack r2:gtavn-data/v1.0.0 --progress --transfers 8
   rclone copyto ./manifest.json r2:gtavn-data/manifest.json
   ```
4. Cache: Rules → Cache Rules: `/manifest.json` = **Bypass cache**; `/v*/*` = Edge TTL 1 năm.

## 4. Cấu hình launcher

Mở `src/app.js`, sửa dòng:

```js
const DATA_MANIFEST_URL = 'https://data.yourdomain.com/manifest.json';
```

Để nguyên giá trị mẫu `data.example.com` thì tính năng tải dữ liệu bị tắt (nút vẫn hiện nhưng báo chưa cấu hình). Sau đó `npm run tauri build`.

## 5. Phát hành bản cập nhật

1. Đặt file mới vào `pack/`, tạo manifest với **version mới** và **`--base-url` thư mục phiên bản mới** (vd `.../v1.1.0/`).
2. Upload thư mục `v1.1.0/` **trước**, upload `manifest.json` **sau cùng**. Như vậy người chơi không bao giờ thấy manifest trỏ tới file chưa có.
3. Người chơi mở launcher sẽ thấy thông báo **"Có bản cập nhật dữ liệu game"** → bấm **Cập nhật**; chỉ file đổi mới được tải.

Không ghi đè file trong thư mục phiên bản đã phát hành (file đã bị CDN cache, hash sẽ lệch và launcher báo *Sai SHA-256*).

## 6. Kiểm tra

```bash
curl -sI https://data.yourdomain.com/manifest.json            # 200, nên có Cache-Control: no-cache
curl -sI -r 0-99 https://data.yourdomain.com/v1.0.0/gta_sa.exe # phải là 206 Partial Content (hỗ trợ Range)
```

## 7. Xử lý sự cố

| Thông báo trong launcher | Nguyên nhân / cách xử lý |
|---|---|
| *Link dữ liệu phải dùng https://* | `DATA_MANIFEST_URL` hoặc `baseUrl` dùng `http://`. Đổi sang https. |
| *Không kết nối được server dữ liệu* | DNS / tường lửa / chứng chỉ HTTPS hết hạn. Thử mở link manifest bằng trình duyệt. |
| *Không tải được X (HTTP 404)* | File chưa upload, hoặc `baseUrl` sai (thiếu dấu `/` cuối vẫn được, nhưng sai thư mục thì 404). |
| *Sai SHA-256 của X* | File trên server khác manifest: ghi đè file sau khi tạo manifest, hoặc CDN còn cache bản cũ. Tạo lại manifest và xoá cache. |
| *File X lớn hơn dung lượng khai báo* | Như trên: file đổi nhưng manifest cũ. |
| *Manifest sai định dạng* | JSON hỏng. Tạo lại bằng `make-manifest.mjs`. |
| Tải chậm / hay đứt | Băng thông VPS thấp: chuyển sang R2 + CDN. Launcher tự thử lại 4 lần và tải tiếp từ chỗ dừng. |

## 8. Bảo mật

- Manifest quyết định file nào được ghi vào máy người chơi. Bảo vệ quyền upload (khoá SSH, API token R2) như bảo vệ chữ ký phát hành phần mềm.
- Launcher chặn đường dẫn nguy hiểm (`..`, ổ đĩa, đường dẫn tuyệt đối) và chặn chuyển hướng sang http, nhưng không thể biết file của bạn có an toàn hay không. Quét virus bộ dữ liệu trước khi phát hành.
