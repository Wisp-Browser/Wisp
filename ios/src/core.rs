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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: "ddg".into(),
            blocker: true,
        }
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<TabState>,
    pub active: u64,
    pub next_id: u64,
    pub settings: Settings,
}

impl Default for Session {
    fn default() -> Self {
        let mut s = Session {
            tabs: vec![],
            active: 0,
            next_id: 1,
            settings: Settings::default(),
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
    fn rules_are_valid_json() {
        let v: Value = serde_json::from_str(&block_rules()).unwrap();
        assert_eq!(v.as_array().unwrap().len(), BLOCKED.len() + 1);
    }
}
