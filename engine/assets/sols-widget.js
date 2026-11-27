/* Sols — the launcher injected into every page SiteBox serves.
 *
 * It is deliberately self-gating: it asks the dashboard for /api/studio/status
 * with the browser's cookie. No cookie (or no configured model) means no button,
 * so a visitor to a site cannot reach the editor. The panel itself is an iframe
 * on the dashboard origin, which keeps auth and state in one place.
 */
(function () {
  'use strict';

  var cfg = window.__SOLS;
  if (!cfg || !cfg.port || !cfg.site) return;

  var DASH = location.protocol + '//' + location.hostname + ':' + cfg.port;
  var MARK =
    '<svg viewBox="0 0 32 32" width="20" height="20" aria-hidden="true">' +
    '<defs>' +
    '<linearGradient id="solsBody" x1="9" y1="4" x2="23" y2="29" gradientUnits="userSpaceOnUse">' +
    '<stop stop-color="#b9f3ff"/><stop offset=".45" stop-color="#59c2ff"/><stop offset="1" stop-color="#3b5bff"/>' +
    '</linearGradient>' +
    '<radialGradient id="solsGlow" cx="0" cy="0" r="1" gradientTransform="translate(16 15) scale(14)">' +
    '<stop stop-color="#7fe6ff" stop-opacity=".45"/><stop offset="1" stop-color="#7fe6ff" stop-opacity="0"/>' +
    '</radialGradient>' +
    '</defs>' +
    '<circle cx="16" cy="16" r="14.5" fill="url(#solsGlow)"/>' +
    '<path fill="url(#solsBody)" fill-rule="evenodd" d="M16 3.4c5.6 0 9.6 4 9.6 9.4 0 3.5-.9 6.2-2.3 9-.7 1.4-.6 2.7.2 4l1 1.8c.5.9-.2 1.9-1.2 1.9-.6 0-1.2-.3-1.6-.8l-1.5-1.9c-.7-.9-2-1-2.8-.2l-.8.8c-.6.6-1.6.6-2.2 0l-.8-.8c-.8-.8-2.1-.7-2.8.2l-1.5 1.9c-.4.5-1 .8-1.6.8-1 0-1.7-1-1.2-1.9l1-1.8c.8-1.3.9-2.6.2-4-1.4-2.8-2.3-5.5-2.3-9 0-5.4 4-9.4 9.6-9.4Zm-3.6 7.4c-.9 0-1.6.8-1.6 1.7s.7 1.7 1.6 1.7 1.6-.8 1.6-1.7-.7-1.7-1.6-1.7Zm7.2 0c-.9 0-1.6.8-1.6 1.7s.7 1.7 1.6 1.7 1.6-.8 1.6-1.7-.7-1.7-1.6-1.7Z"/>' +
    '<path d="M11 7.8c1.4-1.2 3.1-1.9 4.9-2" stroke="#fff" stroke-opacity=".6" stroke-width="1.5" stroke-linecap="round" fill="none"/>' +
    '<path d="M26.4 4.9l.7 1.7 1.7.7-1.7.7-.7 1.7-.7-1.7-1.7-.7 1.7-.7z" fill="#a9ecff" opacity=".9"/>' +
    '<path d="M5.2 8.6l.5 1.2 1.2.5-1.2.5-.5 1.2-.5-1.2L3.5 10.3l1.2-.5z" fill="#a9ecff" opacity=".7"/>' +
    '</svg>';

  var host, root, frameWrap, open = false, prevMargin = null;
  var dockW = (function () {
    var v = parseInt(localStorage.getItem('sols.width') || '', 10);
    return v >= 320 && v <= 900 ? v : 470;
  })();

  // Share the viewport instead of covering it: the page narrows by the drawer
  // width, so the operator can read the site and talk to Sols at once. Wide
  // mode is a modal and gives the space back.
  function dock(on) {
    if (!document.body) return;
    var width = window.innerWidth <= 560 ? 0 : dockW;
    if (on && width) {
      if (prevMargin === null) prevMargin = document.body.style.marginRight || '';
      document.body.style.transition = 'margin-right .18s ease';
      document.body.style.marginRight = width + 'px';
    } else {
      document.body.style.marginRight = prevMargin || '';
      document.body.style.transition = '';
    }
  }

  function mount() {
    if (host) return;
    host = document.createElement('div');
    host.setAttribute('data-sols', '');
    host.style.cssText = 'position:fixed;z-index:2147483000;';
    root = host.attachShadow({ mode: 'open' });
    root.innerHTML =
      '<style>' +
      ':host{all:initial}' +
      '.btn{position:fixed;right:18px;bottom:18px;display:flex;align-items:center;gap:8px;' +
      'padding:9px 14px 9px 11px;border-radius:999px;cursor:pointer;' +
      'font:600 13px/1 system-ui,-apple-system,"Segoe UI",sans-serif;letter-spacing:.01em;' +
      'color:#eaf7ff;background:linear-gradient(180deg,#101a2c,#0a1220);' +
      'border:1px solid rgba(89,194,255,.55);box-shadow:0 8px 28px rgba(20,90,180,.35),inset 0 1px 0 rgba(255,255,255,.08);' +
      'transition:transform .15s ease,box-shadow .15s ease}' +
      '.btn:hover{transform:translateY(-1px);box-shadow:0 12px 32px rgba(20,90,180,.45)}' +
      '.wrap{position:fixed;top:0;right:0;bottom:0;width:min(var(--w,470px),100%);display:flex;' +
      'justify-content:flex-end;background:transparent;pointer-events:none;animation:solsIn .18s ease}' +
      '.panel{width:100%;height:100%;border:0;border-left:1px solid rgba(0,0,0,.14);' +
      'background:#fff;box-shadow:none;pointer-events:auto}' +
      '.grip{position:absolute;left:-3px;top:0;bottom:0;width:8px;cursor:col-resize;pointer-events:auto;z-index:2}' +
      '.grip:hover,.wrap.dragging .grip{background:rgba(9,105,218,.22)}' +
      '.grip::after{content:"";position:absolute;left:3px;top:50%;margin-top:-18px;width:3px;height:36px;' +
      'border-radius:2px;background:rgba(0,0,0,.16)}' +
      '.grip:hover::after,.wrap.dragging .grip::after{background:rgba(9,105,218,.75)}' +
      '.wrap.dragging .panel{pointer-events:none}' +
      '.wrap.wide .grip{display:none}' +
      '.wrap.wide{inset:0;width:auto;justify-content:center;align-items:center;padding:4vh 3vw;' +
      'background:rgba(6,12,22,.5);backdrop-filter:blur(2px);pointer-events:auto}' +
      '.wrap.wide .panel{width:min(1040px,100%);height:100%;border-left:0;border-radius:14px;' +
      'box-shadow:0 24px 80px rgba(0,0,0,.45);pointer-events:auto}' +
      '@keyframes solsIn{from{opacity:0}to{opacity:1}}' +
      '@media(max-width:560px){.panel{width:100%}.btn span{display:none}}' +
      '</style>' +
      '<button class="btn" type="button" aria-label="Open Sols">' + MARK + '<span>Sols</span></button>';
    root.querySelector('.btn').addEventListener('click', toggle);
    document.documentElement.appendChild(host);
    window.addEventListener('message', onMessage);
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && open) toggle();
    });
  }

  function toggle() {
    if (open) return close();
    frameWrap = document.createElement('div');
    frameWrap.className = 'wrap';
    var f = document.createElement('iframe');
    f.className = 'panel';
    f.title = 'Sols';
    f.src = DASH + '/?sols=' + encodeURIComponent(cfg.site) +
      '&embed=1&route=' + encodeURIComponent(location.pathname + location.search) +
      '&v=' + (cfg.v || 1);
    f.setAttribute('allow', 'clipboard-write');
    var grip = document.createElement('div');
    grip.className = 'grip';
    grip.title = 'ลากเพื่อปรับความกว้าง';
    frameWrap.style.setProperty('--w', dockW + 'px');
    frameWrap.appendChild(grip);
    frameWrap.appendChild(f);
    root.appendChild(frameWrap);
    bindDrag(grip);
    dock(true);
    var btn = root.querySelector('.btn');
    if (btn) btn.style.display = 'none';
    open = true;
  }

  function bindDrag(grip) {
    grip.addEventListener('pointerdown', function (e) {
      e.preventDefault();
      grip.setPointerCapture && grip.setPointerCapture(e.pointerId);
      var startX = e.clientX;
      var startW = dockW;
      frameWrap.classList.add('dragging');
      document.body.style.transition = 'none';
      function move(ev) {
        var w = Math.round(startW - (ev.clientX - startX));
        var max = Math.min(760, Math.round(window.innerWidth * 0.72));
        dockW = Math.max(320, Math.min(max, w));
        frameWrap.style.setProperty('--w', dockW + 'px');
        if (document.body) document.body.style.marginRight = dockW + 'px';
      }
      function up() {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        frameWrap && frameWrap.classList.remove('dragging');
        if (document.body) document.body.style.transition = 'margin-right .18s ease';
        try { localStorage.setItem('sols.width', String(dockW)); } catch (err) {}
      }
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    });
  }

  function close() {
    if (frameWrap && frameWrap.parentNode) frameWrap.parentNode.removeChild(frameWrap);
    frameWrap = null;
    dock(false);
    var btn = root && root.querySelector('.btn');
    if (btn) btn.style.display = '';
    open = false;
  }

  function onMessage(e) {
    if (e.origin !== DASH || !e.data) return;
    if (e.data.type === 'sols-close') close();
    // The panel cannot grow inside a 470px iframe; the host owns the size.
    if (e.data.type === 'sols-expand' && frameWrap) {
      var wide = !!e.data.wide;
      frameWrap.classList.toggle('wide', wide);
      dock(!wide);
    }
  }

  // Gate: the dashboard answers 200 only when this browser already holds the
  // Sols session cookie. A site visitor without it never sees the button.
  fetch(DASH + '/api/studio/status', { credentials: 'include' })
    .then(function (r) {
      if (!r.ok) return null;
      return r.json();
    })
    .then(function (s) {
      if (!s || !s.settings || !s.settings.configured) return;
      if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', mount);
      } else {
        mount();
      }
    })
    .catch(function () {});
})();
