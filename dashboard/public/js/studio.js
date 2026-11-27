/* SiteBox Studio — the editing agent's chat panel.
 *
 * The server enforces the rules (no writes from the model, snapshot before
 * apply, rollback on a red check). This file only draws what happened and asks
 * the operator for the one decision the server will not make for them: Apply.
 */

(function () {
  'use strict';

  const state = {
    site: '',
    thread: '',
    sites: [],
    settings: {},
    browser: null,
    busy: false,
    // Turns run on the server per site. The panel may only show one at a
    // time, but a turn on site A must not lock the composer for site B.
    running: new Set(),
    creating: false,
    sending: false,
    attachments: [],
    es: null,
    watch: null,
    prepTimer: null,
    streamEl: null,
    patchEls: new Map(),
  };

  const $ = (sel, root) => (root || document).querySelector(sel);

  const ICONS = {
    'maximize-2': '<polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/>',
    'minimize-2': '<polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" y1="10" x2="21" y2="3"/><line x1="3" y1="21" x2="10" y2="14"/>',
    plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
    'rotate-ccw': '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
    settings: '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
    x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    paperclip: '<path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>',
  };

  function icon(name, size) {
    const body = ICONS[name] || '';
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${size || 16}" height="${size || 16}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
  }

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function esc(s) {
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  /* Minimal, safe markdown: escape first, then build blocks we control.
     Enough for what an assistant actually emits: headings, lists, quotes,
     fenced code, inline code, bold/italic and http(s) links. */
  function md(text) {
    let s = esc(text);
    const blocks = [];
    s = s.replace(/```[^\n]*\n?([\s\S]*?)```/g, (_, code) => {
      blocks.push(code);
      return `\n\u0000${blocks.length - 1}\u0000\n`;
    });
    s = s.replace(/`([^`\n]+)`/g, '<code>$1</code>');
    s = s.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/(^|[\s(])\*([^*\n]+)\*(?=[\s).,!?]|$)/g, '$1<em>$2</em>');
    s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
      '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');

    const out = [];
    let list = null;
    const closeList = () => { if (list) { out.push(`</${list}>`); list = null; } };
    for (const raw of s.split('\n')) {
      const line = raw.replace(/\s+$/, '');
      const code = line.match(/^\u0000(\d+)\u0000$/);
      if (code) { closeList(); out.push(`<pre class="st-code">${blocks[+code[1]]}</pre>`); continue; }
      if (!line.trim()) { closeList(); continue; }
      let m;
      if ((m = line.match(/^(#{1,4})\s+(.*)$/))) {
        closeList();
        out.push(`<div class="st-h st-h${m[1].length}">${m[2]}</div>`);
        continue;
      }
      if ((m = line.match(/^\s*[-*+]\s+(.*)$/))) {
        if (list !== 'ul') { closeList(); out.push('<ul>'); list = 'ul'; }
        out.push(`<li>${m[1]}</li>`);
        continue;
      }
      if ((m = line.match(/^\s*\d+[.)]\s+(.*)$/))) {
        if (list !== 'ol') { closeList(); out.push('<ol>'); list = 'ol'; }
        out.push(`<li>${m[1]}</li>`);
        continue;
      }
      if ((m = line.match(/^&gt;\s?(.*)$/))) {
        closeList();
        out.push(`<blockquote>${m[1]}</blockquote>`);
        continue;
      }
      if (/^-{3,}$/.test(line.trim())) { closeList(); out.push('<hr>'); continue; }
      closeList();
      out.push(`<p>${line}</p>`);
    }
    closeList();
    return out.join('');
  }

  /* Sols mark: a blue spirit. Gradient ids are per-instance so two copies on
     one page cannot share/clobber each other. */
  let markSeq = 0;
  function solsMark(size) {
    const id = 'sols' + (++markSeq);
    return `<svg viewBox="0 0 32 32" width="${size}" height="${size}" aria-hidden="true">
      <defs>
        <linearGradient id="${id}b" x1="9" y1="4" x2="23" y2="29" gradientUnits="userSpaceOnUse">
          <stop stop-color="#b9f3ff"/><stop offset=".45" stop-color="#59c2ff"/><stop offset="1" stop-color="#3b5bff"/>
        </linearGradient>
        <radialGradient id="${id}g" cx="0" cy="0" r="1" gradientTransform="translate(16 15) scale(14)">
          <stop stop-color="#7fe6ff" stop-opacity=".45"/><stop offset="1" stop-color="#7fe6ff" stop-opacity="0"/>
        </radialGradient>
      </defs>
      <circle cx="16" cy="16" r="14.5" fill="url(#${id}g)"/>
      <path fill="url(#${id}b)" fill-rule="evenodd" d="M16 3.4c5.6 0 9.6 4 9.6 9.4 0 3.5-.9 6.2-2.3 9-.7 1.4-.6 2.7.2 4l1 1.8c.5.9-.2 1.9-1.2 1.9-.6 0-1.2-.3-1.6-.8l-1.5-1.9c-.7-.9-2-1-2.8-.2l-.8.8c-.6.6-1.6.6-2.2 0l-.8-.8c-.8-.8-2.1-.7-2.8.2l-1.5 1.9c-.4.5-1 .8-1.6.8-1 0-1.7-1-1.2-1.9l1-1.8c.8-1.3.9-2.6.2-4-1.4-2.8-2.3-5.5-2.3-9 0-5.4 4-9.4 9.6-9.4Zm-3.6 7.4c-.9 0-1.6.8-1.6 1.7s.7 1.7 1.6 1.7 1.6-.8 1.6-1.7-.7-1.7-1.6-1.7Zm7.2 0c-.9 0-1.6.8-1.6 1.7s.7 1.7 1.6 1.7 1.6-.8 1.6-1.7-.7-1.7-1.6-1.7Z"/>
      <path d="M11 7.8c1.4-1.2 3.1-1.9 4.9-2" stroke="#fff" stroke-opacity=".6" stroke-width="1.5" stroke-linecap="round" fill="none"/>
      <path d="M26.4 4.9l.7 1.7 1.7.7-1.7.7-.7 1.7-.7-1.7-1.7-.7 1.7-.7z" fill="#a9ecff" opacity=".9"/>
      <path d="M5.2 8.6l.5 1.2 1.2.5-1.2.5-.5 1.2-.5-1.2L3.5 10.3l1.2-.5z" fill="#a9ecff" opacity=".7"/>
    </svg>`;
  }
  window.solsMark = solsMark;

  async function api(path, opts) {
    const res = await fetch(path, Object.assign({ headers: { 'content-type': 'application/json' } }, opts || {}));
    const text = await res.text();
    let data;
    try { data = text ? JSON.parse(text) : {}; } catch (_) { data = { raw: text }; }
    if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
    return data;
  }

  /* ── shell ─────────────────────────────────────────────────────────────── */

  function mount() {
    if ($('#studio-launcher')) return;

    const embed = document.body.classList.contains('sols-embed');
    const btn = el('button', embed ? 'studio-launcher' : 'st-launch');
    btn.id = 'studio-launcher';
    btn.title = 'Sols';
    btn.innerHTML = `${solsMark(18)}<span>Sols</span>`;
    btn.addEventListener('click', () => toggle(true));
    if (embed) {
      document.body.appendChild(btn);
    } else {
      // On the dashboard Sols belongs in the header, where the primary action
      // lives; a floating pill duplicates it and covers the grid.
      const actions = document.querySelector('.header__actions');
      if (actions) actions.appendChild(btn); else document.body.appendChild(btn);
      document.body.classList.add('sols-ready');
    }

    const panel = el('aside', 'studio-panel');
    panel.id = 'studio-panel';
    panel.innerHTML = `
      <header class="st-head">
        <div class="st-head__title">
          <strong class="st-brand">${solsMark(22)}<span>Sols</span></strong>
          <span class="st-status" id="st-status">…</span>
        </div>
        <div class="st-head__actions">
          <button class="st-icon" id="st-expand" title="Expand">${icon('maximize-2')}</button>
          <button class="st-icon" id="st-new" title="New conversation">${icon('plus')}</button>
          <button class="st-icon" id="st-undo" title="Restore the last snapshot">${icon('rotate-ccw')}</button>
          <button class="st-icon" id="st-settings" title="Settings">${icon('settings')}</button>
          <button class="st-icon" id="st-close" title="Close">${icon('x')}</button>
        </div>
      </header>
      <div class="st-bar">
        <select id="st-site" class="st-select"></select>
        <select id="st-thread" class="st-select st-select--thread"></select>
        <select id="st-mode" class="st-select st-select--mode" title="Ask: propose patches for approval. All-in: apply immediately.">
          <option value="ask">Ask</option>
          <option value="all">All-in</option>
        </select>
      </div>
      <div class="st-plan" id="st-plan" hidden></div>
      <div class="st-log" id="st-log"></div>
      <form class="st-compose" id="st-form">
        <div class="st-attach" id="st-attach-row" hidden></div>
        <textarea id="st-input" rows="2" placeholder="บอกว่าจะแก้ตรงไหน… แนบภาพ หรือวางภาพจาก clipboard ได้"></textarea>
        <div class="st-compose__row">
          <button type="button" class="st-icon st-attach-btn" id="st-pick" title="แนบภาพ">${icon('paperclip')}</button>
          <input type="file" id="st-file" accept="image/png,image/jpeg,image/webp,image/gif" multiple hidden>
          <span class="st-hint" id="st-hint"></span>
          <button type="button" class="st-btn st-btn--ghost" id="st-abort" hidden>Stop</button>
          <button type="submit" class="st-btn st-btn--primary" id="st-send">Send</button>
        </div>
      </form>
      <div class="st-settings" id="st-settings-panel" hidden>
        <label>Base URL<input id="st-base" placeholder="https://api.example.com/v1"></label>
        <label>Model<input id="st-model" placeholder="model-name"></label>
        <label>API key<input id="st-key" type="password" placeholder="unchanged"></label>
        <div class="st-settings__row">
          <span class="st-hint" id="st-settings-hint"></span>
          <button class="st-btn st-btn--primary" id="st-save">Save</button>
        </div>
      </div>`;
    document.body.appendChild(panel);

    $('#st-close').addEventListener('click', () => {
      if (document.body.classList.contains('sols-embed')) {
        // The panel is an iframe on the dashboard origin; the page that hosts
        // it owns the overlay and closes it.
        window.parent.postMessage({ type: 'sols-close' }, '*');
      } else {
        toggle(false);
      }
    });
    $('#st-site').addEventListener('change', (e) => {
      if (!e.target.value) { newSiteThread(); return; }
      state.site = e.target.value;
      localStorage.setItem('studio.site', state.site);
      loadThreads(true);
    });
    $('#st-thread').addEventListener('change', (e) => selectThread(e.target.value));
    $('#st-new').addEventListener('click', () => newThread());
    $('#st-pick').addEventListener('click', () => $('#st-file').click());
    $('#st-file').addEventListener('change', (e) => {
      [...(e.target.files || [])].forEach(addAttachment);
      e.target.value = '';
    });
    $('#st-input').addEventListener('paste', (e) => {
      const items = [...((e.clipboardData && e.clipboardData.items) || [])];
      const imgs = items.filter((it) => it.kind === 'file' && it.type.startsWith('image/'));
      if (!imgs.length) return;
      e.preventDefault();
      imgs.forEach((it) => { const f = it.getAsFile(); if (f) addAttachment(f); });
    });
    $('#st-expand').addEventListener('click', () => {
      const p = $('#studio-panel');
      const wide = p.classList.toggle('is-wide');
      $('#st-expand').setAttribute('aria-pressed', wide ? 'true' : 'false');
      $('#st-expand').innerHTML = icon(wide ? 'minimize-2' : 'maximize-2');
      $('#st-expand').title = wide ? 'Collapse' : 'Expand';
      // In embed mode the iframe is only as wide as the drawer. The host page
      // has to grow it; the class above then lays out the two columns.
      if (document.body.classList.contains('sols-embed')) {
        window.parent.postMessage({ type: 'sols-expand', wide }, '*');
      }
    });
    $('#st-mode').addEventListener('change', async (e) => {
      if (!state.thread) return;
      try {
        const t = await api(`/api/studio/threads/${encodeURIComponent(state.thread)}/mode`, {
          method: 'POST', body: JSON.stringify({ mode: e.target.value }),
        });
        state.mode = t.mode || 'ask';
        toast(state.mode === 'all' ? 'All-in: patches จะถูกใช้ทันที' : 'Ask: ให้กด Apply เอง');
      } catch (err) { toast(err.message); }
    });
    $('#st-settings').addEventListener('click', () => {
      const p = $('#st-settings-panel');
      p.hidden = !p.hidden;
      if (!p.hidden) fillSettings();
    });
    $('#st-save').addEventListener('click', saveSettings);
    $('#st-undo').addEventListener('click', undo);
    $('#st-abort').addEventListener('click', abort);
    $('#st-form').addEventListener('submit', (e) => { e.preventDefault(); send(); });
    $('#st-input').addEventListener('keydown', (e) => {
      // One path only: Enter asks the form to submit, so the submit handler is
      // the single place that calls send(). Calling send() here as well is how
      // one keystroke becomes two turns.
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        if (e.target.form) e.target.form.requestSubmit();
      }
    });

    refreshStatus();
  }

  function toggle(open) {
    const p = $('#studio-panel');
    p.classList.toggle('is-open', open);
    // On the dashboard the panel is a dock, so the grid makes room for it.
    // Inside an embed iframe it already owns the whole viewport.
    if (!document.body.classList.contains('sols-embed')) {
      document.body.classList.toggle('sols-open', open);
    }
    if (open) {
      refreshStatus();
      $('#st-input').focus();
    }
  }

  async function refreshStatus() {
    try {
      const s = await api('/api/studio/status');
      state.settings = s.settings || {};
      state.sites = s.sites || [];
      state.search = s.search || { configured: false };
      state.browser = s.browser;
      renderStatus();
      fillSites();
      // Opening the panel without a card click must still show the last
      // conversation; otherwise the log looks empty until the site is changed.
      if (!state.thread && state.site && !document.body.classList.contains('sols-embed')) {
        loadThreads(true);
      }
    } catch (e) {
      $('#st-status').textContent = e.message;
      $('#st-status').classList.add('is-bad');
    }
  }

  function renderStatus() {
    const s = state.settings;
    const badge = $('#st-status');
    badge.classList.toggle('is-bad', !s.configured);
    badge.textContent = s.configured
      ? `${s.model}${state.browser ? ' · shots on' : ' · checks only'}`
      : 'not configured';
    $('#st-hint').textContent = s.configured
      ? (state.search && state.search.configured ? '' : 'เว็บค้นหาไม่พร้อม (SEARXNG_URL)')
      : 'ตั้งค่า base URL + model ก่อน';
    badge.title = state.search && state.search.configured
      ? `search: ${state.search.url}`
      : 'search: not configured';
    $('#st-send').disabled = !s.configured;
  }

  function fillSites() {
    const sel = $('#st-site');
    const opts = state.sites.map((s) => `<option value="${esc(s.id)}">${esc(s.name || s.id)}</option>`);
    // A conversation that is creating a site has no site yet; keep the empty
    // value selectable so the bar does not lie about what it is attached to.
    opts.unshift('<option value="">New site…</option>');
    sel.innerHTML = opts.join('');
    // Embed opens for one specific site; the dashboard's last choice must not
    // win over the site the operator is actually looking at.
    const saved = document.body.classList.contains('sols-embed')
      ? null
      : localStorage.getItem('studio.site');
    if (!state.site && saved && state.sites.some((s) => s.id === saved)) state.site = saved;
    else if (!state.site && state.sites[0]) state.site = state.sites[0].id;
    sel.value = state.site || '';
  }

  function fillSettings() {
    const env = state.settings.from_env || {};
    $('#st-base').value = state.settings.base_url || '';
    $('#st-model').value = state.settings.model || '';
    $('#st-base').readOnly = !!env.base_url;
    $('#st-model').readOnly = !!env.model;
    $('#st-base').title = env.base_url ? 'ตั้งค่าใน .env (AI_BASE_URL)' : '';
    $('#st-model').title = env.model ? 'ตั้งค่าใน .env (AI_MODEL)' : '';
    $('#st-key').value = '';
    $('#st-key').disabled = !!env.has_key;
    $('#st-key').placeholder = env.has_key
      ? 'from .env (AI_API_KEY)'
      : (state.settings.has_key ? 'unchanged (leave blank)' : 'sk-…');
    $('#st-settings-hint').textContent = (env.base_url || env.model || env.has_key)
      ? 'บางค่ามาจาก .env — แก้ที่ไฟล์นั้น'
      : (state.settings.endpoint || '');
  }

  async function saveSettings() {
    const env = state.settings.from_env || {};
    const body = {};
    // Fields owned by .env stay there; sending them back would copy deployment
    // config into settings.json and make the two disagree.
    if (!env.base_url) body.base_url = $('#st-base').value.trim();
    if (!env.model) body.model = $('#st-model').value.trim();
    const key = $('#st-key').value.trim();
    if (key && !env.has_key) body.api_key = key;
    try {
      state.settings = await api('/api/studio/config', { method: 'PUT', body: JSON.stringify(body) });
      $('#st-settings-panel').hidden = true;
      renderStatus();
    } catch (e) {
      $('#st-settings-hint').textContent = e.message;
    }
  }

  /* ── threads ───────────────────────────────────────────────────────────── */

  async function loadThreads(fresh) {
    state.patchEls.clear();
    $('#st-log').innerHTML = '';
    renderPlan([]);
    if (!state.site) return;
    const asked = state.site;
    let threads = [];
    try { threads = await api(`/api/studio/threads?site=${encodeURIComponent(asked)}`); } catch (_) { threads = []; }
    // A slower answer for the site we just left must not replace the new one.
    if (state.site !== asked) return;
    if (!Array.isArray(threads) || threads.length === 0) {
      // One attempt, not a loop: newThread -> loadThreads -> newThread used to
      // recurse when the list stayed empty (an API hiccup), creating a row of
      // blank conversations at the same second.
      if (!state.creating) await newThread();
      return;
    }
    const sel = $('#st-thread');
    sel.innerHTML = threads.map((t) => `<option value="${esc(t.id)}">${t.running ? '● ' : ''}${esc(t.title || '(new conversation)')}</option>`).join('');
    // A turn that is still running wins over "newest": it is the thing the
    // operator is waiting for.
    selectThread((threads.find((t) => t.running) || threads[0]).id);
  }

  async function newSiteThread() {
    try {
      const t = await api('/api/studio/threads', { method: 'POST', body: JSON.stringify({ site_id: '' }) });
      state.site = '';
      state.thread = t.id;
      state.patchEls.clear();
      $('#st-site').value = '';
      $('#st-thread').innerHTML = `<option value="${esc(t.id)}">New site</option>`;
      $('#st-log').innerHTML = '';
      renderPlan([]);
      addMsg('system', 'จะสร้างเว็บอะไร บอกได้เลย — ประเภทของเว็บ ใครอ่าน และมีเนื้อหาจริงอะไรบ้าง');
      $('#st-input').placeholder = 'เช่น เว็บ portfolio สำหรับช่างภาพ พร้อมผลงาน 6 ชิ้น…';
      subscribe(t.id);
      $('#st-input').focus();
    } catch (e) {
      toast(e.message);
    }
  }

  async function newThread() {
    // In the embed panel the site comes from the iframe URL, but a reload or a
    // half-open panel can leave state empty while the select still knows it.
    const site = state.site || ($('#st-site') && $('#st-site').value) || '';
    if (!site) { toast('เลือกเว็บก่อนสร้างบทสนทนาใหม่'); return; }
    if (state.creating) return;
    state.creating = true;
    try {
      const t = await api('/api/studio/threads', { method: 'POST', body: JSON.stringify({ site_id: site }) });
      state.site = site;
      state.thread = t.id;
      state.patchEls.clear();
      $('#st-log').innerHTML = '';
      renderPlan([]);
      const sel = $('#st-thread');
      if (sel) {
        sel.insertAdjacentHTML('afterbegin', `<option value="${esc(t.id)}">(new conversation)</option>`);
        sel.value = t.id;
      }
      addMsg('system', 'บทสนทนาใหม่ — บอกงานที่อยากให้ทำกับเว็บนี้ได้เลย');
      subscribe(t.id);
      state.busy = false;
      setBusy(false);
      $('#st-input').focus();
    } catch (e) {
      toast(e.message || 'สร้างบทสนทนาไม่สำเร็จ');
    } finally {
      state.creating = false;
    }
  }

  async function selectThread(id) {
    if (!id) return;
    // A send owns the panel for its duration: a slower loadThreads answer that
    // lands mid-send used to select another conversation, so the answer
    // streamed into a thread the operator was not looking at.
    if (state.sending && id !== state.thread) return;
    state.thread = id;
    $('#st-thread').value = id;
    $('#st-log').innerHTML = '';
    state.patchEls.clear();
    try {
      const d = await api(`/api/studio/threads/${encodeURIComponent(id)}/messages`);
      if (state.site && d.thread && d.thread.site_id && d.thread.site_id !== state.site) return;
      state.mode = (d.thread && d.thread.mode) || 'ask';
      $('#st-mode').value = state.mode;
      (d.messages || []).forEach(renderStored);
      if (!(d.messages || []).length) {
        $('#st-log').innerHTML = `<div class="st-empty">${solsMark(30)}` +
          `<p>คุยกับ Sols เรื่องเว็บนี้ได้เลย</p>` +
          `<span>บอกว่าอยากแก้อะไร หรือให้สร้างหน้าใหม่ — Sols จะเสนอ patch ให้ดูก่อน</span></div>`;
      }
      renderPlan(d.thread && d.thread.plan);
      // A turn keeps running across a reload. Without this the Stop button
      // vanished while the harness was still writing patches.
      if (d.running) state.running.add(id);
      state.busy = state.running.has(id) || !!d.running;
      setBusy(state.busy);
      if (d.running) watch(id);
    } catch (_) { /* a fresh thread has no messages file yet */ }
    subscribe(id);
    $('#st-input').focus();
  }

  // While a turn runs, reload the thread's stored messages on a timer. The SSE
  // stream only carries what happens after it connects, so a page that was
  // refreshed mid-turn would otherwise sit on a stale log and no Stop button.
  function watch(threadId) {
    if (state.watch) clearInterval(state.watch);
    state.watch = setInterval(async () => {
      if (state.thread !== threadId) { clearInterval(state.watch); state.watch = null; return; }
      try {
        const d = await api(`/api/studio/threads/${encodeURIComponent(threadId)}/messages`);
        const atBottom = (() => { const l = $('#st-log'); return l.scrollHeight - l.scrollTop - l.clientHeight < 40; })();
        if (state.thread === threadId) {
          $('#st-log').innerHTML = '';
          state.patchEls.clear();
          (d.messages || []).forEach(renderStored);
          renderPlan(d.thread && d.thread.plan);
          if (atBottom) scroll();
        }
        if (!d.running) {
          clearInterval(state.watch); state.watch = null;
          state.running.delete(threadId);
          if (threadId === state.thread) { state.busy = false; setBusy(false); }
        }
      } catch (_) { /* keep watching; a transient error is not a stop */ }
    }, 2500);
  }

  function subscribe(threadId) {
    if (state.watch) { clearInterval(state.watch); state.watch = null; }
    if (state.es) { state.es.close(); state.es = null; }
    state.es = new EventSource(`/api/studio/events?thread=${encodeURIComponent(threadId)}`);
    state.es.onmessage = (m) => {
      let ev;
      try { ev = JSON.parse(m.data); } catch (_) { return; }
      handleEvent(ev);
    };
    state.es.onerror = () => { /* EventSource reconnects by itself */ };
  }

  /* ── rendering ─────────────────────────────────────────────────────────── */

  function renderStored(m) {
    if (m.role === 'user') {
      const imgs = (m.images || []).map((rel) => '/api/studio/uploads/' + String(rel).replace(/^uploads\//, ''));
      addMsg('user', m.content, imgs);
    }
    else if (m.role === 'assistant' && m.content) addMsg('assistant', m.content);
    else if (m.role === 'tool') addToolLine(m.name || 'tool', m.content, true);
  }

  function addMsg(role, text, images) {
    const n = el('div', `st-msg st-msg--${role}`);
    n.innerHTML = md(text);
    (images || []).forEach((src) => {
      const img = el('img', 'st-msg__img');
      img.src = src;
      img.alt = 'attachment';
      n.appendChild(img);
    });
    $('#st-log').appendChild(n);
    scroll();
    return n;
  }

  function addToolLine(name, detail, quiet) {
    const d = el('details', 'st-tool');
    const s = el('summary');
    s.innerHTML = `<span class="st-tool__dot"></span>${esc(String(name).replace(/^sitebox_/, ''))}`;
    d.appendChild(s);
    const pre = el('pre', 'st-tool__body', typeof detail === 'string' ? detail : JSON.stringify(detail, null, 2));
    d.appendChild(pre);
    if (quiet) d.classList.add('is-old');
    $('#st-log').appendChild(d);
    scroll();
    return d;
  }

  function renderPlan(steps) {
    const box = $('#st-plan');
    if (!box) return;
    if (!steps || !steps.length) {
      box.hidden = true;
      box.innerHTML = `<div class="st-plan__empty">${solsMark(20)}` +
        `<p>ยังไม่มีแผน</p><span>แผนจะขึ้นที่นี่เมื่อ Sols เริ่มงาน</span></div>`;
      return;
    }
    const icon = { done: '●', doing: '◐', pending: '○' };
    box.hidden = false;
    box.innerHTML = steps.map((s) => {
      const status = s.status || 'pending';
      return `<div class="st-plan__row is-${esc(status)}"><span class="st-plan__dot">${icon[status] || '○'}</span><span>${esc(s.text)}</span></div>`;
    }).join('');
  }

  function handleEvent(ev) {
    switch (ev.type) {
      case 'plan':
        renderPlan(ev.steps);
        break;
      case 'site_created': {
        // The thread was just bound to the site it created: move the bar with
        // it without re-subscribing (the turn is still streaming).
        state.site = ev.site;
        localStorage.setItem('studio.site', state.site);
        refreshStatus().then(() => {
          $('#st-site').value = ev.site;
          $('#st-thread').innerHTML = `<option value="${esc(state.thread)}">${esc(ev.name || ev.site)}</option>`;
          $('#st-input').placeholder = 'บอกว่าจะแก้ตรงไหน…';
        });
        toast(`สร้าง ${ev.name || ev.site} ที่พอร์ต ${ev.port} แล้ว`);
        break;
      }
      case 'text_delta': {
        if (!state.streamEl) {
          state.streamEl = addMsg('assistant', '');
          state.streamText = '';
        }
        // Render the accumulated text, not the chunk: markdown spans deltas,
        // and a per-chunk render breaks a list or a bold run into fragments.
        state.streamText += ev.text;
        state.streamEl.innerHTML = md(state.streamText);
        scroll();
        break;
      }
      case 'tool_preparing': {
        document.querySelectorAll('.st-tool.is-prep').forEach((n) => n.remove());
        if (state.prepTimer) clearInterval(state.prepTimer);
        const d = el('details', 'st-tool is-prep');
        const name = String(ev.name || '').replace(/^sitebox_/, '');
        d.innerHTML = `<summary><span class="st-tool__dot"></span>${esc(name)} ` +
          `<span class="st-tool__state">กำลังเขียน…</span></summary>`;
        $('#st-log').appendChild(d);
        scroll();
        // The model can spend a minute emitting a page; a counter proves the
        // panel is alive even when the bytes arrive in one late chunk.
        const t0 = Date.now();
        state.prepTimer = setInterval(() => {
          const st = document.querySelector('.st-tool.is-prep .st-tool__state');
          if (!st) return;
          if (!st.dataset.kb) st.textContent = `กำลังเขียน… ${Math.round((Date.now() - t0) / 1000)}s`;
        }, 1000);
        break;
      }
      case 'tool_progress': {
        const d = document.querySelector('.st-tool.is-prep .st-tool__state');
        if (d) {
          d.dataset.kb = '1';
          d.textContent = `กำลังเขียน… ${(Number(ev.chars || 0) / 1024).toFixed(1)} KB`;
        }
        break;
      }
      case 'tool_start': {
        state.streamEl = null;
        state.streamText = '';
        document.querySelectorAll('.st-tool.is-prep').forEach((n) => n.remove());
        if (state.prepTimer) { clearInterval(state.prepTimer); state.prepTimer = null; }
        const label = String(ev.name || '').replace(/^sitebox_/, '');
        const d = addToolLine(ev.name, JSON.stringify(ev.args || {}));
        d.dataset.running = '1';
        d.classList.add('is-running');
        d.querySelector('summary').insertAdjacentHTML('beforeend', `<span class="st-tool__state">running ${esc(label)}…</span>`);
        d.open = false;
        break;
      }
      case 'tool_end': {
        const ds = [...document.querySelectorAll('.st-tool.is-running')];
        const d = ds[ds.length - 1];
        if (d) {
          d.classList.remove('is-running');
          d.dataset.running = '';
          const st = d.querySelector('.st-tool__state');
          if (st) st.textContent = ev.ok === false ? 'failed' : 'done';
          const pre = d.querySelector('.st-tool__body');
          if (pre) pre.textContent = JSON.stringify(ev.result, null, 2);
          const url = ev.result && ev.result.url;
          if (url && String(url).startsWith('/api/studio/shots/')) {
            const img = el('img', 'st-shot');
            img.src = url;
            img.alt = 'screenshot';
            d.appendChild(img);
          }
          if (ev.ok === false) d.classList.add('is-bad');
        }
        break;
      }
      case 'patch':
        state.streamEl = null;
        state.streamText = '';
        renderPatch(ev.patch);
        break;
      case 'patch_applied': {
        // Creation turns apply their own patches. Without this the card keeps
        // offering Apply for a patch that is already in the file.
        const card = state.patchEls.get(ev.patch_id);
        if (card) {
          const btn = card.querySelector('.st-apply');
          btn.disabled = true;
          btn.hidden = true;
          card.querySelector('.st-patch__result').textContent = ev.ok
            ? 'applied while creating (Undo restores)'
            : 'rolled back: the checks failed';
          card.classList.toggle('is-red', !ev.ok);
        }
        break;
      }
      case 'done':
        state.busy = false;
        state.running.delete(state.thread);
        document.querySelectorAll('.st-tool.is-prep').forEach((n) => n.remove());
        if (state.watch) { clearInterval(state.watch); state.watch = null; }
        state.streamEl = null;
        state.streamText = '';
        setBusy(false);
        break;
      case 'error':
        state.busy = false;
        state.running.delete(state.thread);
        document.querySelectorAll('.st-tool.is-prep').forEach((n) => n.remove());
        if (state.watch) { clearInterval(state.watch); state.watch = null; }
        state.streamEl = null;
        state.streamText = '';
        setBusy(false);
        addMsg('error', ev.message || 'error');
        break;
      case 'aborted':
        state.busy = false;
        state.running.delete(state.thread);
        document.querySelectorAll('.st-tool.is-prep').forEach((n) => n.remove());
        if (state.watch) { clearInterval(state.watch); state.watch = null; }
        state.streamEl = null;
        state.streamText = '';
        setBusy(false);
        break;
      case 'bee': {
        // A bee is a worker, not a chat message: one line that names it and
        // then updates in place, so a swarm of four stays readable.
        const id = 'bee-' + (ev.id || 0);
        let row = document.getElementById(id);
        if (!row) {
          row = el('div', 'st-msg st-msg--system');
          row.id = id;
          $('#st-log').appendChild(row);
        }
        const mark = ev.status === 'start' ? '◐' : ev.status === 'done' ? '●' : '✕';
        row.textContent = `${mark} Bee ${ev.id || ''} · ${ev.title || ''}`;
        row.style.opacity = ev.status === 'start' ? '.75' : '1';
        $('#st-log').scrollTop = $('#st-log').scrollHeight;
        break;
      }
      case 'turn_start':
        state.busy = true;
        setBusy(true);
        break;
    }
  }

  function renderPatch(p) {
    const card = el('div', 'st-patch');
    card.dataset.patch = p.id;
    const v = p.verify || {};
    const intro = v.introduced || [];
    const pre = v.pre_existing || [];
    const chip = intro.length === 0
      ? '<span class="st-chip is-ok">no new failures</span>'
      : `<span class="st-chip is-bad">${intro.length} new failure${intro.length === 1 ? '' : 's'}: ${esc(intro.join(', '))}</span>`;
    const preChip = pre.length
      ? `<span class="st-chip" title="these checks were already failing before this patch">${pre.length} pre-existing: ${esc(pre.join(', '))}</span>`
      : '';
    card.innerHTML = `
      <div class="st-patch__head">
        <code>${esc(p.path)}</code>
        <span class="st-patch__stat"><b>+${p.added || 0}</b> <i>−${p.removed || 0}</i></span>
      </div>
      <div class="st-patch__reason">${esc(p.reason || '')}</div>
      <div class="st-patch__chips">${chip}${preChip}<span class="st-chip">${esc(v.mode || '')}</span></div>
      <pre class="st-diff">${esc(p.diff || '')}</pre>
      <div class="st-patch__actions">
        <button class="st-btn st-btn--primary st-apply" ${state.busy ? 'disabled' : ''}>Apply</button>
        <button class="st-btn st-btn--ghost st-toggle-diff">Hide diff</button>
        <span class="st-hint st-patch__result"></span>
      </div>`;
    $('#st-log').appendChild(card);
    state.patchEls.set(p.id, card);
    card.querySelector('.st-toggle-diff').addEventListener('click', (e) => {
      const pre = card.querySelector('.st-diff');
      pre.hidden = !pre.hidden;
      e.target.textContent = pre.hidden ? 'Show diff' : 'Hide diff';
    });
    card.querySelector('.st-apply').addEventListener('click', () => applyPatch(p.id, card));
    scroll();
  }

  async function applyPatch(id, card) {
    const btn = card.querySelector('.st-apply');
    const out = card.querySelector('.st-patch__result');
    btn.disabled = true;
    out.textContent = 'applying…';
    try {
      const r = await api(`/api/studio/patches/${encodeURIComponent(id)}/apply`, { method: 'POST' });
      const pre = (r.pre_existing || []).length;
      if (r.ok) {
        card.classList.add('is-applied');
        out.textContent = pre ? `applied · no new failures (${pre} pre-existing)` : 'applied · checks pass';
        btn.textContent = 'Applied';
      } else {
        card.classList.add('is-rolledback');
        out.textContent = r.restored ? 'introduced new failures · restored' : 'checks failed';
        btn.textContent = 'Not applied';
        const fails = ((r.static && r.static.findings) || [])
          .filter((f) => f.status === 'fail' && (r.introduced || []).includes(f.check));
        fails.slice(0, 4).forEach((f) => {
          const line = el('div', 'st-fail', `${f.check}: ${f.title}${f.evidence ? ' — ' + f.evidence : ''}`);
          card.appendChild(line);
        });
      }
    } catch (e) {
      out.textContent = e.message;
      btn.disabled = false;
    }
  }

  async function undo() {
    if (!state.site) return;
    if (!confirm('คืนค่า site กลับไป snapshot ล่าสุด?')) return;
    try {
      const r = await api(`/api/studio/sites/${encodeURIComponent(state.site)}/undo`, { method: 'POST' });
      const n = (r.changed || []).length;
      const text = !r.ok
        ? 'ย้อนกลับไม่สำเร็จ'
        : r.checks_ok
          ? `ย้อนกลับแล้ว (${n} ไฟล์) · checks pass`
          : `ย้อนกลับแล้ว (${n} ไฟล์) · checks เดิมยังแดง`;
      addMsg(r.ok ? 'system' : 'error', text);
      toast(text);
    } catch (e) {
      toast(e.message);
    }
  }

  async function abort() {
    if (!state.thread) return;
    try { await api('/api/studio/abort', { method: 'POST', body: JSON.stringify({ thread_id: state.thread }) }); }
    catch (e) { toast(e.message); }
  }

  /* ── sending ───────────────────────────────────────────────────────────── */

  function markRunning(id, on) {
    if (!id) return;
    if (on) state.running.add(id); else state.running.delete(id);
    if (id === state.thread) { state.busy = on; setBusy(on); }
  }

  function setBusy(on) {
    $('#st-send').disabled = on || !state.settings.configured;
    $('#st-abort').hidden = !on;
    $('#st-input').disabled = on;
    // A patch can be offered before the turn's last message lands; applying it
    // then would answer 409 because the site lock is still held by the turn.
    document.querySelectorAll('.st-apply').forEach((b) => {
      b.disabled = on;
    });
  }

  function addAttachment(file) {
    if (!file || !file.type.startsWith('image/')) return;
    if (state.attachments.length >= 4) { toast('แนบได้สูงสุด 4 ภาพ'); return; }
    if (file.size > 8 * 1024 * 1024) { toast('ภาพใหญ่เกิน 8 MB'); return; }
    const r = new FileReader();
    r.onload = () => {
      state.attachments.push({ data: String(r.result), name: file.name || 'paste.png' });
      paintAttachments();
    };
    r.readAsDataURL(file);
  }

  function paintAttachments() {
    const row = $('#st-attach-row');
    if (!row) return;
    row.hidden = state.attachments.length === 0;
    row.innerHTML = '';
    state.attachments.forEach((a, i) => {
      const chip = el('div', 'st-attach-chip');
      const img = el('img');
      img.src = a.data;
      img.alt = a.name;
      const x = el('button', 'st-attach-chip__x');
      x.innerHTML = icon('x', 11);
      x.type = 'button';
      x.title = 'เอาภาพออก';
      x.addEventListener('click', () => {
        state.attachments.splice(i, 1);
        paintAttachments();
      });
      chip.appendChild(img);
      chip.appendChild(x);
      row.appendChild(chip);
    });
  }

  async function send() {
    const input = $('#st-input');
    let message = input.value.trim();
    if (!message && state.attachments.length) message = 'ดูภาพที่แนบมาแล้วทำตามบริบทของภาพ';
    if (!message || state.running.has(state.thread)) return;
    // Lock before the first await: two entry points (Enter, submit) must not
    // both get past the guard while a new thread is being created.
    state.busy = true;
    state.sending = true;
    setBusy(true);
    input.value = '';
    let tid = state.thread;
    try {
      if (!tid) {
        await newThread();
        tid = state.thread;
        if (!tid) throw new Error('could not create a conversation');
      }
      const atts = state.attachments.map((a) => a.data);
      addMsg('user', message, atts);
      markRunning(tid, true);
      await api('/api/studio/chat', {
        method: 'POST',
        body: JSON.stringify({
          thread_id: tid,
          message,
          images: atts,
          // A nonce makes a double submit a no-op on the server instead of a
          // second turn. Cheap insurance against a browser that fires both
          // Enter and submit for one keystroke.
          nonce: (window.crypto && crypto.randomUUID)
            ? crypto.randomUUID()
            : String(Date.now()) + Math.random(),
          page: state.page || undefined,
        }),
      });
      state.attachments = [];
      paintAttachments();
    } catch (e) {
      const busy = /already running/i.test(e.message || '');
      addMsg('error', busy
        ? 'เว็บนี้มีเทิร์นกำลังทำงานอยู่ — สลับไปอีกเว็บหนึ่งแล้วสั่งพร้อมกันได้ หรือกด Stop ก่อน'
        : e.message);
      state.busy = false;
      if (tid) state.running.delete(tid);
      setBusy(false);
    } finally {
      state.sending = false;
      // If the panel drifted to another conversation while this one was being
      // posted, come back to the thread that actually has the turn.
      if (tid && state.thread !== tid) selectThread(tid);
    }
  }

  function toast(msg) {
    const t = el('div', 'st-toast', msg);
    document.body.appendChild(t);
    setTimeout(() => t.remove(), 3200);
  }

  function scroll() {
    const log = $('#st-log');
    log.scrollTop = log.scrollHeight;
  }

  window.studioOpen = function (siteId) {
    mount();
    if (siteId) {
      state.site = siteId;
      $('#st-site').value = siteId;
      localStorage.setItem('studio.site', siteId);
      loadThreads(true);
    }
    toggle(true);
  };

  /* Opened inside a site's page (?sols=<site>&embed=1): show only the panel,
     bound to that site, and give the close button back to the parent. */
  async function startEmbedded() {
    const p = new URLSearchParams(location.search);
    if (p.get('embed') !== '1') return;
    document.body.classList.add('sols-embed');
    const site = p.get('sols');
    state.page = p.get('route') || '';
    await refreshStatus();
    if (site) {
      state.site = site;
      const sel = $('#st-site');
      if (sel) sel.value = site;
      await loadThreads(true);
    }
    toggle(true);
  }

  function boot() {
    mount();
    startEmbedded();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
