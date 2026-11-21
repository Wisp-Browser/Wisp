// Wisp website: direct download links from the latest release, scroll reveals, screenshot zoom.
(() => {
  const REPO = 'jakedev2/Wisp';
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];

  // Download buttons start out pointing at the latest release page and switch to the files
  // themselves once GitHub tells us their names (they carry the version number).
  const kinds = { setup: /-setup\.exe$/i, portable: /-windows-x64\.exe$/i, ios: /-ios\.ipa$/i };
  const fmt = n => n > 1e6 ? (n / 1e6).toFixed(1) + ' MB' : Math.round(n / 1e3) + ' KB';
  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: 'application/vnd.github+json' } })
    .then(r => (r.ok ? r.json() : Promise.reject(r.status)))
    .then(rel => {
      $$('[data-version]').forEach(e => (e.textContent = rel.tag_name.replace(/^v/, '')));
      $$('[data-dl]').forEach(a => {
        const asset = (rel.assets || []).find(x => kinds[a.dataset.dl] && kinds[a.dataset.dl].test(x.name));
        if (!asset) return;
        a.href = asset.browser_download_url;
        a.setAttribute('download', '');
        const sz = a.querySelector('[data-size]');
        if (sz) sz.textContent = fmt(asset.size);
      });
    })
    .catch(() => {}); // offline or rate-limited: the buttons still open the release page

  // Point the main button at the right download for this device.
  const ios = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  if (ios) $$('[data-main]').forEach(a => {
    a.dataset.dl = 'ios';
    a.href = '#iphone';
    const l = a.querySelector('[data-label]');
    if (l) l.textContent = 'Get Wisp for iPhone';
  });

  // Reveal sections as they scroll in.
  const io = 'IntersectionObserver' in window && new IntersectionObserver(es => es.forEach(e => {
    if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
  }), { rootMargin: '0px 0px -8% 0px' });
  $$('.rv, .hero .shot').forEach(el => (io ? io.observe(el) : el.classList.add('in')));

  // Click a screenshot to see it large.
  const lb = document.createElement('div');
  lb.className = 'lb';
  lb.innerHTML = '<img alt="">';
  document.body.appendChild(lb);
  const close = () => lb.classList.remove('on');
  lb.onclick = close;
  addEventListener('keydown', e => e.key === 'Escape' && close());
  $$('.gallery figure').forEach(f => (f.onclick = () => {
    const img = $('img', f);
    $('img', lb).src = img.src;
    $('img', lb).alt = img.alt;
    lb.classList.add('on');
  }));

  const y = $('#year');
  if (y) y.textContent = new Date().getFullYear();
})();
