//! Bookmarks, history, downloads, and bookmark import (Chrome/Edge/Brave JSON, Firefox places.sqlite, HTML exports).

use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub folder: String,
    pub ts: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub ts: u64,
    pub n: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct Download {
    pub id: usize,
    pub name: String,
    pub url: String,
    pub path: String,
    pub state: String, // "progress" | "done" | "failed"
    pub ts: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Default)]
pub struct Library {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<Visit>,
}

const HISTORY_CAP: usize = 5000;

fn trackable(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://")) && !url.contains("wisp.localhost")
}

impl Library {
    pub fn load() -> Library {
        Library {
            bookmarks: store::load_json("bookmarks.json"),
            history: store::load_json("history.json"),
        }
    }
    pub fn save_bookmarks(&self) {
        store::save_json("bookmarks.json", &self.bookmarks);
    }
    pub fn save_history(&self) {
        store::save_json("history.json", &self.history);
    }

    /// Record a finished page load (newest first, de-duplicated by URL).
    pub fn add_visit(&mut self, url: &str, title: &str) {
        if !trackable(url) {
            return;
        }
        let ts = now();
        if let Some(i) = self.history.iter().position(|v| v.url == url) {
            let mut v = self.history.remove(i);
            v.n += 1;
            v.ts = ts;
            if !title.is_empty() && title != url {
                v.title = title.to_string();
            }
            self.history.insert(0, v);
        } else {
            self.history.insert(
                0,
                Visit {
                    url: url.into(),
                    title: title.into(),
                    ts,
                    n: 1,
                },
            );
            self.history.truncate(HISTORY_CAP);
        }
        self.save_history();
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    /// Returns true when the page is now bookmarked.
    pub fn toggle_bookmark(&mut self, url: &str, title: &str) -> bool {
        if !trackable(url) {
            return false;
        }
        let now_marked = if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
            false
        } else {
            self.bookmarks.insert(
                0,
                Bookmark {
                    url: url.into(),
                    title: if title.is_empty() { url.into() } else { title.into() },
                    folder: String::new(),
                    ts: now(),
                },
            );
            true
        };
        self.save_bookmarks();
        now_marked
    }

    /// Rename a bookmark and/or change its address. Returns false if `url` isn't bookmarked or the
    /// new address isn't a web page.
    pub fn edit_bookmark(&mut self, url: &str, new_url: &str, title: &str) -> bool {
        if !trackable(new_url) {
            return false;
        }
        // Changing the address onto another bookmark's: keep just this one.
        if new_url != url {
            self.bookmarks.retain(|b| b.url != new_url);
        }
        let Some(b) = self.bookmarks.iter_mut().find(|b| b.url == url) else {
            return false;
        };
        b.url = new_url.to_string();
        b.title = if title.trim().is_empty() {
            new_url.to_string()
        } else {
            title.trim().to_string()
        };
        self.save_bookmarks();
        true
    }

    pub fn import(&mut self, items: Vec<Bookmark>) -> usize {
        let mut added = 0;
        for b in items {
            if trackable(&b.url) && !self.is_bookmarked(&b.url) {
                self.bookmarks.push(b);
                added += 1;
            }
        }
        if added > 0 {
            self.save_bookmarks();
        }
        added
    }

    pub fn search_bookmarks(&self, q: &str, limit: usize) -> Vec<&Bookmark> {
        let q = q.to_lowercase();
        self.bookmarks
            .iter()
            .filter(|b| q.is_empty() || b.title.to_lowercase().contains(&q) || b.url.to_lowercase().contains(&q))
            .take(limit)
            .collect()
    }

    pub fn search_history(&self, q: &str, limit: usize) -> Vec<&Visit> {
        let q = q.to_lowercase();
        self.history
            .iter()
            .filter(|v| q.is_empty() || v.title.to_lowercase().contains(&q) || v.url.to_lowercase().contains(&q))
            .take(limit)
            .collect()
    }

    /// Omnibox suggestions: bookmarks first, then frequently/recently visited pages.
    pub fn suggest(&self, q: &str, limit: usize) -> Vec<(String, String, String)> {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return vec![];
        }
        let mut out: Vec<(String, String, String)> = vec![];
        for b in self
            .bookmarks
            .iter()
            .filter(|b| b.title.to_lowercase().contains(&q) || b.url.to_lowercase().contains(&q))
            .take(limit)
        {
            out.push(("bookmark".into(), b.title.clone(), b.url.clone()));
        }
        let mut hist: Vec<&Visit> = self
            .history
            .iter()
            .filter(|v| v.title.to_lowercase().contains(&q) || v.url.to_lowercase().contains(&q))
            .collect();
        hist.sort_by_key(|v| std::cmp::Reverse(v.n));
        for v in hist.into_iter().take(limit) {
            if !out.iter().any(|(_, _, u)| *u == v.url) {
                out.push(("history".into(), v.title.clone(), v.url.clone()));
            }
        }
        out.truncate(limit);
        out
    }
}

pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let name = name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let name = if name.trim().is_empty() { "download".to_string() } else { name };
    let first = dir.join(&name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    for n in 1..1000 {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    first
}

// ---------- import ----------

fn walk_chrome(node: &serde_json::Value, path: &str, out: &mut Vec<Bookmark>) {
    match node["type"].as_str() {
        Some("url") => {
            if let Some(u) = node["url"].as_str() {
                out.push(Bookmark {
                    url: u.into(),
                    title: node["name"].as_str().unwrap_or(u).into(),
                    folder: path.into(),
                    ts: now(),
                });
            }
        }
        Some("folder") => {
            let name = node["name"].as_str().unwrap_or("");
            let p = if path.is_empty() {
                name.to_string()
            } else {
                format!("{path} / {name}")
            };
            for c in node["children"].as_array().into_iter().flatten() {
                walk_chrome(c, &p, out);
            }
        }
        _ => {}
    }
}

/// Chrome, Edge and Brave all use the same `Bookmarks` JSON file.
pub fn parse_chrome_json(text: &str) -> Vec<Bookmark> {
    let mut out = vec![];
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        for root in ["bookmark_bar", "other", "synced"] {
            if v["roots"][root].is_object() {
                walk_chrome(&v["roots"][root], "", &mut out);
            }
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Netscape bookmark HTML (what Firefox, Chrome and Edge export).
pub fn parse_netscape(text: &str) -> Vec<Bookmark> {
    let low = text.to_ascii_lowercase(); // ASCII-only lowercase keeps byte offsets identical
    let mut out = vec![];
    let mut stack: Vec<String> = vec![];
    let mut pending: Option<String> = None;
    let mut i = 0;
    while let Some(off) = low[i..].find('<') {
        let at = i + off;
        let rest = &low[at..];
        if rest.starts_with("<h3") {
            if let (Some(gt), Some(end)) = (rest.find('>'), rest.find("</h3>")) {
                pending = Some(unescape(text[at + gt + 1..at + end].trim()));
                i = at + end + 5;
                continue;
            }
        } else if rest.starts_with("<dl") {
            stack.push(pending.take().unwrap_or_default());
        } else if rest.starts_with("</dl") {
            stack.pop();
        } else if rest.starts_with("<a ") || rest.starts_with("<a\n") {
            if let (Some(gt), Some(end)) = (rest.find('>'), rest.find("</a>")) {
                let tag = &text[at..at + gt];
                let tag_low = &low[at..at + gt];
                if let Some(h) = tag_low.find("href=\"") {
                    let s = h + 6;
                    if let Some(e) = tag[s..].find('"') {
                        let url = unescape(&tag[s..s + e]);
                        let title = unescape(text[at + gt + 1..at + end].trim());
                        let folder = stack.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" / ");
                        out.push(Bookmark {
                            url,
                            title,
                            folder,
                            ts: now(),
                        });
                    }
                }
                i = at + end + 4;
                continue;
            }
        }
        i = at + 1;
    }
    out
}

pub fn read_firefox(places: &Path) -> Result<Vec<Bookmark>, String> {
    let tmp = std::env::temp_dir().join(format!("wisp-places-{}.sqlite", std::process::id()));
    std::fs::copy(places, &tmp).map_err(|_| "Couldn't read the Firefox profile. Close Firefox and try again.".to_string())?;
    let res = (|| -> Result<Vec<Bookmark>, rusqlite::Error> {
        let conn = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut st = conn.prepare(
            "SELECT b.title, p.url, COALESCE(parent.title, '') FROM moz_bookmarks b \
             JOIN moz_places p ON b.fk = p.id LEFT JOIN moz_bookmarks parent ON b.parent = parent.id \
             WHERE b.type = 1 AND p.url LIKE 'http%'",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Bookmark {
                title: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                url: r.get(1)?,
                folder: r.get(2)?,
                ts: now(),
            })
        })?;
        rows.collect()
    })();
    let _ = std::fs::remove_file(&tmp);
    res.map_err(|e| format!("Couldn't read Firefox bookmarks: {e}"))
}

pub struct Source {
    pub id: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
}

/// Browsers whose bookmarks we can find on this machine.
pub fn detect_sources() -> Vec<Source> {
    let mut v = vec![];
    if let Some(l) = dirs::data_local_dir() {
        for (id, label, rel) in [
            ("chrome", "Google Chrome", "Google/Chrome/User Data/Default/Bookmarks"),
            ("edge", "Microsoft Edge", "Microsoft/Edge/User Data/Default/Bookmarks"),
            ("brave", "Brave", "BraveSoftware/Brave-Browser/User Data/Default/Bookmarks"),
        ] {
            let p = l.join(rel);
            if p.is_file() {
                v.push(Source { id, label, path: p });
            }
        }
    }
    if let Some(r) = dirs::data_dir() {
        let profiles = r.join("Mozilla/Firefox/Profiles");
        let best = std::fs::read_dir(profiles)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("places.sqlite"))
            .filter(|p| p.is_file())
            .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        if let Some(p) = best {
            v.push(Source {
                id: "firefox",
                label: "Mozilla Firefox",
                path: p,
            });
        }
    }
    v
}

pub fn import_from(id: &str, path: &Path) -> Result<Vec<Bookmark>, String> {
    match id {
        "firefox" => read_firefox(path),
        "html" => Ok(parse_netscape(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)),
        _ => Ok(parse_chrome_json(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_json() {
        let j = r#"{"roots":{"bookmark_bar":{"type":"folder","name":"Bookmarks bar","children":[
          {"type":"url","name":"Rust","url":"https://rust-lang.org/"},
          {"type":"folder","name":"Dev","children":[{"type":"url","name":"Crates","url":"https://crates.io"}]}]},
          "other":{"type":"folder","name":"Other","children":[]}}}"#;
        let b = parse_chrome_json(j);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1].folder, "Bookmarks bar / Dev");
    }

    #[test]
    fn netscape_html() {
        let h = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p>
          <DT><H3>News</H3><DL><p><DT><A HREF="https://a.com/?x=1&amp;y=2" ADD_DATE="1">Site &amp; A</A></DL><p>
          <DT><A HREF="https://b.com">B</A></DL>"#;
        let b = parse_netscape(h);
        assert_eq!(b.len(), 2);
        assert_eq!(
            (b[0].url.as_str(), b[0].title.as_str(), b[0].folder.as_str()),
            ("https://a.com/?x=1&y=2", "Site & A", "News")
        );
        assert_eq!(b[1].folder, "");
    }

    #[test]
    fn firefox_places() {
        let p = std::env::temp_dir().join(format!("wisp-test-places-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let c = rusqlite::Connection::open(&p).unwrap();
            c.execute_batch(
                "CREATE TABLE moz_places(id INTEGER PRIMARY KEY, url TEXT); CREATE TABLE moz_bookmarks(id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, title TEXT);
                 INSERT INTO moz_places VALUES(1,'https://mozilla.org'),(2,'place:folder=1');
                 INSERT INTO moz_bookmarks VALUES(1,2,NULL,0,'Menu'),(2,1,1,1,'Mozilla'),(3,1,2,1,'Smart');",
            )
            .unwrap();
        }
        let b = read_firefox(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].title.as_str(), b[0].folder.as_str()), ("Mozilla", "Menu"));
    }

    #[test]
    fn history_bookmarks_and_suggestions() {
        let mut l = Library::default();
        // avoid touching disk in tests: exercise pure logic
        l.history.push(Visit {
            url: "https://rust-lang.org/".into(),
            title: "Rust".into(),
            ts: 1,
            n: 5,
        });
        l.history.push(Visit {
            url: "https://example.com/rusty".into(),
            title: "Rusty".into(),
            ts: 2,
            n: 1,
        });
        l.bookmarks.push(Bookmark {
            url: "https://doc.rust-lang.org".into(),
            title: "Rust docs".into(),
            folder: String::new(),
            ts: 0,
        });
        let s = l.suggest("rust", 5);
        assert_eq!(s[0].0, "bookmark");
        assert_eq!(s[1].2, "https://rust-lang.org/");
        assert!(l.suggest("", 5).is_empty());
        assert_eq!(
            l.import(vec![Bookmark {
                url: "https://doc.rust-lang.org".into(),
                ..Default::default()
            }]),
            0
        );
    }

    #[test]
    fn unique_download_names() {
        let d = std::env::temp_dir().join(format!("wisp-dl-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.zip"), b"x").unwrap();
        assert_eq!(unique_path(&d, "a.zip"), d.join("a (1).zip"));
        assert_eq!(unique_path(&d, "b/c:d.txt"), d.join("b_c_d.txt"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
