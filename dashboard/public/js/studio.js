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
    es: null,
    streamEl: null,
    patchEls: new Map(),
  };

  const $ = (sel, root) => (root || document).querySelector(sel);

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

    const btn = el('button', 'studio-launcher');
    btn.id = 'studio-launcher';
    btn.title = 'Sols';
    btn.innerHTML = `${solsMark(20)}<span>Sols</span>`;
    btn.addEventListener('click', () => toggle(true));
    document.body.appendChild(btn);

    const panel = el('aside', 'studio-panel');
    panel.id = 'studio-panel';
    panel.innerHTML = `
      <header class="st-head">
        <div class="st-head__title">
          <strong class="st-brand">${solsMark(22)}<span>Sols</span></strong>
          <span class="st-status" id="st-status">…</span>
        </div>
        <div class="st-head__actions">
          <button class="st-icon" id="st-new" title="New conversation">＋</button>
          <button class="st-icon" id="st-settings" title="Settings">⚙</button>
          <button class="st-icon" id="st-close" title="Close">✕</button>
        </div>
      </header>
      <div class="st-bar">
        <select id="st-site" class="st-select"></select>
        <select id="st-thread" class="st-select st-select--thread"></select>
        <button class="st-btn st-btn--ghost" id="st-undo" title="Restore the last snapshot">Undo</button>
      </div>
      <div class="st-log" id="st-log"></div>
      <form class="st-compose" id="st-form">
        <textarea id="st-input" rows="2" placeholder="บอกว่าจะแก้ตรงไหน…"></textarea>
        <div class="st-compose__row">
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
      state.site = e.target.value;
      localStorage.setItem('studio.site', state.site);
      loadThreads(true);
    });
    $('#st-thread').addEventListener('change', (e) => selectThread(e.target.value));
    $('#st-new').addEventListener('click', () => newThread());
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
      state.browser = s.browser;
      renderStatus();
      fillSites();
      // Opening the panel without a card click must still show the last
      // conversation; otherwise the log looks empty until the site is changed.
      if (!state.thread && state.site) loadThreads(true);
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
    $('#st-hint').textContent = s.configured ? '' : 'ตั้งค่า base URL + model ก่อน';
    $('#st-send').disabled = !s.configured;
  }

  function fillSites() {
    const sel = $('#st-site');
    if (sel.options.length === state.sites.length && sel.options.length > 0) return;
    sel.innerHTML = state.sites.map((s) => `<option value="${esc(s.id)}">${esc(s.name || s.id)}</option>`).join('');
    const saved = localStorage.getItem('studio.site');
    if (saved && state.sites.some((s) => s.id === saved)) state.site = saved;
    else if (state.sites[0]) state.site = state.sites[0].id;
    if (state.site) sel.value = state.site;
  }

  function fillSettings() {
    $('#st-base').value = state.settings.base_url || '';
    $('#st-model').value = state.settings.model || '';
    $('#st-key').value = '';
    $('#st-key').placeholder = state.settings.has_key ? 'unchanged (leave blank)' : 'sk-…';
    $('#st-settings-hint').textContent = state.settings.endpoint || '';
  }

  async function saveSettings() {
    const body = {
      base_url: $('#st-base').value.trim(),
      model: $('#st-model').value.trim(),
    };
    const key = $('#st-key').value.trim();
    if (key) body.api_key = key;
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
    if (!state.site) return;
    let threads = [];
    try { threads = await api(`/api/studio/threads?site=${encodeURIComponent(state.site)}`); } catch (_) { threads = []; }
    if (!Array.isArray(threads) || threads.length === 0) {
      await newThread();
      return;
    }
    const sel = $('#st-thread');
    sel.innerHTML = threads.map((t) => `<option value="${esc(t.id)}">${esc(t.title || '(new conversation)')}</option>`).join('');
    selectThread(threads[0].id);
  }

  async function newThread() {
    if (!state.site) return;
    try {
      const t = await api('/api/studio/threads', { method: 'POST', body: JSON.stringify({ site_id: state.site }) });
      await loadThreads(false);
      selectThread(t.id);
    } catch (e) {
      toast(e.message);
    }
  }

  async function selectThread(id) {
    if (!id) return;
    state.thread = id;
    $('#st-thread').value = id;
    $('#st-log').innerHTML = '';
    state.patchEls.clear();
    try {
      const d = await api(`/api/studio/threads/${encodeURIComponent(id)}/messages`);
      (d.messages || []).forEach(renderStored);
    } catch (_) { /* a fresh thread has no messages file yet */ }
    subscribe(id);
    $('#st-input').focus();
  }

  function subscribe(threadId) {
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
    if (m.role === 'user') addMsg('user', m.content);
    else if (m.role === 'assistant' && m.content) addMsg('assistant', m.content);
    else if (m.role === 'tool') addToolLine(m.name || 'tool', m.content, true);
  }

  function addMsg(role, text) {
    const n = el('div', `st-msg st-msg--${role}`);
    n.innerHTML = md(text);
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

  function handleEvent(ev) {
    switch (ev.type) {
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
      case 'tool_start': {
        state.streamEl = null;
        state.streamText = '';
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
      case 'done':
        state.busy = false;
        state.streamEl = null;
        state.streamText = '';
        setBusy(false);
        break;
      case 'error':
        state.busy = false;
        state.streamEl = null;
        state.streamText = '';
        setBusy(false);
        addMsg('error', ev.message || 'error');
        break;
      case 'aborted':
        state.busy = false;
        state.streamEl = null;
        state.streamText = '';
        setBusy(false);
        break;
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

  async function send() {
    const input = $('#st-input');
    const message = input.value.trim();
    if (!message || state.busy) return;
    // Lock before the first await: two entry points (Enter, submit) must not
    // both get past the guard while a new thread is being created.
    state.busy = true;
    setBusy(true);
    input.value = '';
    try {
      if (!state.thread) {
        await newThread();
        if (!state.thread) throw new Error('could not create a conversation');
      }
      addMsg('user', message);
      await api('/api/studio/chat', {
        method: 'POST',
        body: JSON.stringify({
          thread_id: state.thread,
          message,
          // A nonce makes a double submit a no-op on the server instead of a
          // second turn. Cheap insurance against a browser that fires both
          // Enter and submit for one keystroke.
          nonce: (window.crypto && crypto.randomUUID)
            ? crypto.randomUUID()
            : String(Date.now()) + Math.random(),
          page: state.page || undefined,
        }),
      });
    } catch (e) {
      addMsg('error', e.message);
      state.busy = false;
      setBusy(false);
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
