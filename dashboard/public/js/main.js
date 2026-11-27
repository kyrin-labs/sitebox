/* SiteBox Dashboard — Client JS */

const API = '';
let sites = [];
let activeFilter = 'all';
let lastSig = '';
let lastCats = '';

/* ── What this browser may do ──
 * The UI is the operator's surface: the page mints a session for itself, so the
 * destructive controls are shown and the server stays the authority (a refused write
 * is a 403, not a hidden button). Start optimistic and let /api/me narrow it only when
 * the answer actually arrives — a failed status fetch must not quietly remove Add Site.
 */
let ME = { canManage: true, canAdminTokens: true, loopback: false };

async function loadMe() {
  try {
    const res = await fetch(`${API}/api/me`, { cache: 'no-store' });
    if (res.ok) ME = await res.json();
  } catch (e) {
    // Keep the optimistic defaults: the dashboard is still usable, and every write is
    // checked by the server anyway.
  }
  const add = document.getElementById('btn-add');
  const keys = document.getElementById('btn-keys');
  if (add) add.style.display = ME.canManage === false ? 'none' : '';
  // Keys are dashboard business, not machine business: any device that can open the
  // dashboard can manage them.
  if (keys) keys.style.display = ME.canAdminTokens === false ? 'none' : '';
}


/* ── Icon mapping (Lucide SVGs) ── */
const ICONS = {
  globe:    '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></svg>',
  book:     '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/></svg>',
  clock:    '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>',
  code:     '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>',
  camera:   '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>',
  gamepad:  '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" x2="10" y1="12" y2="12"/><line x1="8" x2="8" y1="10" y2="14"/><line x1="15" x2="15.01" y1="13" y2="13"/><line x1="18" x2="18.01" y1="11" y2="11"/><rect width="20" height="12" x="2" y="6" rx="2"/></svg>',
  music:    '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>',
  settings: '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>',
  notebook: '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/></svg>',
  'notebook-pen': '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>',
  database: '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5V19A9 3 0 0 0 21 19V5"/><path d="M3 12A9 3 0 0 0 21 12"/></svg>',
  star:     '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>',
};

function getIcon(name) {
  return ICONS[name] || ICONS.globe;
}

/* ── Theme toggle ── */
function initTheme() {
  const saved = localStorage.getItem('sitebox-theme');
  const theme = saved || 'light';
  document.documentElement.setAttribute('data-theme', theme);
  updateThemeIcon(theme);
}

function updateThemeIcon(theme) {
  const sun = document.querySelector('#btn-theme .icon-sun');
  const moon = document.querySelector('#btn-theme .icon-moon');
  if (sun && moon) {
    sun.style.display = theme === 'dark' ? 'block' : 'none';
    moon.style.display = theme === 'light' ? 'block' : 'none';
  }
}

function toggleTheme() {
  const current = document.documentElement.getAttribute('data-theme');
  const next = current === 'light' ? 'dark' : 'light';
  document.documentElement.setAttribute('data-theme', next);
  localStorage.setItem('sitebox-theme', next);
  updateThemeIcon(next);
}

document.getElementById('btn-theme').addEventListener('click', toggleTheme);
initTheme();

/* ── Fetch sites ── */
async function loadSites(force = false) {
  try {
    const res = await fetch(`${API}/api/sites`);
    const data = await res.json();
    if (!Array.isArray(data)) throw new Error(data?.error || 'Unexpected /api/sites response');
    const sig = JSON.stringify(data);
    if (!force && sig === lastSig) return;   // nothing changed → skip DOM work
    lastSig = sig;
    sites = data;
    render();
  } catch (e) {
    console.error('Failed to load sites', e);
  }
}

/* ── Filters (rebuilt only when the category set changes) ── */
document.getElementById('filters').addEventListener('click', (e) => {
  const btn = e.target.closest('.filter');
  if (!btn) return;
  activeFilter = btn.dataset.cat;
  render();
});

function renderFilters(cats) {
  const filtersEl = document.getElementById('filters');
  const sig = cats.join('|');
  if (sig !== lastCats) {
    lastCats = sig;
    filtersEl.innerHTML = `<button class="filter" data-cat="all">All</button>` +
      cats.map((c) => `<button class="filter" data-cat="${c}">${c}</button>`).join('');
  }
  filtersEl.querySelectorAll('.filter').forEach((b) => {
    b.classList.toggle('active', b.dataset.cat === activeFilter);
  });
}

/* ── Render ── */
function render() {
  const grid = document.getElementById('sites-grid');
  const empty = document.getElementById('empty-state');
  const q = document.getElementById('search').value.toLowerCase();

  const filtered = sites.filter((s) => {
    if (activeFilter !== 'all' && s.category !== activeFilter) return false;
    if (q && !s.name.toLowerCase().includes(q) && !s.description?.toLowerCase().includes(q)) return false;
    return true;
  });

  // Stats
  document.getElementById('stat-total').textContent = sites.length;
  document.getElementById('stat-running').textContent = sites.filter((s) => s._running).length;

  // Filters
  const cats = [...new Set(sites.map((s) => s.category).filter(Boolean))];
  renderFilters(cats);

  if (filtered.length === 0) {
    grid.innerHTML = '';
    empty.style.display = 'block';
    return;
  }
  empty.style.display = 'none';

  grid.innerHTML = filtered.map((s) => {
    const hasFavicon = !!s._favicon;
    const iconColor = hasFavicon ? '' : (s.iconColor || '');
    const iconStyle = iconColor ? `style="--icon-color: ${esc(iconColor)}"` : '';
    const iconHtml = hasFavicon
      ? `<img src="${esc(s._favicon)}" alt="" loading="lazy">`
      : getIcon(s.icon);
    const openUrl = SiteUrl.displayUrl(s, window.location.hostname);
    const urlHint = SiteUrl.urlHint(s);
    // Destructive controls exist only where the server will accept them.
    const dragHandle = ME.canManage
      ? `<div class="site-card__drag" title="Drag to reorder"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="5" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="9" cy="19" r="1"/><circle cx="15" cy="5" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="15" cy="19" r="1"/></svg></div>`
      : '';
    const manageButtons = ME.canManage
      ? `<button class="btn btn--ghost btn--sm" onclick="editSite('${s.id}')" title="Edit"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/></svg></button><button class="btn btn--ghost btn--sm" onclick="deleteSite('${s.id}')" title="Delete site and its files"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg></button>`
      : '';
    return `
    <div class="site-card" data-id="${s.id}" ${ME.canManage ? 'draggable="true"' : ''}>
      ${dragHandle}
        <div class="site-card__header">
        <div class="site-card__icon" ${iconStyle}>${iconHtml}</div>
        <div class="site-card__info">
          <div class="site-card__name">${esc(s.name)}</div>
          <div class="site-card__desc" title="${esc(s.description || '')}">${esc(s.description || '—')}</div>
        </div>
      </div>
      <div class="site-card__meta">
        <div class="site-card__status">
          <span class="status-dot ${s._stale ? 'status-dot--stopped' : s._running ? 'status-dot--running' : 'status-dot--stopped'}"></span>
          <span>${s._stale ? 'Missing files' : s._running ? 'Running' : 'Stopped'}</span>
        </div>
        ${s.category ? `<span>${esc(s.category)}</span>` : ''}
        ${s._stale ? `<span class="badge badge--warn" title="server.js not found — the folder was moved, renamed, or deleted">stale</span>` : ''}
        ${s._localOnly ? `<span class="badge badge--info" title="Bound to 127.0.0.1 — this link only opens on the machine running SiteBox">local</span>` : ''}
      </div>
      <div class="site-card__url">
        <code title="${esc(urlHint)}">${esc(openUrl)}</code>
        <button class="btn btn--ghost site-card__copy" data-copy="${esc(openUrl)}" title="Copy URL" aria-label="Copy URL for ${esc(s.name)}">
          <svg class="icon-copy" xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
          <svg class="icon-check" xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
        </button>
      </div>
      <div class="site-card__actions">
        ${s._running
          ? `<button class="btn btn--danger btn--sm" onclick="stopSite('${s.id}')">
               <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="4" height="10" x="8" y="6" rx="1"/></svg>
               Stop
             </button>`
          : `<button class="btn btn--success btn--sm" onclick="startSite('${s.id}')" ${s._stale ? 'disabled title="Site files are missing"' : ''}>
               <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
               Start
             </button>`
        }
        <a href="${esc(openUrl)}" target="_blank" rel="noopener" class="btn btn--outline btn--sm" title="${esc(urlHint)}" ${s._stale ? 'aria-disabled="true" style="opacity:.5;pointer-events:none"' : ''}>
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
          Open
        </a>
        <button class="btn btn--ghost btn--sm" onclick="studioOpen('${s.id}')" title="Studio — edit with AI">
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
        </button>
        <button class="btn btn--ghost btn--sm" onclick="showLogs('${s.id}')" title="Logs">
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/></svg>
        </button>
        ${manageButtons}
      </div>
    </div>
  `;
  }).join('');
}

function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

/* The URL is shown so it can be copied, not only opened. navigator.clipboard needs a
 * secure context; the dashboard is often plain http on a LAN, so keep a fallback that
 * works there instead of failing silently. */
async function copyText(text) {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch { /* fall through to the selection fallback */ }
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.setAttribute('readonly', '');
  ta.style.position = 'fixed';
  ta.style.top = '-1000px';
  ta.style.opacity = '0';
  document.body.appendChild(ta);
  ta.select();
  let ok = false;
  try { ok = document.execCommand('copy'); } catch { ok = false; }
  ta.remove();
  return ok;
}

document.getElementById('sites-grid').addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-copy]');
  if (!btn) return;
  e.preventDefault();
  const ok = await copyText(btn.getAttribute('data-copy'));
  btn.classList.toggle('is-copied', ok);
  btn.title = ok ? 'Copied' : 'Copy failed — select the URL instead';
  setTimeout(() => { btn.classList.remove('is-copied'); btn.title = 'Copy URL'; }, 1200);
});

/* ── Actions ── */
async function startSite(id) {
  const res = await fetch(`${API}/api/sites/${id}/start`, { method: 'POST' });
  const data = await res.json();
  if (!data.ok) {
    const lines = (data.logs || []).map((l) => l.text).join('\n');
    alert(`Could not start "${id}"\n\n${data.message || 'unknown error'}${data.hint ? `\n\n${data.hint}` : ''}${lines ? `\n\nRecent output:\n${lines}` : ''}`);
  }
  loadSites();
}

async function stopSite(id) {
  await fetch(`${API}/api/sites/${id}/stop`, { method: 'POST' });
  setTimeout(loadSites, 300);
}

async function deleteSite(id) {
  // There is no per-device permission any more, so the brake has to be here: typing the
  // id is the difference between a considered delete and a thumb on a phone.
  const typed = prompt(`Delete "${id}" permanently?\n\nThis stops the site, removes sites/${id}/ from disk and drops it from the dashboard. It cannot be undone.\n\nType the site id to confirm:`);
  if (typed === null) return;
  if (typed.trim() !== id) { alert(`Not deleted: "${typed.trim()}" does not match "${id}".`); return; }
  const res = await fetch(`${API}/api/sites/${id}`, { method: 'DELETE' });
  const data = await res.json().catch(() => ({}));
  if (!res.ok || data.ok === false) {
    alert(`Could not delete "${id}"\n\n${data.error || res.statusText}${data.hint ? `\n\n${data.hint}` : ''}`);
  }
  loadSites();
}

/* ── Logs modal ── */
let logsSiteId = null;

async function showLogs(id) {
  logsSiteId = id;
  document.getElementById('logs-title').textContent = `Logs — ${id}`;
  document.getElementById('logs-modal').style.display = 'flex';
  await refreshLogs();
}

async function refreshLogs() {
  if (!logsSiteId) return;
  const el = document.getElementById('logs-view');
  try {
    const res = await fetch(`${API}/api/sites/${logsSiteId}/logs?lines=300`);
    const data = await res.json();
    if (!data.lines || data.lines.length === 0) {
      el.textContent = 'No logs yet — start the site to capture its output.';
    } else {
      el.textContent = data.lines
        .map((l) => `${l.t.slice(11, 19)} [${l.stream}] ${l.text}`)
        .join('\n');
      el.scrollTop = el.scrollHeight;
    }
  } catch (e) {
    el.textContent = `Failed to load logs: ${e.message}`;
  }
}

function closeLogs() {
  document.getElementById('logs-modal').style.display = 'none';
  logsSiteId = null;
}

document.getElementById('logs-close').addEventListener('click', closeLogs);
document.getElementById('logs-refresh').addEventListener('click', refreshLogs);
document.getElementById('logs-clear').addEventListener('click', async () => {
  if (!logsSiteId || !confirm('Clear captured logs?')) return;
  await fetch(`${API}/api/sites/${logsSiteId}/logs`, { method: 'DELETE' });
  refreshLogs();
});
document.getElementById('logs-modal').addEventListener('click', (e) => { if (e.target === e.currentTarget) closeLogs(); });

function editSite(id) {
  const s = sites.find((x) => x.id === id);
  if (!s) return;
  document.getElementById('modal-title').textContent = 'Edit Site';
  document.getElementById('form-id').value = s.id;
  document.getElementById('form-name').value = s.name;
  document.getElementById('form-desc').value = s.description || '';
  document.getElementById('form-port').value = s.port;
  document.getElementById('form-icon').value = s.icon || '';
  document.getElementById('form-category').value = s.category || '';
  document.getElementById('form-path').value = s.path || '';
  const colorInput = document.getElementById('form-icon-color');
  const colorText = document.getElementById('form-icon-color-text');
  if (colorInput) colorInput.value = s.iconColor || '#58a6ff';
  if (colorText) colorText.value = s.iconColor || '#58a6ff';
  document.getElementById('form-id').disabled = true;
  // A detected favicon wins over the configured icon. Lock the fields so the
  // stored values cannot silently diverge from what the card renders; removing
  // the favicon file unlocks them again.
  setIconFieldsLocked(!!s._favicon, s._faviconSource);
  document.getElementById('modal').style.display = 'flex';
}

function setIconFieldsLocked(locked, source) {
  const icon = document.getElementById('form-icon');
  const color = document.getElementById('form-icon-color');
  const colorText = document.getElementById('form-icon-color-text');
  const note = document.getElementById('form-icon-note');
  for (const el of [icon, color, colorText]) if (el) el.disabled = locked;
  if (note) {
    note.style.display = locked ? 'block' : 'none';
    if (locked) note.textContent = `Icon auto-detected from ${source || "this site's favicon"} — remove the favicon to set one manually.`;
  }
}

/* ── Icon color input sync ── */
const formIconColor = document.getElementById('form-icon-color');
const formIconColorText = document.getElementById('form-icon-color-text');
if (formIconColor && formIconColorText) {
  formIconColor.addEventListener('input', () => { formIconColorText.value = formIconColor.value; });
  formIconColorText.addEventListener('input', () => { formIconColor.value = formIconColorText.value; });
}

/* ── Modal ── */
document.getElementById('btn-add').addEventListener('click', () => {
  document.getElementById('modal-title').textContent = 'Add Site';
  document.getElementById('site-form').reset();
  document.getElementById('form-id').disabled = false;
  setIconFieldsLocked(false);
  document.getElementById('port-check-result').style.display = 'none';
  document.getElementById('modal').style.display = 'flex';
});

document.getElementById('modal-close').addEventListener('click', closeModal);
document.getElementById('form-cancel').addEventListener('click', closeModal);
document.getElementById('modal').addEventListener('click', (e) => { if (e.target === e.currentTarget) closeModal(); });

function closeModal() { document.getElementById('modal').style.display = 'none'; }

/* ── Port check on blur ── */
document.getElementById('form-port').addEventListener('blur', async (e) => {
  const port = parseInt(e.target.value);
  if (!port) return;
  const res = await fetch(`${API}/api/ports/check?port=${port}`);
  const data = await res.json();
  const el = document.getElementById('port-check-result');
  el.style.display = 'block';
  if (data.used) {
    el.className = 'used';
    el.textContent = `Port ${port} is already in use`;
  } else {
    el.className = 'available';
    el.textContent = `Port ${port} is available`;
  }
});

/* ── Form submit ── */
document.getElementById('site-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const id = document.getElementById('form-id').value || document.getElementById('form-name').value.toLowerCase().replace(/\s+/g, '-');
  const port = parseInt(document.getElementById('form-port').value);
  const iconColor = document.getElementById('form-icon-color')
    ? document.getElementById('form-icon-color').value
    : '#58a6ff';
  const data = {
    id,
    name: document.getElementById('form-name').value,
    description: document.getElementById('form-desc').value,
    port,
    icon: document.getElementById('form-icon').value || 'globe',
    iconColor,
    category: document.getElementById('form-category').value || 'uncategorized',
    path: document.getElementById('form-path').value,
    url: `http://localhost:${port}`,
  };
  await fetch(`${API}/api/sites`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  closeModal();
  loadSites();
});

/* ── Search ── */
document.getElementById('search').addEventListener('input', render);

/* ── Refresh ── */
document.getElementById('btn-refresh').addEventListener('click', () => loadSites(true));

/* ── Auto-refresh status every 10s, paused while the tab is hidden ── */
const REFRESH_MS = 10000;
let refreshTimer = null;

function startPolling() {
  clearInterval(refreshTimer);
  refreshTimer = setInterval(() => loadSites(), REFRESH_MS);
}



/* ── Drag & Drop Reorder ── */
let draggedId = null;

document.addEventListener('dragstart', (e) => {
  const card = e.target.closest('.site-card');
  if (!card) return;
  draggedId = card.dataset.id;
  card.classList.add('dragging');
  e.dataTransfer.effectAllowed = 'move';
  e.dataTransfer.setData('text/plain', draggedId);
});

document.addEventListener('dragend', (e) => {
  const card = e.target.closest('.site-card');
  if (card) card.classList.remove('dragging');
  document.querySelectorAll('.site-card.drag-over').forEach(c => c.classList.remove('drag-over'));
  draggedId = null;
});

document.addEventListener('dragover', (e) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'move';
  const card = e.target.closest('.site-card');
  if (!card || card.dataset.id === draggedId) return;
  document.querySelectorAll('.site-card.drag-over').forEach(c => c.classList.remove('drag-over'));
  card.classList.add('drag-over');
});

document.addEventListener('dragleave', (e) => {
  const card = e.target.closest('.site-card');
  if (card) card.classList.remove('drag-over');
});

document.addEventListener('drop', async (e) => {
  e.preventDefault();
  const targetCard = e.target.closest('.site-card');
  if (!targetCard || !draggedId) return;
  const targetId = targetCard.dataset.id;
  if (draggedId === targetId) return;

  // Reorder sites array
  const fromIdx = sites.findIndex(s => s.id === draggedId);
  const toIdx = sites.findIndex(s => s.id === targetId);
  if (fromIdx === -1 || toIdx === -1) return;

  const [moved] = sites.splice(fromIdx, 1);
  sites.splice(toIdx, 0, moved);

  // Save order to server
  const order = sites.map(s => s.id);
  await fetch(`${API}/api/sites/reorder`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ order }),
  });

  render();
});
document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  } else {
    loadSites();
    startPolling();
  }
});

/* ── API keys: minted here, used by clients (MCP, scripts) ── */
function openKeys() {
  document.getElementById('keys-modal').style.display = 'flex';
  document.getElementById('key-new').style.display = 'none';
  loadKeys();
}
function closeKeys() { document.getElementById('keys-modal').style.display = 'none'; }

async function loadKeys() {
  const body = document.getElementById('keys-body');
  body.innerHTML = '<tr><td colspan="5">Loading…</td></tr>';
  try {
    const res = await fetch(`${API}/api/tokens`);
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || res.status);
    const rows = data.tokens || [];
    body.innerHTML = rows.length
      ? rows.map((t) => `
      <tr>
        <td><code>${esc(t.label)}</code></td>
        <td>${esc(t.scope)}</td>
        <td>${esc(String(t.created || '').slice(0, 10))}</td>
        <td>${t.lastUsed ? esc(String(t.lastUsed).slice(0, 16).replace('T', ' ')) : '—'}</td>
        <td><button class="btn btn--ghost btn--sm" data-revoke="${esc(t.label)}">Revoke</button></td>
      </tr>`).join('')
      : '<tr><td colspan="5">No keys yet — generate one for your MCP client.</td></tr>';
  } catch (e) {
    body.innerHTML = `<tr><td colspan="5">Cannot read keys: ${esc(String(e.message || e))}</td></tr>`;
  }
  loadAuditTail();
}

/* The audit log is the only record of who changed what, so it belongs next to the keys
 * that name those actors — otherwise the answer lives in a file you can only read over SSH. */
async function loadAuditTail() {
  const out = document.getElementById('audit-lines');
  if (!out) return;
  try {
    const res = await fetch(`${API}/api/audit?lines=5`);
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || res.status);
    const lines = data.lines || [];
    out.textContent = lines.length
      ? lines.map((l) => `${String(l.time || '').slice(0, 16).replace('T', ' ')}  ${l.status == null ? '---' : String(l.status).padEnd(3)}  ${l.actor}  ${l.method} ${l.path}`).join('\n')
      : 'No activity recorded yet.';
  } catch (e) {
    out.textContent = 'Cannot read the audit log: ' + String(e.message || e);
  }
}

async function createKey() {
  const label = document.getElementById('key-label').value.trim();
  const scope = document.getElementById('key-scope').value;
  const out = document.getElementById('key-new');
  out.style.display = 'block';
  if (!/^[a-z0-9][a-z0-9-]{1,31}$/.test(label)) {
    out.textContent = 'Label must be kebab-case, 2–32 characters (letters, digits, single hyphens).';
    return;
  }
  const res = await fetch(`${API}/api/tokens`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ label, scope }),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) { out.textContent = `Failed: ${data.error || res.status}`; return; }
  out.textContent = `${data.key}\n\nCopy it now — only its hash is stored, so it cannot be shown again.`;
  document.getElementById('key-label').value = '';
  loadKeys();
}

async function revokeKey(label) {
  if (!confirm(`Revoke "${label}"? Every client using that key stops working immediately. This does not log you out of the dashboard.`)) return;
  const res = await fetch(`${API}/api/tokens/${encodeURIComponent(label)}`, { method: 'DELETE' });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) { alert(`Failed: ${data.error || res.status}`); return; }
  loadKeys();
}

document.getElementById('btn-keys').addEventListener('click', openKeys);
document.getElementById('keys-close').addEventListener('click', closeKeys);
document.getElementById('keys-modal').addEventListener('click', (e) => { if (e.target === e.currentTarget) closeKeys(); });
document.getElementById('key-create').addEventListener('click', createKey);
document.getElementById('keys-body').addEventListener('click', (e) => {
  const b = e.target.closest('[data-revoke]');
  if (b) revokeKey(b.getAttribute('data-revoke'));
});

/* ── Init ── */
loadMe().then(() => loadSites());
if (!document.hidden) startPolling();
