# ⚡ ZEN ROUTER v0.2.0

> **High-Performance Enterprise AI Gateway & Multi-Account Proxy** written in Rust, designed for **Google Antigravity / Code Assist OAuth**, with a modern Web Dashboard, Client API Keys management, Real-time Quota Monitor, Anti-Ban Protection Suite, and full Docker support.

---

## 🌟 Fitur Utama (v0.2.0)

- 🦀 **Rust Core + High-Performance Allocator (`mimalloc`)**:
  - Menggunakan `mimalloc` buatan Microsoft untuk mengurangi alokasi memori hingga 40% dan mencegah fragmentasi memori pada streaming SSE chunk.
  - Latensi proxy < 1ms, throughput tinggi, dan memory footprint minimal.
- ⚡ **Zero-Disk In-Memory Account Caching**:
  - Seluruh routing akun, pengecekan cooldown, dan pembacaan token dilakukan langsung di RAM (`RwLock<Vec<Account>>`).
  - **0ms disk latency**: Database SQLite murni difungsikan sebagai persistent background storage.
- 🛡️ **Anti-Ban Protection Suite**:
  - **Header Camouflage**: Emulasi client resmi `gccl/1.104.0`, dynamic session IDs, dan `X-Client-Version`.
  - **Sliding-Window Leaky Bucket**: Pembatasan otomatis maksimal 25 RPM per akun agar tidak menyentuh limit kuota Google.
  - **Anti-Burst Micro-Jitter (5-20ms)**: Menghilangkan pola request robotik yang serentak.
  - **Auto-Cooldown Isolation**: Akun yang terkena 429 otomatis diisolasi selama 60 detik agar kuota mendingin.
  - **Auto-Rehabilitation Task**: Background task berkala yang memulihkan health score akun (+5% per menit).
- 🔄 **Singleflight Anti-Stampede & Proactive Background Refresh**:
  - Jika banyak request paralel masuk bersamaan pada akun yang tokennya kedaluwarsa, hanya 1 request yang memanggil Google OAuth (Anti-Stampede Mutex).
  - Background task otomatis me-refresh token yang akan habis dalam 10 menit ke depan, sehingga user **tidak pernah menunggu proses token refresh**.
- 🔁 **Full Transparent Failover (401, 429, 500..=504)**:
  - Mengalihkan request secara instan dan transparan ke akun sehat lainnya bila terjadi rate limit, token kedaluwarsa, atau error server Google sebelum stream dimulai. IDE tidak akan menerima error.
- 🛠️ **Full Tools & Function Calling**:
  - Dukungan menyeluruh untuk OpenAI `tools`, `tool_choice`, legacy `functions`, dan Anthropic `tools`.
  - **Gemini Thought Signature Memory**: Menyimpan opaque `thoughtSignature` secara in-memory untuk multi-turn tool calling agar tidak ditolak oleh model Gemini 2.0 / 3.0.
- 🎛️ **Thinking Budget Control**:
  - Terjemahan parameter `reasoning_effort` (OpenAI) dan `thinking` (Anthropic).
  - Bisa mematikan thinking sepenuhnya (`budget: 0`) untuk respon secepat kilat atau mengatur kuota token penalaran.
- 💰 **Quota Protection & Stream Cancellation**:
  - Memutus stream upstream seketika dan mengirim frame HTTP/2 `RST_STREAM` ke Google jika user mengklik "Stop Generating" di IDE.
- 🌐 **Persistent Connection Pooling & HTTP/2 Multiplexing**:
  - Upstream keep-alive 120 detik, `tcp_nodelay(true)`, dan HTTP/2 active ping setiap 20 detik untuk mencegah koneksi diputus sepihak oleh firewall/Cloudflare.
  - Byte-level SSE buffer parsing untuk mencegah karakter UTF-8 terpotong antar paket TCP.
- 🏷️ **Live Build & Git Commit Tracking**:
  - Footer dashboard dan endpoint `/api/version` menampilkan versi rilis dan Git commit hash secara real-time agar Anda selalu tahu status update server.

---

## 🚀 Cara Menjalankan

### 1. Menjalankan via Docker Compose (Rekomendasi)

```bash
# Clone atau pull repository terbaru
git pull origin main

# Jalankan container di background dengan build ulang
docker compose up -d --build

# Cek logs
docker compose logs -f
```

Buka Dashboard di browser:
👉 **[http://localhost:20128](http://localhost:20128)**

### 2. Menjalankan secara Lokal (Native Rust)

```bash
cargo run --release
```

Server akan aktif di `http://localhost:20128`.

---

## 🔑 Langkah Awal: Hubungkan Akun & Buat API Key

1. **Buka Dashboard**: Akses `http://localhost:20128` di browser Anda.
2. **Hubungkan Akun Google**:
   - Klik tombol **"Add Google Account"** di kanan atas.
   - Login dengan akun Google Anda dan berikan izin akses.
   - Status akun, sisa kuota, dan badge kesehatan akan langsung tampil di Quota Monitor.
3. **Generate API Key**:
   - Masuk ke bagian **Client API Keys**.
   - Klik **+ Generate API Key**, beri nama (contoh: `Cursor Key` atau `Claude Code`).
   - Salin token yang dihasilkan (`zen-sk-xxxx...`).

---

## 🛠️ Integrasi dengan AI Coding Tools

### 1. Claude Code CLI
```bash
export ANTHROPIC_BASE_URL="http://localhost:20128"
export ANTHROPIC_API_KEY="zen-sk-your-token-here"
claude
```

### 2. Cursor / Cline / OpenCode / Roo Code
- **Provider**: `OpenAI Compatible`
- **Base URL**: `http://localhost:20128/v1`
- **API Key**: `zen-sk-your-token-here`
- **Model**: `gemini-2.5-flash` (kecepatan kilat) atau `gemini-3.8-flash-high` (deep reasoning)

### 3. Python (OpenAI SDK)
```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:20128/v1",
    api_key="zen-sk-your-token-here"
)

response = client.chat.completions.create(
    model="gemini-2.5-flash",
    messages=[{"role": "user", "content": "Halo Zen Router!"}],
    stream=True
)

for chunk in response:
    if chunk.choices and chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)
print()
```

### 4. Pengujian Langsung via cURL

**OpenAI Chat Completions (Streaming):**
```bash
curl -N http://localhost:20128/v1/chat/completions \
  -H "Authorization: Bearer zen-sk-your-token-here" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gemini-2.5-flash",
    "stream": true,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

**Anthropic Messages:**
```bash
curl http://localhost:20128/v1/messages \
  -H "x-api-key: zen-sk-your-token-here" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello Claude!"}]
  }'
```

---

## ⚡ Panduan Model & Latency

| Model ID | Level Thinking | Latency Rata-rata | Rekomendasi Penggunaan |
| :--- | :--- | :--- | :--- |
| **`gemini-2.5-flash`** | **None** | **~650 ms** ⚡ | **Paling Cepat / Rekomendasi Chat, Edit & Autocomplete Instan** |
| `gemini-3.5-flash-lite` | None | ~800 ms ⚡ | Hemat kuota token |
| `gemini-3.8-flash-low` | Low Thinking | ~1.500 ms 🚀 | Cepat dengan penalaran ringan |
| `gemini-3.8-flash-medium` | Medium Thinking | ~2.000 ms ⚖️ | Seimbang untuk tugas umum |
| `gemini-3.8-flash-high` | High Thinking | ~2.400 ms 🧠 | Analisis arsitektur & logika rumit |
| `gemini-2.5-pro` | None | ~1.200 ms 🌟 | Konteks kapasitas besar |
| `gpt-4o` | Mapped to 3.8 High | ~2.400 ms | OpenAI compatibility alias |
| `claude-3-5-sonnet-20241022` | Mapped to 3.8 High | ~2.400 ms | Anthropic compatibility alias |

---

## 📡 Daftar Endpoint API

| Method | Endpoint | Deskripsi |
|---|---|---|
| `GET` | `/` atau `/dashboard` | Modern Web Dashboard |
| `GET` | `/health` | Health check endpoint (termasuk status, versi & commit hash) |
| `GET` | `/api/version` | Versi, Git commit hash, dan build timestamp |
| `POST` | `/v1/chat/completions` | OpenAI format proxy endpoint (mendukung Tools & SSE stream) |
| `GET` | `/v1/models` | List model yang tersedia & metadata |
| `POST` | `/v1/messages` | Anthropic format proxy endpoint (mendukung Tools) |
| `GET` | `/api/quota` | Real-time Antigravity upstream quota monitor |
| `GET` | `/api/keys` | List client API keys |
| `POST` | `/api/keys` | Generate client API key baru |
| `DELETE`| `/api/keys/:id` | Hapus client API key |
| `GET` | `/api/oauth/login` | Redirect ke Google OAuth 2.0 login |
| `GET` | `/api/oauth/callback` | Callback penerima auth code Google |
| `GET` | `/api/accounts` | List semua akun yang terhubung |
| `POST` | `/api/accounts` | Tambah / Import akun manual |
| `POST` | `/api/accounts/:id/toggle`| Pause / Aktifkan kembali akun |
| `DELETE`| `/api/accounts/:id` | Hapus akun dari pool |
| `GET` | `/api/stats` | Real-time traffic & token statistics |
| `GET` | `/api/logs` | Riwayat request terakhir |

---

## ⚙️ Variabel Konfigurasi (`.env` / Docker)

| Variabel | Default | Keterangan |
|---|---|---|
| `PORT` | `20128` | Port server (sama dengan 9router) |
| `APP_ENV` | `production` | Environment mode (`production` / `development`) |
| `PUBLIC_BASE_URL` | `https://ai.yourdomain.com` | Domain publik untuk auto-redirect & snippet updater |
| `GOOGLE_CLIENT_ID` | `(default built-in)` | Client ID Google Antigravity |
| `GOOGLE_CLIENT_SECRET`| `(default built-in)` | Client Secret Google Antigravity |
| `REDIRECT_URI` | `http://localhost:20128/api/oauth/callback` | Callback redirect URL |
| `DATABASE_URL` | `sqlite:/app/data/zen-router.db` | Path SQLite database |
| `UPSTREAM_BASE_URL` | `https://daily-cloudcode-pa.googleapis.com` | Base URL upstream Antigravity |
| `UPSTREAM_LOCATION` | `us-central1` | Region Cloud Code model |
| `RUST_LOG` | `zen_router=info,tower_http=info` | Tracing log level |
