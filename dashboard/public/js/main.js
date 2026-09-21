/* SiteBox Dashboard — Client JS */

const API = '';
let sites = [];
let activeFilter = 'all';

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
async function loadSites() {
  const res = await fetch(`${API}/api/sites`);
  sites = await res.json();
  render();
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
  const filtersEl = document.getElementById('filters');
  filtersEl.innerHTML = `<button class="filter ${activeFilter === 'all' ? 'active' : ''}" data-cat="all">All</button>` +
    cats.map((c) => `<button class="filter ${activeFilter === c ? 'active' : ''}" data-cat="${c}">${c}</button>`).join('');
  filtersEl.querySelectorAll('.filter').forEach((btn) => {
    btn.addEventListener('click', () => { activeFilter = btn.dataset.cat; render(); });
  });

  if (filtered.length === 0) {
    grid.innerHTML = '';
    empty.style.display = 'block';
    return;
  }
  empty.style.display = 'none';

  grid.innerHTML = filtered.map((s) => {
    const iconColor = s.iconColor || '';
    const iconStyle = iconColor ? `style="--icon-color: ${esc(iconColor)}"` : '';
    const openUrl = (s.url || '').replace(/localhost|127\.0\.0\.1/, window.location.hostname);
    return `
    <div class="site-card" data-id="${s.id}">
      <div class="site-card__header">
        <div class="site-card__icon" ${iconStyle}>${getIcon(s.icon)}</div>
        <div class="site-card__info">
          <div class="site-card__name">${esc(s.name)}</div>
          <div class="site-card__desc" title="${esc(s.description || '')}">${esc(s.description || '—')}</div>
        </div>
      </div>
      <div class="site-card__meta">
        <div class="site-card__status">
          <span class="status-dot ${s._running ? 'status-dot--running' : 'status-dot--stopped'}"></span>
          <span>${s._running ? 'Running' : 'Stopped'}</span>
        </div>
        <code>:${s.port}</code>
        ${s.category ? `<span>${esc(s.category)}</span>` : ''}
      </div>
      <div class="site-card__actions">
        ${s._running
          ? `<button class="btn btn--danger btn--sm" onclick="stopSite('${s.id}')">
               <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="4" height="10" x="8" y="6" rx="1"/></svg>
               Stop
             </button>`
          : `<button class="btn btn--success btn--sm" onclick="startSite('${s.id}')">
               <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
               Start
             </button>`
        }
        <a href="${openUrl}" target="_blank" rel="noopener" class="btn btn--outline btn--sm">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
          Open
        </a>
        <button class="btn btn--ghost btn--sm" onclick="editSite('${s.id}')" title="Edit">
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/></svg>
        </button>
        <button class="btn btn--ghost btn--sm" onclick="deleteSite('${s.id}')" title="Delete">
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
        </button>
      </div>
    </div>
  `;
  }).join('');
}

function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

/* ── Actions ── */
async function startSite(id) {
  await fetch(`${API}/api/sites/${id}/start`, { method: 'POST' });
  setTimeout(loadSites, 500);
}

async function stopSite(id) {
  await fetch(`${API}/api/sites/${id}/stop`, { method: 'POST' });
  setTimeout(loadSites, 300);
}

async function deleteSite(id) {
  if (!confirm(`Delete "${id}"?`)) return;
  await fetch(`${API}/api/sites/${id}`, { method: 'DELETE' });
  loadSites();
}

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
  document.getElementById('modal').style.display = 'flex';
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
document.getElementById('btn-refresh').addEventListener('click', loadSites);

/* ── Auto-refresh status every 10s ── */
setInterval(loadSites, 10000);

/* ── Init ── */
loadSites();
