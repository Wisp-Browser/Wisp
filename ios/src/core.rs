//! Platform-independent part of Wisp for iOS: tabs, settings, persistence and the content blocker
//! rules. Kept free of Apple APIs so it builds and is unit-tested on any machine.

// Only the iOS app uses all of this; elsewhere it is built for the unit tests.
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// At most this many tabs keep a live page; older background tabs are unloaded (they reload,
/// from their address, when selected). Keeps memory low on the phone.
pub const MAX_LIVE: usize = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub engine: String,
    pub blocker: bool,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Where the bar sits: "bottom" | "top"
    pub bar: String,
    /// Frosted glass (system blur) or Flat (solid surfaces).
    pub glass: bool,
    /// "system" | "rounded" | "serif" | "mono" (all built into iOS)
    pub font: String,
    pub accent: String,
    /// New Tab background (a CSS background); empty = Wisp's default.
    pub wall: String,
    pub clock: bool,
    pub dial: bool,
    /// Show the bookmarks bar next to the address bar (when there are bookmarks).
    pub bmbar: bool,
    /// The first-run setup has been completed.
    pub setup_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: "ddg".into(),
            blocker: true,
            theme: "system".into(),
            bar: "bottom".into(),
            glass: true,
            font: "system".into(),
            accent: "#5e5ce6".into(),
            wall: String::new(),
            clock: true,
            dial: true,
            bmbar: true,
            setup_done: false,
        }
    }
}

/// What changed when a setting was applied, so the app redoes only that part.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Changed {
    Nothing,
    /// Engine, blocker or other values only the interface shows.
    Plain,
    Blocker,
    Theme,
    /// Bar position or glass: the native layout changes.
    Layout,
    /// Font, accent, New Tab look: the interface and New Tab pages restyle.
    Look,
}

fn one_of(v: &Value, allowed: &[&str]) -> Option<String> {
    v.as_str().filter(|s| allowed.contains(s)).map(String::from)
}

impl Settings {
    /// Apply one setting from the interface, validated. Unknown keys and bad values are ignored.
    pub fn set(&mut self, k: &str, v: &Value) -> Changed {
        let before = self.clone();
        let changed = match k {
            "engine" => {
                self.engine = crate::omni::engine_by_id(v.as_str().unwrap_or("")).id.to_string();
                Changed::Plain
            }
            "blocker" => {
                self.blocker = v.as_bool().unwrap_or(true);
                Changed::Blocker
            }
            "theme" => {
                if let Some(x) = one_of(v, &["system", "light", "dark"]) {
                    self.theme = x;
                }
                Changed::Theme
            }
            "bar" => {
                if let Some(x) = one_of(v, &["bottom", "top"]) {
                    self.bar = x;
                }
                Changed::Layout
            }
            "glass" => {
                self.glass = v.as_bool().unwrap_or(true);
                Changed::Layout
            }
            "font" => {
                if let Some(x) = one_of(v, &["system", "rounded", "serif", "mono"]) {
                    self.font = x;
                }
                Changed::Look
            }
            "accent" => {
                let a = v.as_str().unwrap_or("");
                if a.len() == 7 && a.starts_with('#') && a[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    self.accent = a.to_lowercase();
                }
                Changed::Look
            }
            "wall" => {
                // Only Wisp's own gradients/colours: no url(), no closing the declaration.
                let w = v.as_str().unwrap_or("");
                if w.len() < 400 && !w.contains(['{', '}', ';', '<', '>', '\\']) && !w.to_lowercase().contains("url(") {
                    self.wall = w.to_string();
                }
                Changed::Look
            }
            "clock" => {
                self.clock = v.as_bool().unwrap_or(true);
                Changed::Look
            }
            "dial" => {
                self.dial = v.as_bool().unwrap_or(true);
                Changed::Look
            }
            "bmbar" => {
                self.bmbar = v.as_bool().unwrap_or(true);
                Changed::Layout
            }
            "setup_done" => {
                self.setup_done = v.as_bool().unwrap_or(true);
                Changed::Plain
            }
            _ => Changed::Nothing,
        };
        if *self == before {
            Changed::Nothing
        } else {
            changed
        }
    }

    /// The New Tab page's look.
    pub fn newtab(&self) -> Value {
        json!({"font":self.font,"accent":self.accent,"wall":self.wall,"clock":self.clock,"dial":self.dial})
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TabState {
    pub id: u64,
    /// Empty for the New Tab page.
    pub url: String,
    pub title: String,
    pub icon: String,
    #[serde(skip)]
    pub loading: bool,
    #[serde(skip)]
    pub desktop: bool,
    /// Higher = used more recently.
    #[serde(skip)]
    pub used: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub icon: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<TabState>,
    pub active: u64,
    pub next_id: u64,
    pub settings: Settings,
    pub bookmarks: Vec<Bookmark>,
}

impl Default for Session {
    fn default() -> Self {
        let mut s = Session {
            tabs: vec![],
            active: 0,
            next_id: 1,
            settings: Settings::default(),
            bookmarks: vec![],
        };
        let id = s.add_tab("");
        s.active = id;
        s
    }
}

impl Session {
    pub fn load(json: &str) -> Session {
        let mut s: Session = serde_json::from_str(json).unwrap_or_default();
        if s.tabs.is_empty() {
            return Session {
                settings: s.settings,
                bookmarks: s.bookmarks,
                ..Session::default()
            };
        }
        s.next_id = s.next_id.max(s.tabs.iter().map(|t| t.id).max().unwrap_or(0) + 1);
        if !s.tabs.iter().any(|t| t.id == s.active) {
            s.active = s.tabs[0].id;
        }
        s
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn add_tab(&mut self, url: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(TabState {
            id,
            url: url.into(),
            ..Default::default()
        });
        id
    }

    pub fn tab(&self, id: u64) -> Option<&TabState> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: u64) -> Option<&mut TabState> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// The bookmarks bar is on screen.
    pub fn bmbar_shown(&self) -> bool {
        self.settings.bmbar && !self.bookmarks.is_empty()
    }

    /// Bookmark the active page, or remove its bookmark. Returns whether it is now bookmarked.
    pub fn toggle_bookmark(&mut self) -> bool {
        let Some(t) = self.tab(self.active).cloned() else { return false };
        if !t.url.starts_with("http") {
            return false;
        }
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == t.url) {
            self.bookmarks.remove(i);
            return false;
        }
        let title = if t.title.is_empty() { t.url.clone() } else { t.title.clone() };
        self.bookmarks.push(Bookmark {
            url: t.url,
            title,
            icon: t.icon,
        });
        true
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.bookmarks.retain(|b| b.url != url);
    }

    /// Mark `id` active and most recently used.
    pub fn select(&mut self, id: u64) {
        if self.tab(id).is_none() {
            return;
        }
        let top = self.tabs.iter().map(|t| t.used).max().unwrap_or(0) + 1;
        self.active = id;
        if let Some(t) = self.tab_mut(id) {
            t.used = top;
        }
    }

    /// Close a tab; returns the tab to show next. Closing the last tab leaves a fresh New Tab.
    pub fn close(&mut self, id: u64) -> u64 {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
            return self.active;
        };
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            let n = self.add_tab("");
            self.select(n);
            return n;
        }
        if self.active == id {
            // The most recently used remaining tab, like Safari.
            let next = self.tabs.iter().max_by_key(|t| t.used).map(|t| t.id).unwrap_or(self.tabs[0].id);
            self.select(next);
        }
        self.active
    }

    /// Tabs whose page should be unloaded: everything except the `MAX_LIVE` most recently used
    /// (the active tab always stays). `live` lists tabs that currently have a page.
    pub fn to_unload(&self, live: &[u64]) -> Vec<u64> {
        let mut l: Vec<&TabState> = self.tabs.iter().filter(|t| live.contains(&t.id)).collect();
        l.sort_by_key(|t| std::cmp::Reverse(t.used));
        l.iter().skip(MAX_LIVE).filter(|t| t.id != self.active).map(|t| t.id).collect()
    }

    /// The state the interface draws.
    pub fn view_state(&self, can_back: bool, can_fwd: bool) -> Value {
        let a = self.tab(self.active).cloned().unwrap_or_default();
        let engines: Vec<Value> = crate::omni::ENGINES.iter().map(|e| json!({"id":e.id,"name":e.name})).collect();
        json!({
            "tabs": self.tabs.iter().map(|t| json!({"id":t.id,"url":t.url,"title":t.title,"icon":t.icon})).collect::<Vec<_>>(),
            "active": self.active,
            "url": a.url, "title": a.title, "loading": a.loading, "desktop": a.desktop,
            "back": can_back, "fwd": can_fwd,
            "engine": self.settings.engine, "engines": engines, "blocker": self.settings.blocker,
            "set": self.settings,
            "bookmarks": self.bookmarks,
            "bookmarked": self.bookmarks.iter().any(|b| b.url == a.url),
            "version": env!("CARGO_PKG_VERSION"),
        })
    }
}

/// Ad and tracker hosts blocked as third-party loads (WebKit content rule list).
const BLOCKED: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "google-analytics.com",
    "googletagmanager.com",
    "googletagservices.com",
    "adservice.google.com",
    "pagead2.googlesyndication.com",
    "facebook.net",
    "connect.facebook.net",
    "ads-twitter.com",
    "analytics.twitter.com",
    "amazon-adsystem.com",
    "adnxs.com",
    "adsrvr.org",
    "criteo.com",
    "criteo.net",
    "taboola.com",
    "outbrain.com",
    "rubiconproject.com",
    "pubmatic.com",
    "openx.net",
    "casalemedia.com",
    "indexww.com",
    "scorecardresearch.com",
    "quantserve.com",
    "moatads.com",
    "hotjar.com",
    "mouseflow.com",
    "fullstory.com",
    "mixpanel.com",
    "segment.io",
    "segment.com",
    "branch.io",
    "app-measurement.com",
    "adcolony.com",
    "applovin.com",
    "unityads.unity3d.com",
    "chartbeat.com",
    "chartbeat.net",
    "newrelic.com",
    "nr-data.net",
    "bugsnag.com",
    "yieldmo.com",
    "sharethrough.com",
    "smartadserver.com",
    "teads.tv",
    "media.net",
    "zedo.com",
    "33across.com",
    "bidswitch.net",
    "lijit.com",
    "sovrn.com",
    "contextweb.com",
    "advertising.com",
    "yahoo-ads.com",
    "adform.net",
    "serving-sys.com",
    "bluekai.com",
    "krxd.net",
    "demdex.net",
    "everesttech.net",
    "omtrdc.net",
    "tiqcdn.com",
    "clarity.ms",
    "bat.bing.com",
    "ads.linkedin.com",
    "px.ads.linkedin.com",
    "snap.licdn.com",
    "tr.snapchat.com",
    "ads.tiktok.com",
    "analytics.tiktok.com",
    "ads.pinterest.com",
    "ct.pinterest.com",
    "adroll.com",
    "doubleverify.com",
    "adsafeprotected.com",
    "imrworldwide.com",
    "exelator.com",
    "agkn.com",
];

/// Element hiding for common ad slots that survive request blocking.
const HIDE: &str =
    ".adsbygoogle, ins.adsbygoogle, [id^='google_ads_iframe'], [id^='div-gpt-ad'], .ad-slot, .ad-banner, .advertisement, [data-ad-slot]";

/// WebKit content rule list JSON for the blocker.
pub fn block_rules() -> String {
    let mut rules: Vec<Value> = BLOCKED
        .iter()
        .map(|d| {
            let host = d.replace('.', "\\.");
            json!({"trigger":{"url-filter":format!("^[^:]+://+([^:/]+\\.)?{host}[:/]"),"load-type":["third-party"]},
                   "action":{"type":"block"}})
        })
        .collect();
    rules.push(json!({"trigger":{"url-filter":".*"},"action":{"type":"css-display-none","selector":HIDE}}));
    Value::Array(rules).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_has_one_new_tab() {
        let s = Session::default();
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.active, s.tabs[0].id);
        assert!(s.tabs[0].url.is_empty());
    }

    #[test]
    fn closing_picks_most_recent_and_never_empties() {
        let mut s = Session::default();
        let a = s.tabs[0].id;
        let b = s.add_tab("https://b.example/");
        let c = s.add_tab("https://c.example/");
        s.select(b);
        s.select(c);
        assert_eq!(s.close(c), b);
        s.close(b);
        assert_eq!(s.active, a);
        let n = s.close(a);
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.active, n);
    }

    #[test]
    fn unloads_all_but_recent_and_active() {
        let mut s = Session::default();
        let ids: Vec<u64> = (0..5).map(|i| s.add_tab(&format!("https://{i}.example/"))).collect();
        for id in &ids {
            s.select(*id);
        }
        let live: Vec<u64> = s.tabs.iter().map(|t| t.id).collect();
        let gone = s.to_unload(&live);
        assert_eq!(gone.len(), live.len() - MAX_LIVE);
        assert!(!gone.contains(&s.active));
        assert!(!gone.contains(&ids[4]) && !gone.contains(&ids[3]) && !gone.contains(&ids[2]));
    }

    #[test]
    fn session_roundtrip_and_bad_input() {
        let mut s = Session::default();
        s.add_tab("https://x.example/");
        s.settings.engine = "google".into();
        let back = Session::load(&s.to_json());
        assert_eq!(back.tabs.len(), 2);
        assert_eq!(back.settings.engine, "google");
        assert_eq!(Session::load("not json").tabs.len(), 1);
    }

    #[test]
    fn settings_validate() {
        let mut s = Settings::default();
        assert_eq!(s.set("theme", &json!("dark")), Changed::Theme);
        assert_eq!(s.set("theme", &json!("dark")), Changed::Nothing);
        assert_eq!(s.set("theme", &json!("purple")), Changed::Nothing);
        assert_eq!(s.set("bar", &json!("top")), Changed::Layout);
        assert_eq!(s.set("accent", &json!("#FF0000")), Changed::Look);
        assert_eq!(s.accent, "#ff0000");
        assert_eq!(s.set("accent", &json!("red;x")), Changed::Nothing);
        assert_eq!(s.set("wall", &json!("linear-gradient(160deg,#111,#333)")), Changed::Look);
        assert_eq!(s.set("wall", &json!("url(https://x)")), Changed::Nothing);
        assert_eq!(s.set("wall", &json!("red}body{")), Changed::Nothing);
        assert_eq!(s.set("nope", &json!(1)), Changed::Nothing);
        assert_eq!(s.set("engine", &json!("google")), Changed::Plain);
        assert_eq!(s.engine, "google");
    }

    #[test]
    fn old_settings_files_get_new_defaults() {
        let s = Session::load(r#"{"tabs":[{"id":1,"url":"https://a.example/"}],"active":1,"settings":{"engine":"bing","blocker":false}}"#);
        assert_eq!(s.settings.engine, "bing");
        assert!(s.settings.glass && !s.settings.setup_done);
        assert_eq!(s.settings.bar, "bottom");
    }

    #[test]
    fn bookmarks_toggle_and_survive_reset() {
        let mut s = Session::default();
        assert!(!s.toggle_bookmark(), "New Tab can't be bookmarked");
        let id = s.add_tab("https://a.example/");
        s.select(id);
        s.tab_mut(id).unwrap().title = "A".into();
        assert!(s.toggle_bookmark());
        assert!(s.bmbar_shown());
        assert_eq!(s.bookmarks[0].title, "A");
        assert!(!s.toggle_bookmark());
        assert!(!s.bmbar_shown());
        s.toggle_bookmark();
        let json = s.to_json();
        let back = Session::load(&json);
        assert_eq!(back.bookmarks.len(), 1);
        s.remove_bookmark("https://a.example/");
        assert!(s.bookmarks.is_empty());
    }

    #[test]
    fn rules_are_valid_json() {
        let v: Value = serde_json::from_str(&block_rules()).unwrap();
        assert_eq!(v.as_array().unwrap().len(), BLOCKED.len() + 1);
    }
}
