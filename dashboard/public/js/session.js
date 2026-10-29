/* session.js — stopgap UI login, loaded BEFORE main.js.
 *
 * The dashboard API now needs a credential. A cookie cannot be set from JavaScript, so
 * this wraps fetch: on the first 401 it asks for a token once, trades it for an
 * sbx_session cookie via POST /api/session, and retries the original request. Nothing
 * in main.js has to change, and <img src="/api/sites/:id/icon"> works because the
 * browser sends the cookie by itself.
 *
 * Deliberately minimal: the real key management pane (generate / revoke / lastUsed)
 * replaces the prompt later; this file exists so enabling auth does not white-screen
 * the dashboard on the day it ships.
 */
(function () {
  var KEY = 'sitebox-token';
  var asking = false;

  function login(token) {
    return fetch('/api/session', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token: token }),
    });
  }

  function ask() {
    if (asking) return Promise.resolve(false);
    asking = true;
    var token = window.prompt('SiteBox token (sbx_…). Paste it once; it is kept in this browser.');
    asking = false;
    if (!token) return Promise.resolve(false);
    token = token.trim();
    return login(token).then(function (res) {
      if (!res.ok) {
        window.alert('That token was refused. Generate one on the host: node dashboard/token.js add ui --scope write --print');
        return false;
      }
      try { localStorage.setItem(KEY, token); } catch (e) {}
      return true;
    });
  }

  var raw = window.fetch.bind(window);
  window.fetch = function (input, init) {
    var url = typeof input === 'string' ? input : (input && input.url) || '';
    var isApi = url.indexOf('/api/') === 0 || url.indexOf(location.origin + '/api/') === 0;
    return raw(input, init).then(function (res) {
      if (!isApi || res.status !== 401) return res;
      return ask().then(function (ok) {
        if (!ok) return res;
        return raw(input, init);
      });
    });
  };

  // A stored token means a brand-new browser session can log in without a prompt.
  try {
    var saved = localStorage.getItem(KEY);
    if (saved) login(saved).catch(function () {});
  } catch (e) {}
})();
