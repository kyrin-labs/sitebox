'use strict';
/**
 * site-url — the one place that decides which URL to show for a site.
 *
 * Two facts, and the URL must agree with both:
 *   - a site registered with host "127.0.0.1" answers on the host only, so its URL is
 *     localhost and never the browser's hostname: that link is dead on a phone;
 *   - a site registered with "0.0.0.0" (or an old entry with no host field, which also
 *     binds 0.0.0.0) is reachable on the LAN, so the host the person already used to
 *     open the dashboard is the right one to print.
 *
 * A URL that cannot open is worse than no URL, so this never guesses a hostname and
 * never writes the browser's view back into sites.json. Loadable by a browser (sets
 * window.SiteUrl) and by the node test suite (module.exports) without a build step.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.SiteUrl = api;
})(typeof self !== 'undefined' ? self : this, function () {
  function displayUrl(site, pageHost) {
    const port = Number(site && site.port);
    if (!Number.isInteger(port) || port <= 0) return (site && site.url) || '';
    const host = site._localOnly ? 'localhost' : (pageHost || 'localhost');
    return 'http://' + host + ':' + port;
  }

  function urlHint(site) {
    return site && site._localOnly
      ? 'Bound to 127.0.0.1 — this URL opens only on the machine running SiteBox. Copy it there, not from another device.'
      : 'Reachable on the LAN — copy it to any device.';
  }

  return { displayUrl, urlHint };
});
