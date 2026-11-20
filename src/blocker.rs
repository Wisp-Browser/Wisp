//! Built-in ad / tracker blocker: an ABP-syntax filter-list engine (network + cosmetic rules),
//! plus the WebView2 request hook that applies it.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
};
use url::Url;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Document,
    Script,
    Image,
    Style,
    Xhr,
    Media,
    Font,
    Other,
}

fn kind_bit(k: Kind) -> u16 {
    match k {
        Kind::Script => 1,
        Kind::Image => 2,
        Kind::Style => 4,
        Kind::Xhr => 8,
        Kind::Media => 16,
        Kind::Font => 32,
        Kind::Other => 64,
        Kind::Document => 128,
    }
}

#[derive(Clone, Debug, Default)]
struct Rule {
    pat: String,
    host_anchor: bool,
    start_anchor: bool,
    end_anchor: bool,
    third: Option<bool>,
    kinds: u16, // 0 = any
    inc: Vec<String>,
    exc: Vec<String>,
}

#[derive(Default)]
struct Index {
    by_host: HashMap<String, Vec<Rule>>,
    by_token: HashMap<String, Vec<Rule>>,
    generic: Vec<Rule>,
}

#[derive(Default)]
pub struct Engine {
    block: Index,
    allow: Index,
    generic_css: Vec<String>,
    site_css: HashMap<String, Vec<String>>,
    pub rule_count: usize,
}

fn is_sep(c: u8) -> bool {
    !(c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'%'))
}

/// ABP pattern match: `*` any run, `^` separator (or end). Matches a prefix of `t` unless `end`.
fn m(p: &[u8], t: &[u8], end: bool) -> bool {
    match p.first() {
        None => !end || t.is_empty(),
        Some(b'*') => (0..=t.len()).any(|i| m(&p[1..], &t[i..], end)),
        Some(b'^') => match t.first() {
            None => m(&p[1..], t, end),
            Some(c) if is_sep(*c) => m(&p[1..], &t[1..], end),
            _ => false,
        },
        Some(c) => t.first() == Some(c) && m(&p[1..], &t[1..], end),
    }
}

fn registrable(host: &str) -> String {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }
    let l: Vec<&str> = host.split('.').collect();
    let n = l.len();
    if n <= 2 {
        return host.to_string();
    }
    let second_level = matches!(l[n - 2], "co" | "com" | "org" | "net" | "gov" | "edu" | "ac") && l[n - 1].len() == 2;
    l[n - if second_level { 3 } else { 2 }..].join(".")
}

fn is_third(host: &str, page: &str) -> bool {
    !page.is_empty() && registrable(host) != registrable(page)
}

fn host_matches(list: &[String], page: &str) -> bool {
    list.iter().any(|d| page == d || page.ends_with(&format!(".{d}")))
}

impl Rule {
    fn applies(&self, full: &str, host: &str, page: &str, kind: Kind) -> bool {
        if self.kinds != 0 && self.kinds & kind_bit(kind) == 0 {
            return false;
        }
        if let Some(t) = self.third {
            if t != is_third(host, page) {
                return false;
            }
        }
        if !self.inc.is_empty() && !host_matches(&self.inc, page) {
            return false;
        }
        if !self.exc.is_empty() && host_matches(&self.exc, page) {
            return false;
        }
        let p = self.pat.as_bytes();
        let f = full.as_bytes();
        if self.host_anchor {
            let Some(s) = full.find("://") else { return false };
            let hs = s + 3;
            let he = full[hs..].find(['/', '?', '#', ':']).map(|i| hs + i).unwrap_or(full.len());
            let mut starts = vec![hs];
            starts.extend(full[hs..he].match_indices('.').map(|(i, _)| hs + i + 1));
            return starts.into_iter().any(|st| m(p, &f[st..], self.end_anchor));
        }
        if self.start_anchor {
            return m(p, f, self.end_anchor);
        }
        (0..=f.len()).any(|i| m(p, &f[i..], self.end_anchor))
    }
}

fn tokens(s: &str) -> Vec<&str> {
    s.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| t.len() >= 3).collect()
}

impl Index {
    fn add(&mut self, r: Rule) {
        if r.host_anchor {
            let h: String = r.pat.chars().take_while(|c| !matches!(c, '/' | '^' | '*' | ':' | '?')).collect();
            if h.contains('.') {
                self.by_host.entry(h).or_default().push(r);
                return;
            }
        }
        let best = tokens(&r.pat)
            .into_iter()
            .filter(|t| !r.pat.contains(&format!("*{t}")) && !r.pat.contains(&format!("{t}*")))
            .max_by_key(|t| t.len());
        match best {
            Some(t) => self.by_token.entry(t.to_string()).or_default().push(r),
            None => self.generic.push(r),
        }
    }

    fn hit(&self, full: &str, host: &str, page: &str, kind: Kind) -> bool {
        // host-anchored rules: try every suffix of the request host
        let mut h = host;
        loop {
            if let Some(v) = self.by_host.get(h) {
                if v.iter().any(|r| r.applies(full, host, page, kind)) {
                    return true;
                }
            }
            match h.find('.') {
                Some(i) => h = &h[i + 1..],
                None => break,
            }
        }
        let mut seen: HashSet<&str> = HashSet::new();
        for t in tokens(full) {
            if seen.insert(t) {
                if let Some(v) = self.by_token.get(t) {
                    if v.iter().any(|r| r.applies(full, host, page, kind)) {
                        return true;
                    }
                }
            }
        }
        self.generic.iter().any(|r| r.applies(full, host, page, kind))
    }
}

fn parse_rule(line: &str) -> Option<Rule> {
    let mut rule = Rule::default();
    let (body, opts) = match line.rfind('$') {
        Some(i)
            if line[i + 1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '~' | '=' | '|' | '_' | '-' | '.')) =>
        {
            (&line[..i], &line[i + 1..])
        }
        _ => (line, ""),
    };
    for o in opts.split(',').filter(|o| !o.is_empty()) {
        let (neg, name) = match o.strip_prefix('~') {
            Some(n) => (true, n),
            None => (false, o),
        };
        match name {
            "third-party" | "3p" => rule.third = Some(!neg),
            "script" => rule.kinds |= 1,
            "image" => rule.kinds |= 2,
            "stylesheet" | "css" => rule.kinds |= 4,
            "xmlhttprequest" | "xhr" => rule.kinds |= 8,
            "media" => rule.kinds |= 16,
            "font" => rule.kinds |= 32,
            "other" => rule.kinds |= 64,
            "document" | "doc" => rule.kinds |= 128,
            "important" | "match-case" => {}
            n if n.starts_with("domain=") => {
                for d in n[7..].split('|') {
                    match d.strip_prefix('~') {
                        Some(x) => rule.exc.push(x.to_lowercase()),
                        None => rule.inc.push(d.to_lowercase()),
                    }
                }
            }
            _ => return None, // unsupported option: skip the whole rule rather than over-block
        }
    }
    let mut pat = body;
    if let Some(p) = pat.strip_prefix("||") {
        rule.host_anchor = true;
        pat = p;
    } else if let Some(p) = pat.strip_prefix('|') {
        rule.start_anchor = true;
        pat = p;
    }
    if let Some(p) = pat.strip_suffix('|') {
        rule.end_anchor = true;
        pat = p;
    }
    if pat.is_empty() || (pat.starts_with('/') && pat.ends_with('/') && pat.len() > 2) {
        return None; // regex rules are not supported
    }
    rule.pat = pat.to_lowercase();
    Some(rule)
}

impl Engine {
    pub fn from_lists(lists: &[&str]) -> Engine {
        let mut e = Engine::default();
        for text in lists {
            for raw in text.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
                    continue;
                }
                if let Some(i) = line.find("##") {
                    let (doms, sel) = (&line[..i], &line[i + 2..]);
                    if sel.is_empty() || sel.contains(":-abp-") || sel.contains(":has-text") || sel.contains(":xpath") {
                        continue;
                    }
                    if doms.is_empty() {
                        e.generic_css.push(sel.to_string());
                    } else {
                        for d in doms.split(',').filter(|d| !d.starts_with('~')) {
                            e.site_css.entry(d.to_lowercase()).or_default().push(sel.to_string());
                        }
                    }
                    continue;
                }
                if line.contains("#@#") || line.contains("#?#") || line.contains("#$#") {
                    continue;
                }
                let (is_allow, rest) = match line.strip_prefix("@@") {
                    Some(r) => (true, r),
                    None => (false, line),
                };
                if let Some(r) = parse_rule(rest) {
                    e.rule_count += 1;
                    if is_allow {
                        e.allow.add(r)
                    } else {
                        e.block.add(r)
                    }
                }
            }
        }
        e
    }

    /// Should this subresource request be blocked?
    pub fn check(&self, url: &str, page_host: &str, kind: Kind) -> bool {
        let Ok(u) = Url::parse(url) else { return false };
        if !matches!(u.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = u.host_str() else { return false };
        let (host, page) = (host.to_lowercase(), page_host.to_lowercase());
        let full = url.to_lowercase();
        if self.allow.hit(&full, &host, &page, kind) {
            return false;
        }
        self.block.hit(&full, &host, &page, kind)
    }

    /// Generic (all-sites) cosmetic rules as one stylesheet, capped to keep the page script small.
    pub fn generic_css(&self) -> String {
        self.generic_css
            .iter()
            .take(3000)
            .collect::<Vec<_>>()
            .chunks(60)
            .map(|c| {
                format!(
                    "{}{{display:none!important}}",
                    c.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// CSS that hides ad containers on `host`.
    #[allow(dead_code)]
    pub fn cosmetic_css(&self, host: &str) -> String {
        let mut sels: Vec<&str> = self.generic_css.iter().map(|s| s.as_str()).collect();
        let mut h = host;
        loop {
            if let Some(v) = self.site_css.get(h) {
                sels.extend(v.iter().map(|s| s.as_str()));
            }
            match h.find('.') {
                Some(i) => h = &h[i + 1..],
                None => break,
            }
        }
        sels.chunks(60)
            .map(|c| format!("{}{{display:none!important}}", c.join(",")))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------- shared state used by the request hook ----------

pub struct Shared {
    pub engine: RwLock<Arc<Engine>>,
    pub enabled: AtomicBool,
    /// Hosts where blocking is switched off ("allow ads on this site").
    pub allow: RwLock<HashSet<String>>,
    pub total: AtomicUsize,
}

impl Shared {
    pub fn new(engine: Engine, enabled: bool, allow: HashSet<String>) -> Arc<Shared> {
        Arc::new(Shared {
            engine: RwLock::new(Arc::new(engine)),
            enabled: AtomicBool::new(enabled),
            allow: RwLock::new(allow),
            total: AtomicUsize::new(0),
        })
    }
    pub fn site_allowed(&self, host: &str) -> bool {
        let a = self.allow.read().unwrap();
        let mut h = host;
        loop {
            if a.contains(h) {
                return true;
            }
            match h.find('.') {
                Some(i) => h = &h[i + 1..],
                None => return false,
            }
        }
    }
}

/// Per-tab view of the blocker: which page it is on and how much it has blocked there.
pub struct TabBlock {
    pub shared: Arc<Shared>,
    pub host: Mutex<String>,
    pub blocked: AtomicUsize,
}

impl TabBlock {
    pub fn new(shared: Arc<Shared>) -> Arc<TabBlock> {
        Arc::new(TabBlock {
            shared,
            host: Mutex::new(String::new()),
            blocked: AtomicUsize::new(0),
        })
    }
    pub fn should_block(&self, url: &str, kind: Kind) -> bool {
        if !self.shared.enabled.load(Ordering::Relaxed) {
            return false;
        }
        let host = self.host.lock().unwrap().clone();
        if !host.is_empty() && self.shared.site_allowed(&host) {
            return false;
        }
        let engine = self.shared.engine.read().unwrap().clone();
        let hit = engine.check(url, &host, kind);
        if hit {
            self.blocked.fetch_add(1, Ordering::Relaxed);
            self.shared.total.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }
}

/// Hook WebView2's request pipeline so blocked requests get an empty 403 response.
#[cfg(windows)]
pub fn attach(view: &wry::WebView, tb: Arc<TabBlock>) {
    use webview2_com::{take_pwstr, Microsoft::Web::WebView2::Win32::*, WebResourceRequestedEventHandler};
    use windows::{
        core::{Interface, HSTRING, PWSTR},
        Win32::System::WinRT::EventRegistrationToken,
    };
    use wry::WebViewExtWindows;
    // SAFETY: COM calls on the webview owned by `view`, made on the UI thread that created it.
    unsafe {
        let controller = view.controller();
        let Ok(wv) = controller.CoreWebView2() else { return };
        let Ok(wv2) = wv.cast::<ICoreWebView2_2>() else { return };
        let Ok(env) = wv2.Environment() else { return };
        if wv
            .AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            .is_err()
        {
            return;
        }
        let mut token = EventRegistrationToken::default();
        let _ = wv.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let req = args.Request()?;
                let mut uri = PWSTR::null();
                req.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                if !(uri.starts_with("http://") || uri.starts_with("https://")) || uri.contains("wisp.localhost") {
                    return Ok(());
                }
                let mut ctx = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                args.ResourceContext(&mut ctx)?;
                let kind = if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT {
                    Kind::Document
                } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT {
                    Kind::Script
                } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE {
                    Kind::Image
                } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET {
                    Kind::Style
                } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST || ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH {
                    Kind::Xhr
                } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA {
                    Kind::Media
                } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT {
                    Kind::Font
                } else {
                    Kind::Other
                };
                if kind != Kind::Document && tb.should_block(&uri, kind) {
                    let resp = env.CreateWebResourceResponse(None, 403, &HSTRING::from("Blocked"), &HSTRING::from(""))?;
                    args.SetResponse(&resp)?;
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eng(s: &str) -> Engine {
        Engine::from_lists(&[s])
    }

    #[test]
    fn domain_rules_and_subdomains() {
        let e = eng("||ads.example.com^\n||tracker.net^");
        assert!(e.check("https://ads.example.com/x.js", "news.org", Kind::Script));
        assert!(e.check("https://a.b.tracker.net/p?x=1", "news.org", Kind::Image));
        assert!(!e.check("https://example.com/ads", "news.org", Kind::Script));
        assert!(!e.check("https://nottracker.net/x", "news.org", Kind::Script));
    }

    #[test]
    fn options_and_exceptions() {
        let e = eng("/banner/*$image,third-party\n@@||cdn.good.com^$image\n||x.com/ad.js$script,domain=a.com|~b.a.com");
        assert!(e.check("https://img.evil.com/banner/1.png", "site.org", Kind::Image));
        assert!(!e.check("https://img.evil.com/banner/1.png", "evil.com", Kind::Image)); // first party
        assert!(!e.check("https://img.evil.com/banner/1.js", "site.org", Kind::Script)); // wrong type
        assert!(!e.check("https://cdn.good.com/banner/1.png", "site.org", Kind::Image)); // exception
        assert!(e.check("https://x.com/ad.js", "a.com", Kind::Script));
        assert!(!e.check("https://x.com/ad.js", "b.a.com", Kind::Script));
        assert!(!e.check("https://x.com/ad.js", "other.com", Kind::Script));
    }

    #[test]
    fn wildcards_and_separators() {
        let e = eng("||site.com/ads/*/img^\n|https://ping.\n/pagead/ads");
        assert!(e.check("https://site.com/ads/9/img?q=1", "o.com", Kind::Image));
        assert!(!e.check("https://site.com/ads/9/imgs", "o.com", Kind::Image));
        assert!(e.check("https://ping.foo.com/a", "o.com", Kind::Xhr));
        assert!(!e.check("http://ping.foo.com/a", "o.com", Kind::Xhr));
        assert!(e.check("https://x.com/pagead/ads?id=1", "o.com", Kind::Script));
    }

    #[test]
    fn builtin_list_blocks_known_trackers_but_not_normal_sites() {
        let e = Engine::from_lists(&[include_str!("../assets/filters.txt")]);
        assert!(e.rule_count > 150);
        assert!(e.check("https://securepubads.g.doubleclick.net/gpt/pubads_impl.js", "cnn.com", Kind::Script));
        assert!(e.check("https://www.google-analytics.com/analytics.js", "bbc.co.uk", Kind::Script));
        assert!(e.check("https://connect.facebook.net/en_US/fbevents.js", "shop.com", Kind::Script));
        assert!(!e.check("https://www.google.com/search?q=hi", "google.com", Kind::Xhr));
        assert!(!e.check("https://github.githubassets.com/assets/app.js", "github.com", Kind::Script));
        assert!(!e.check("https://fonts.gstatic.com/s/roboto.woff2", "x.com", Kind::Font));
    }

    #[test]
    fn cosmetic_and_third_party() {
        let e = eng("##.ad-banner\nexample.com###sidebar-ad\n##div[id^=\"div-gpt-ad\"]");
        let css = e.cosmetic_css("www.example.com");
        assert!(css.contains(".ad-banner") && css.contains("#sidebar-ad") && css.contains("display:none!important"));
        assert!(!e.cosmetic_css("other.org").contains("#sidebar-ad"));
        assert_eq!(registrable("a.b.bbc.co.uk"), "bbc.co.uk");
        assert!(is_third("cdn.x.com", "y.com") && !is_third("cdn.x.com", "www.x.com"));
    }

    #[test]
    fn per_site_allowlist() {
        let sh = Shared::new(eng("||ads.net^"), true, ["news.org".to_string()].into());
        let tb = TabBlock::new(sh.clone());
        *tb.host.lock().unwrap() = "news.org".into();
        assert!(!tb.should_block("https://ads.net/a.js", Kind::Script));
        *tb.host.lock().unwrap() = "other.org".into();
        assert!(tb.should_block("https://ads.net/a.js", Kind::Script));
        assert_eq!(tb.blocked.load(Ordering::Relaxed), 1);
        sh.enabled.store(false, Ordering::Relaxed);
        assert!(!tb.should_block("https://ads.net/a.js", Kind::Script));
    }
}
