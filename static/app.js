/* ============================================================
   ZEN ROUTER — Dashboard JavaScript
   ============================================================ */

const API_BASE = '';
let refreshInterval = null;
let cachedApiKeys = [];
let modelsLoaded = false;
let currentGitCommit = '';

// ─── Init ────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  updateDynamicSnippets();
  fetchVersion();
  refreshAll();
  refreshInterval = setInterval(refreshAll, 5000);
});

function updateDynamicSnippets() {
  const origin = window.location.origin;
  document.querySelectorAll('.code-block-content').forEach(block => {
    block.textContent = block.textContent.replaceAll('http://localhost:20128', origin);
  });
}

async function fetchVersion() {
  try {
    const res = await fetch(`${API_BASE}/api/version`);
    if (res.ok) {
      const data = await res.json();
      currentGitCommit = data.commit || '';
      const versionStr = `v${data.version || '0.2.0'}`;

      const headerVer = document.getElementById('headerVersion');
      if (headerVer) headerVer.textContent = versionStr;

      const footerVer = document.getElementById('footerVersion');
      if (footerVer) footerVer.textContent = versionStr;

      const footerCommit = document.getElementById('footerCommitHash');
      if (footerCommit) {
        footerCommit.textContent = data.commit ? `commit ${data.commit}` : 'commit dev';
      }

      const footerTime = document.getElementById('footerBuildTime');
      if (footerTime && data.build_time) {
        footerTime.textContent = `Built: ${data.build_time}`;
      }
    }
  } catch (e) {
    console.debug('Failed to fetch version info', e);
  }
}

function copyCommitHash() {
  if (!currentGitCommit) return;
  navigator.clipboard.writeText(currentGitCommit).then(() => {
    showToast(`Commit hash copied: ${currentGitCommit}`);
  }).catch(() => {
    showToast(`Commit: ${currentGitCommit}`);
  });
}

async function refreshAll() {
  await Promise.allSettled([
    fetchStats(),
    fetchQuota(),
    fetchAccounts(),
    fetchKeys(),
    fetchModels(),
    fetchLogs(),
  ]);
}

// ─── Toast Notification ──────────────────────────────────────

function showToast(msg) {
  const container = document.getElementById('toastContainer');
  if (!container) return;
  const toast = document.createElement('div');
  toast.className = 'toast-pill';
  toast.innerHTML = `
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="20 6 9 17 4 12"/>
    </svg>
    <span>${escapeHtml(msg)}</span>
  `;
  container.appendChild(toast);
  setTimeout(() => {
    toast.classList.add('hide');
    setTimeout(() => toast.remove(), 300);
  }, 2400);
}

// ─── Quota Monitor ───────────────────────────────────────────

async function fetchQuota() {
  try {
    const res = await fetch(`${API_BASE}/api/quota`);
    if (!res.ok) return;
    const data = await res.json();
    renderQuota(data);
  } catch (e) {
    console.error('Quota fetch failed:', e);
  }
}

function renderQuota(data) {
  if (!data || !data.success) return;
  const percent = data.remaining_percent ?? 0;
  setText('quotaPercent', `${percent.toFixed(1)}%`);
  setText('quotaAccount', data.account || 'Connected Account');

  const bar = document.getElementById('quotaProgressBar');
  if (bar) {
    bar.style.width = `${Math.min(100, Math.max(0, percent))}%`;
    bar.style.backgroundColor = percent > 50 ? '#22c55e' : percent > 20 ? '#eab308' : '#ef4444';
  }

  const resetBadge = document.getElementById('quotaResetTime');
  if (resetBadge && data.reset_time) {
    resetBadge.textContent = `Reset dalam: ${formatRemainingTime(data.reset_time)}`;
  }

  const claudeStatus = document.getElementById('claudeStatus');
  if (claudeStatus && data.claude_reset_time) {
    claudeStatus.textContent = `Reset dalam ${formatRemainingTime(data.claude_reset_time)}`;
  }
}

function formatRemainingTime(isoDate) {
  if (!isoDate) return 'Normal';
  const target = new Date(isoDate).getTime();
  const diff = Math.max(0, Math.round((target - Date.now()) / 1000));
  const hours = Math.floor(diff / 3600);
  const minutes = Math.floor((diff % 3600) / 60);
  if (hours > 0) return `${hours}j ${minutes}m`;
  return `${minutes} menit`;
}

// ─── OAuth ───────────────────────────────────────────────────

function initiateOAuth() {
  window.location.href = `${API_BASE}/api/oauth/login`;
}

// ─── Stats ───────────────────────────────────────────────────

async function fetchStats() {
  try {
    const res = await fetch(`${API_BASE}/api/stats`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    renderStats(data);
    setStatus('online');
  } catch (e) {
    console.error('Stats fetch failed:', e);
    setStatus('offline');
  }
}

function renderStats(data) {
  setText('totalRequests', formatNumber(data.total_requests ?? 0));
  setText('activeAccounts', formatNumber(data.active_accounts ?? 0));
  setText('successRate', `${(data.success_rate ?? 0).toFixed(1)}%`);
  setText('avgLatency', `${Math.round(data.avg_latency_ms ?? 0)}ms`);
  setText('reqPerMin', `${data.requests_last_minute ?? 0} /min`);

  const inputT = data.total_input_tokens ?? 0;
  const outputT = data.total_output_tokens ?? 0;
  setText('inputTokens', formatNumber(inputT));
  setText('outputTokens', formatNumber(outputT));
  setText('totalTokens', formatNumber(inputT + outputT));
}

// ─── Accounts ────────────────────────────────────────────────

async function fetchAccounts() {
  try {
    const res = await fetch(`${API_BASE}/api/accounts`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    renderAccounts(data.accounts ?? []);
  } catch (e) {
    console.error('Accounts fetch failed:', e);
  }
}

function renderAccounts(accounts) {
  const tbody = document.getElementById('accountsBody');
  if (!tbody) return;

  if (accounts.length === 0) {
    tbody.innerHTML = `
      <tr class="empty-row">
        <td colspan="7">
          <div class="empty-state">
            <div class="empty-icon-svg">
              <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/>
                <circle cx="9" cy="7" r="4"/>
                <line x1="17" y1="11" x2="23" y2="11"/>
              </svg>
            </div>
            <p>No accounts connected yet</p>
            <button class="btn btn-primary btn-sm" onclick="initiateOAuth()">Connect Google Account</button>
          </div>
        </td>
      </tr>`;
    return;
  }

  tbody.innerHTML = accounts.map(acc => {
    const health = Math.round(acc.health_score ?? 0);
    const healthColor = health > 60 ? '#22c55e' : health > 30 ? '#f59e0b' : '#ef4444';
    let statusBadge = acc.is_active
      ? `<span class="badge badge-success">Active</span>`
      : `<span class="badge badge-inactive">Inactive</span>`;
    if (acc.cooldown_remaining_seconds && acc.cooldown_remaining_seconds > 0) {
      statusBadge = `<span class="badge" style="background:rgba(234,179,8,0.15);color:#eab308;border:1px solid rgba(234,179,8,0.3);" title="Cooldown Anti-Ban aktif">Cooldown (${acc.cooldown_remaining_seconds}s)</span>`;
    }
    const lastUsed = acc.last_used ? timeAgo(acc.last_used) : 'Never';
    const tokenExpiry = acc.token_expires_in != null
      ? (acc.token_expires_in > 0 ? `${Math.round(acc.token_expires_in / 60)}m` : 'Expired')
      : 'Unknown';

    return `
      <tr>
        <td>
          <div style="display:flex;flex-direction:column;gap:0.15rem;">
            <span style="color:var(--text-primary);font-weight:600;font-size:0.85rem;">${escapeHtml(acc.email)}</span>
            <span style="font-size:0.7rem;color:var(--text-muted);font-family:monospace;">Token expiry: ${tokenExpiry}</span>
          </div>
        </td>
        <td>${statusBadge}</td>
        <td>
          <div class="health-bar-wrap">
            <div class="health-bar">
              <div class="health-fill" style="width:${health}%;background:${healthColor};"></div>
            </div>
            <span class="health-score-text">${health}%</span>
          </div>
        </td>
        <td style="color:var(--text-primary);font-weight:500;">${formatNumber(acc.request_count ?? 0)}</td>
        <td style="color:${(acc.error_count ?? 0) > 0 ? 'var(--accent-orange)' : 'var(--text-muted)'};font-weight:500;">${formatNumber(acc.error_count ?? 0)}</td>
        <td style="color:var(--text-muted);font-size:0.78rem;">${lastUsed}</td>
        <td>
          <div style="display:flex;gap:0.4rem;">
            <button class="btn btn-outline btn-xs" onclick="toggleAccount('${acc.id}')">
              ${acc.is_active ? 'Pause' : 'Resume'}
            </button>
            <button class="btn btn-danger btn-xs" onclick="deleteAccount('${acc.id}', '${escapeHtml(acc.email)}')">
              Remove
            </button>
          </div>
        </td>
      </tr>`;
  }).join('');
}

async function toggleAccount(id) {
  try {
    const res = await fetch(`${API_BASE}/api/accounts/${id}/toggle`, { method: 'POST' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    await fetchAccounts();
  } catch (e) {
    console.error('Toggle failed:', e);
    alert('Failed to toggle account: ' + e.message);
  }
}

async function deleteAccount(id, email) {
  if (!confirm(`Remove account "${email}"? This cannot be undone.`)) return;
  try {
    const res = await fetch(`${API_BASE}/api/accounts/${id}`, { method: 'DELETE' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    await fetchAccounts();
    await fetchStats();
  } catch (e) {
    console.error('Delete failed:', e);
    alert('Failed to delete account: ' + e.message);
  }
}

// ─── Client API Keys ─────────────────────────────────────────

async function fetchKeys() {
  try {
    const res = await fetch(`${API_BASE}/api/keys`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    cachedApiKeys = data.keys ?? [];
    renderKeys(cachedApiKeys);
  } catch (e) {
    console.error('Keys fetch failed:', e);
  }
}

function renderKeys(keys) {
  const tbody = document.getElementById('keysBody');
  if (!tbody) return;

  if (keys.length === 0) {
    tbody.innerHTML = `
      <tr class="empty-row">
        <td colspan="6">
          <div class="empty-state">
            <div class="empty-icon-svg">
              <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="7.5" cy="15.5" r="5.5"/>
                <path d="M21 2l-9.6 9.6"/>
                <path d="M15.5 7.5l3 3L22 7l-3-3"/>
              </svg>
            </div>
            <p>Belum ada API Key. (Jika belum ada key, semua request bebas diakses tanpa token. Buat key untuk mengamankan router!)</p>
            <button class="btn btn-primary btn-sm" onclick="openNewKeyModal()">Generate API Key Pertama</button>
          </div>
        </td>
      </tr>`;
    return;
  }

  tbody.innerHTML = keys.map(k => {
    const masked = k.key.length > 18
      ? `${k.key.substring(0, 10)}...${k.key.substring(k.key.length - 6)}`
      : k.key;
    const lastUsed = k.last_used ? timeAgo(k.last_used) : 'Never';
    const created = k.created_at ? formatTime(k.created_at) : '—';
    const statusBadge = k.is_active
      ? `<span class="badge badge-success">Active</span>`
      : `<span class="badge badge-inactive">Inactive</span>`;

    return `
      <tr>
        <td>
          <span style="font-weight:600;color:var(--text-primary);">${escapeHtml(k.name)}</span>
        </td>
        <td>
          <div class="key-code">
            <span>${escapeHtml(masked)}</span>
            <button class="btn btn-outline btn-xs" style="padding:0.15rem 0.45rem;font-size:0.7rem;" onclick="copySnippet('${escapeHtml(k.key)}')">
              Copy
            </button>
          </div>
        </td>
        <td style="color:var(--text-muted);font-size:0.78rem;">${created}</td>
        <td style="color:var(--text-muted);font-size:0.78rem;">${lastUsed}</td>
        <td>${statusBadge}</td>
        <td>
          <button class="btn btn-danger btn-xs" onclick="deleteApiKey('${k.id}', '${escapeHtml(k.name)}')">
            Revoke
          </button>
        </td>
      </tr>`;
  }).join('');
}

async function deleteApiKey(id, name) {
  if (!confirm(`Revoke API Key "${name}"? Klien eksternal yang memakai key ini akan langsung ditolak.`)) return;
  try {
    const res = await fetch(`${API_BASE}/api/keys/${id}`, { method: 'DELETE' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    await fetchKeys();
  } catch (err) {
    alert('Failed to revoke API key: ' + err.message);
  }
}

// ─── Models ──────────────────────────────────────────────────

async function fetchModels() {
  try {
    const res = await fetch(`${API_BASE}/v1/models`);
    if (!res.ok) return;
    const data = await res.json();
    const models = data.data ?? [];

    const badge = document.getElementById('modelsCountBadge');
    if (badge) badge.textContent = models.length;

    const select = document.getElementById('pgModel');
    if (select && (!modelsLoaded || select.options.length <= 1)) {
      const currentVal = select.value;
      select.innerHTML = models.map(m => {
        let label = m.id;
        if (m.id.includes('high')) label += ' [Thinking: HIGH]';
        else if (m.id.includes('medium')) label += ' [Thinking: MEDIUM]';
        else if (m.id.includes('low')) label += ' [Thinking: LOW]';
        else if (m.id.startsWith('gpt')) label += ' [OpenAI Alias]';
        else if (m.id.startsWith('claude')) label += ' [Anthropic Alias]';
        return `<option value="${escapeHtml(m.id)}">${escapeHtml(label)}</option>`;
      }).join('');
      if (currentVal) select.value = currentVal;
      modelsLoaded = true;
    }
  } catch (e) {
    console.warn('Failed to load models list dynamically:', e);
  }
}

// ─── Logs ────────────────────────────────────────────────────

async function fetchLogs() {
  try {
    const res = await fetch(`${API_BASE}/api/logs`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    renderLogs(data.logs ?? []);
  } catch (e) {
    console.error('Logs fetch failed:', e);
  }
}

function renderLogs(logs) {
  const tbody = document.getElementById('logsBody');
  if (!tbody) return;

  if (logs.length === 0) {
    tbody.innerHTML = `
      <tr class="empty-row">
        <td colspan="6">
          <div class="empty-state">
            <div class="empty-icon-svg">
              <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                <polyline points="14 2 14 8 20 8"/>
                <line x1="16" y1="13" x2="8" y2="13"/>
                <line x1="16" y1="17" x2="8" y2="17"/>
              </svg>
            </div>
            <p>No requests yet</p>
          </div>
        </td>
      </tr>`;
    return;
  }

  tbody.innerHTML = logs.slice(0, 50).map(log => {
    const statusBadge = log.status === 'success'
      ? `<span class="badge badge-success">success</span>`
      : `<span class="badge badge-error">${escapeHtml(log.status)}</span>`;
    const latencyColor = (log.latency_ms ?? 0) < 2000 ? 'var(--accent-green)' : (log.latency_ms ?? 0) < 5000 ? 'var(--accent-orange)' : 'var(--accent-red)';

    return `
      <tr>
        <td style="font-size:0.75rem;color:var(--text-muted);font-family:monospace;">
          ${formatTime(log.created_at)}
        </td>
        <td>
          <code style="font-size:0.75rem;color:var(--text-secondary);">${escapeHtml(log.model)}</code>
        </td>
        <td>${statusBadge}</td>
        <td style="color:${latencyColor};font-weight:600;">${log.latency_ms ?? 0}ms</td>
        <td style="color:var(--text-muted);">${formatNumber(log.input_tokens ?? 0)}</td>
        <td style="color:var(--text-muted);">${formatNumber(log.output_tokens ?? 0)}</td>
      </tr>`;
  }).join('');
}

// ─── Status ──────────────────────────────────────────────────

function setStatus(state) {
  const pill = document.getElementById('statusPill');
  if (!pill) return;
  pill.className = `status-pill ${state}`;
  pill.querySelector('span:last-child').textContent = state === 'online' ? 'Online' : 'Offline';
}

// ─── Helpers ─────────────────────────────────────────────────

function setText(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
}

function formatNumber(n) {
  if (n == null) return '—';
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toString();
}

function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function timeAgo(isoDate) {
  if (!isoDate) return 'Never';
  const diff = (Date.now() - new Date(isoDate).getTime()) / 1000;
  if (diff < 60) return `${Math.round(diff)}s ago`;
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  return `${Math.round(diff / 86400)}d ago`;
}

function formatTime(isoDate) {
  if (!isoDate) return '—';
  const d = new Date(isoDate);
  return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

// ─── Modal Functions ─────────────────────────────────────────

function openOAuthModal() {
  const modal = document.getElementById('oauthModal');
  if (modal) modal.style.display = 'flex';
}

function closeOAuthModal() {
  const modal = document.getElementById('oauthModal');
  if (modal) modal.style.display = 'none';
}

async function openRemoteOAuthWindow() {
  const btn = document.getElementById('btnOpenAuthUrl');
  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `
      <svg class="spinner-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10" stroke-opacity="0.25"/><path d="M12 2a10 10 0 0 1 10 10" /></svg>
      <span>Membuka Google Login...</span>
    `;
  }

  try {
    const res = await fetch(`${API_BASE}/api/oauth/url`);
    const data = await res.json();
    if (data.url) {
      window.open(data.url, '_blank');
    } else {
      alert('Gagal mengambil OAuth URL: ' + (data.error || 'Unknown error'));
    }
  } catch (err) {
    alert('Network error: ' + err.message);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
        <span>1. Buka Halaman Login Google (Tab Baru)</span>
      `;
    }
  }
}

async function handleManualOAuthExchange(event) {
  event.preventDefault();
  const input = document.getElementById('manualOAuthUrl');
  const rawVal = input ? input.value.trim() : '';
  const btn = document.getElementById('btnExchangeCode');

  if (!rawVal) return;

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = `
      <svg class="spinner-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10" stroke-opacity="0.25"/><path d="M12 2a10 10 0 0 1 10 10" /></svg>
      <span>Menghubungkan akun...</span>
    `;
  }

  try {
    const res = await fetch(`${API_BASE}/api/oauth/exchange-code`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url: rawVal, code: rawVal })
    });

    const data = await res.json();
    if (!res.ok) {
      throw new Error(data.error || 'Gagal menukar kode auth');
    }

    showToast(data.message || 'Akun berhasil terhubung!');
    closeOAuthModal();
    if (input) input.value = '';
    await refreshAll();
  } catch (err) {
    alert('Error: ' + err.message);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>
        <span>2. Hubungkan Akun dari URL</span>
      `;
    }
  }
}

function openTokenModal() {
  const modal = document.getElementById('tokenModal');
  if (modal) {
    modal.style.display = 'flex';
    document.getElementById('modalEmail')?.focus();
  }
}

function closeTokenModal() {
  const modal = document.getElementById('tokenModal');
  if (modal) {
    modal.style.display = 'none';
    document.getElementById('tokenForm')?.reset();
  }
}

async function handleTokenSubmit(event) {
  event.preventDefault();
  const email = document.getElementById('modalEmail').value.trim();
  const accessToken = document.getElementById('modalAccessToken').value.trim();
  const refreshToken = document.getElementById('modalRefreshToken').value.trim() || null;
  const submitBtn = document.getElementById('modalSubmitBtn');

  if (!email || !accessToken) {
    alert('Please provide both email and access token.');
    return;
  }

  submitBtn.disabled = true;
  submitBtn.textContent = 'Saving...';

  try {
    const res = await fetch(`${API_BASE}/api/accounts`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: email,
        access_token: accessToken,
        refresh_token: refreshToken,
        expires_in_seconds: 3600
      })
    });

    const data = await res.json();
    if (!res.ok) {
      throw new Error(data.error || 'Failed to import account');
    }

    showToast('Account successfully imported!');
    closeTokenModal();
    await refreshAll();
  } catch (err) {
    console.error('Import error:', err);
    alert('Error saving account: ' + err.message);
  } finally {
    submitBtn.disabled = false;
    submitBtn.textContent = 'Save Account';
  }
}

// ─── Key Modal & Guide Tabs ───────────────────────────────────

function openNewKeyModal() {
  const modal = document.getElementById('newKeyModal');
  const formView = document.getElementById('newKeyFormView');
  const resultView = document.getElementById('newKeyResultView');
  const input = document.getElementById('newKeyName');
  if (modal) {
    modal.style.display = 'flex';
    if (formView) formView.style.display = 'block';
    if (resultView) resultView.style.display = 'none';
    if (input) {
      input.value = '';
      input.focus();
    }
  }
}

function closeNewKeyModal() {
  const modal = document.getElementById('newKeyModal');
  if (modal) modal.style.display = 'none';
}

async function handleCreateKeySubmit(event) {
  event.preventDefault();
  const nameInput = document.getElementById('newKeyName');
  const name = nameInput ? nameInput.value.trim() : '';
  const btn = document.getElementById('btnGenerateKey');
  if (btn) {
    btn.disabled = true;
    btn.textContent = 'Generating...';
  }

  try {
    const res = await fetch(`${API_BASE}/api/keys`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: name || 'Client Key' })
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to create key');

    const createdKey = data.key.key;
    document.getElementById('createdKeyValue').value = createdKey;
    document.getElementById('newKeyFormView').style.display = 'none';
    document.getElementById('newKeyResultView').style.display = 'block';
    showToast('API Key generated successfully');

    await fetchKeys();
  } catch (err) {
    alert('Failed to generate key: ' + err.message);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = 'Generate Key';
    }
  }
}

function copyCreatedKey() {
  const input = document.getElementById('createdKeyValue');
  if (input && input.value) {
    copySnippet(input.value);
  }
}

function switchGuideTab(tabId, btn) {
  document.querySelectorAll('.guide-tab-content').forEach(el => el.classList.remove('active'));
  document.querySelectorAll('.guide-tab-btn').forEach(el => el.classList.remove('active'));
  const target = document.getElementById(tabId);
  if (target) target.classList.add('active');
  if (btn) btn.classList.add('active');
}

// ─── Playground & Snippets ───────────────────────────────────

function copySnippet(text) {
  navigator.clipboard.writeText(text).then(() => {
    showToast('Copied to clipboard!');
  }).catch(() => {
    prompt('Copy to clipboard:', text);
  });
}

async function runPlaygroundTest() {
  const model = document.getElementById('pgModel').value;
  const promptText = document.getElementById('pgPrompt').value.trim();
  const sendBtn = document.getElementById('pgSendBtn');
  const resultBox = document.getElementById('pgResultBox');
  const outputEl = document.getElementById('pgOutput');
  const metaEl = document.getElementById('pgMeta');
  const streamEnabled = document.getElementById('pgStreamToggle')?.checked ?? true;

  if (!promptText) return;

  sendBtn.disabled = true;
  sendBtn.innerHTML = `
    <svg class="spinner-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10" stroke-opacity="0.25"/><path d="M12 2a10 10 0 0 1 10 10" /></svg>
    <span>Processing...</span>
  `;
  resultBox.style.display = 'flex';
  outputEl.textContent = 'Connecting to model via ZEN ROUTER...';
  outputEl.style.color = 'var(--text-primary)';
  metaEl.textContent = 'Measuring latency...';

  const startTime = Date.now();

  try {
    const isAnthropic = model.startsWith('claude');
    let url = isAnthropic ? '/v1/messages' : '/v1/chat/completions';
    let payload = isAnthropic
      ? { model: model, max_tokens: 1024, stream: streamEnabled, messages: [{ role: 'user', content: promptText }] }
      : { model: model, stream: streamEnabled, messages: [{ role: 'user', content: promptText }] };

    const headers = { 'Content-Type': 'application/json' };
    if (cachedApiKeys.length > 0) {
      if (isAnthropic) {
        headers['x-api-key'] = cachedApiKeys[0].key;
      } else {
        headers['Authorization'] = `Bearer ${cachedApiKeys[0].key}`;
      }
    }

    const res = await fetch(url, {
      method: 'POST',
      headers: headers,
      body: JSON.stringify(payload)
    });

    if (!res.ok) {
      const data = await res.json().catch(() => ({}));
      outputEl.style.color = '#ef4444';
      outputEl.textContent = `Error ${res.status}: ${JSON.stringify(data.error || data, null, 2)}`;
      metaEl.textContent = `${Date.now() - startTime}ms`;
      return;
    }

    if (streamEnabled && res.body) {
      outputEl.textContent = '';
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let accumulated = '';
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop(); // keep partial line

        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed.startsWith('data:')) {
            const rest = trimmed.slice(5).trim();
            if (rest === '[DONE]') continue;
            try {
              const parsed = JSON.parse(rest);
              if (isAnthropic) {
                if (parsed.type === 'content_block_delta' && parsed.delta?.text) {
                  accumulated += parsed.delta.text;
                  outputEl.textContent = accumulated;
                }
              } else {
                const delta = parsed.choices?.[0]?.delta?.content;
                if (delta) {
                  accumulated += delta;
                  outputEl.textContent = accumulated;
                }
              }
            } catch (err) {}
          }
        }
        metaEl.textContent = `Streaming... (${Date.now() - startTime}ms) | Model: ${model}`;
      }

      const elapsed = Date.now() - startTime;
      metaEl.textContent = `✓ Streamed in ${elapsed}ms | Model: ${model}`;
    } else {
      const data = await res.json();
      const elapsed = Date.now() - startTime;
      let reply = '';
      if (isAnthropic) {
        reply = data.content?.[0]?.text || JSON.stringify(data, null, 2);
      } else {
        reply = data.choices?.[0]?.message?.content || JSON.stringify(data, null, 2);
      }
      outputEl.textContent = reply;
      metaEl.textContent = `✓ Completed in ${elapsed}ms | Model: ${model}`;
    }

    refreshAll();
  } catch (err) {
    outputEl.style.color = '#ef4444';
    outputEl.textContent = `Network / Request Error: ${err.message}`;
  } finally {
    sendBtn.disabled = false;
    sendBtn.innerHTML = `
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="22" y1="2" x2="11" y2="13"/>
        <polygon points="22 2 15 22 11 13 2 9 22 2"/>
      </svg>
      <span>Send</span>
    `;
  }
}
