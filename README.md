# ⚡ ZEN ROUTER

> **High-Performance AI Gateway & Multi-Account Proxy** written in Rust, designed for **Google Antigravity / Code Assist OAuth**, with a modern Web Dashboard, Client API Keys management, Real-time Quota Monitor, and full Docker support (similar to **9router**).

---

## 🌟 Fitur Utama

- 🦀 **Rust Core (Axum 0.7 + Tokio + SQLx)**: Latensi sangat rendah (< 1ms proxy overhead), memory footprint kecil, dan streaming SSE ultra-cepat.
- 🔐 **Google Antigravity OAuth 2.0 + PKCE**: Autentikasi langsung dengan akun Google menggunakan client ID resmi Google Antigravity.
- 🔄 **Multi-Account Pool & Auto-Failover**:
  - Pool banyak akun Google sekaligus
  - Otomatis rotasi akun saat terkena `429 Too Many Requests` (Quota exhaustion)
  - Health scoring per-akun (0 - 100%) dan auto token refresh
- 🛡️ **Lossless Token Saver & Smart Thinking Budget**:
  - Kompresi whitespace dan baris kosong berlebih tanpa merusak indentasi kode atau markdown
  - Deduplikasi otomatis blok system prompt yang berulang
  - **Dynamic Thinking Budget**: Secara cerdas menyetel thinking level ke `LOW` untuk pertanyaan sederhana (< 160 karakter tanpa kode) guna menghemat 70% token penalaran & memangkas latency menjadi < 800ms, serta otomatis `HIGH` untuk pertanyaan koding kompleks
- 🌐 **Universal Deployment (Localhost, Node IP, Domain & Cloudflare Tunnel)**:
  - Berjalan mulus di `http://localhost:20128`, IP internal server, maupun domain publik `https://9r.alinlab.cloud`
  - Mendukung **Remote OAuth Callback Exchange**: Pengguna domain publik atau perangkat remote dapat menghubungkan akun Google tanpa perlu membuka localhost
- 🎨 **Premium Modern Web Dashboard**:
  - Dark obsidian theme terinspirasi oleh Linear dan Vercel
  - Bebas dari emoji norak, menggunakan Vector SVG icons bersih
  - Toast notifications instan tanpa dialog alert browser
  - Interactive Playground dengan real-time SSE streaming dan latency timer
- 🔑 **Client API Keys (Downstream Auth)**:
  - Generate token akses berformat `zen-sk-...` untuk klien eksternal
  - Amankan router agar hanya klien terverifikasi yang dapat memanggil model
- ⚡ **Real-Time Quota Monitor**:
  - Pantau kapasitas sisa kuota upstream Antigravity langsung dari dashboard (persentase & visual bar)
  - Countdown reset waktu harian otomatis
- 🔀 **Dual Protocol Bridge**:
  - **OpenAI Compatible**: `POST /v1/chat/completions` (mendukung streaming SSE dan format chat)
  - **Anthropic Compatible**: `POST /v1/messages` (kompatibel penuh dengan **Claude Code**)
- 🤖 **Dynamic Models & Thinking Levels**:
  - `gemini-2.5-flash`: **Ultra Fast (~650ms)** tanpa thinking (Rekomendasi untuk chat/coding instan)
  - `gemini-3.8-flash-low`: **Fast (~1.5s)** dengan penalaran ringan
  - `gemini-3.8-flash-medium`: **Balanced (~2.0s)**
  - `gemini-3.8-flash-high`: **Deep Reasoning (~2.4s)** untuk analisis kompleks
  - `gemini-2.5-pro`: Model reasoning kapasitas tinggi
  - Alias kompatibilitas: `gpt-4o`, `claude-3-5-sonnet-20241022`, dll.
- 📊 **Modern Web Dashboard & Interactive Playground**:
  - Live statistics (Requests per minute, active accounts, success rate, token usage)
  - Live Playground dengan real-time streaming SSE and latency timer
  - Account manager (OAuth login, direct token import, pause/resume, delete)
  - Tab Panduan Integrasi (Claude Code, Cursor/Cline, Python SDK, cURL)
- 🐳 **Ready-to-run Docker**:
  - Multi-stage Dockerfile berbasis Alpine (image ringan ~25MB)
  - SQLite data persistence via Docker volume (`zen-router-data`)
  - Health check otomatis

---

## 🚀 Cara Menjalankan

### 1. Menjalankan via Docker Compose (Rekomendasi)

```bash
# Masuk ke direktori
cd zen-router

# Jalankan container di background
docker compose up -d

# Cek logs
docker compose logs -f
```

Buka Dashboard di browser:
👉 **[http://localhost:20128](http://localhost:20128)**

### 2. Menjalankan secara Lokal (Native Rust)

Jika ingin build & run secara lokal tanpa Docker:

```bash
cargo run --release
```

Server akan aktif di `http://localhost:20128`.

---

## 🔑 Langkah Awal: Hubungkan Akun & Buat API Key

1. **Buka Dashboard**: Buka `http://localhost:20128` di browser Anda.
2. **Hubungkan Akun Google**:
   - Klik tombol **"Add Google Account"** di kanan atas.
   - Login dengan akun Google Anda dan berikan izin akses.
   - Status akun dan sisa kuota (misal: 74%) akan langsung tampil di Quota Monitor.
3. **Generate API Key**:
   - Masuk ke bagian **Client API Keys**.
   - Klik **+ Generate API Key**, beri nama (contoh: `Claude Code Key`).
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
- **Model**: `gemini-2.5-flash` (untuk kecepatan instan) atau `gemini-3.8-flash-high`

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

| Model ID | Level Thinking | Latency | Kategori Penggunaan |
| :--- | :--- | :--- | :--- |
| **`gemini-2.5-flash`** | **None** | **~650 ms** ⚡ | **Paling Cepat / Rekomendasi Chat & Edit Instan** |
| `gemini-3.5-flash-lite` | None | ~800 ms ⚡ | Ringan & hemat kuota |
| `gemini-3.8-flash-low` | Low Thinking | ~1.500 ms 🚀 | Cepat dengan sedikit penalaran |
| `gemini-3.8-flash-medium` | Medium Thinking | ~2.000 ms ⚖️ | Seimbang |
| `gemini-3.8-flash-high` | High Thinking | ~2.400 ms 🧠 | Analisis arsitektur & logika rumit |
| `gemini-2.5-pro` | None | ~1.200 ms 🌟 | Kapasitas model besar |
| `gpt-4o` | Mapped to 3.8 High | ~2.400 ms | OpenAI compatibility alias |
| `claude-3-5-sonnet-20241022` | Mapped to 3.8 High | ~2.400 ms | Anthropic compatibility alias |

> **Catatan Latency:** Model dengan level *Thinking HIGH* memerlukan 2–3 detik karena server Google memproses ratusan token penalaran internal (*chain-of-thought*) sebelum mengeluarkan teks jawaban. Jika ingin respon secepat kilat, gunakan `gemini-2.5-flash`.

---

## 📡 Daftar Endpoint API

| Method | Endpoint | Deskripsi |
|---|---|---|
| `GET` | `/` atau `/dashboard` | Modern Web Dashboard |
| `GET` | `/health` | Health check endpoint |
| `POST` | `/v1/chat/completions` | OpenAI format proxy endpoint (mendukung SSE stream) |
| `GET` | `/v1/models` | List model yang tersedia & metadata |
| `POST` | `/v1/messages` | Anthropic format proxy endpoint |
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
| `PUBLIC_BASE_URL` | `https://9r.alinlab.cloud` | Domain publik untuk auto-redirect & snippet updater |
| `GOOGLE_CLIENT_ID` | `(default built-in)` | Client ID Google Antigravity |
| `GOOGLE_CLIENT_SECRET`| `(default built-in)` | Client Secret Google Antigravity |
| `REDIRECT_URI` | `http://localhost:20128/api/oauth/callback` | Callback redirect URL |
| `DATABASE_URL` | `sqlite:/app/data/zen-router.db` | Path SQLite database |
| `UPSTREAM_BASE_URL` | `https://daily-cloudcode-pa.googleapis.com` | Base URL upstream Antigravity |
| `UPSTREAM_LOCATION` | `us-central1` | Region Cloud Code model |
| `RUST_LOG` | `zen_router=info,tower_http=info` | Tracing log level |
