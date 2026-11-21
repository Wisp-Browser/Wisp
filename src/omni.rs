//! Omnibox input resolution: URL vs. search, engine prefixes.

use url::Url;

#[derive(Clone, Debug)]
pub struct Engine {
    pub id: &'static str,
    pub name: &'static str,
    pub prefix: &'static str,
    pub template: &'static str,
}

pub const ENGINES: &[Engine] = &[
    Engine {
        id: "google",
        name: "Google",
        prefix: "g",
        template: "https://www.google.com/search?q=%s",
    },
    Engine {
        id: "ddg",
        name: "DuckDuckGo",
        prefix: "d",
        template: "https://duckduckgo.com/?q=%s",
    },
    Engine {
        id: "brave",
        name: "Brave",
        prefix: "b",
        template: "https://search.brave.com/search?q=%s",
    },
    Engine {
        id: "bing",
        name: "Bing",
        prefix: "bi",
        template: "https://www.bing.com/search?q=%s",
    },
    Engine {
        id: "startpage",
        name: "Startpage",
        prefix: "s",
        template: "https://www.startpage.com/do/search?q=%s",
    },
    Engine {
        id: "kagi",
        name: "Kagi",
        prefix: "k",
        template: "https://kagi.com/search?q=%s",
    },
];

pub fn engine_by_id(id: &str) -> &'static Engine {
    ENGINES.iter().find(|e| e.id == id).unwrap_or(&ENGINES[1])
}

fn encode(q: &str) -> String {
    url::form_urlencoded::byte_serialize(q.as_bytes()).collect()
}

fn looks_like_url(s: &str) -> Option<String> {
    if s.contains(' ') {
        return None;
    }
    if let Ok(u) = Url::parse(s) {
        if matches!(u.scheme(), "https" | "file" | "about" | "wisp" | "chrome-extension") || (u.scheme() == "http" && u.has_host()) {
            return Some(s.to_string());
        }
    }
    let host = s.split(['/', '?', '#']).next().unwrap_or("");
    let host_nport = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    let is_local = host_nport == "localhost";
    let is_ip = host_nport.split('.').count() == 4 && host_nport.split('.').all(|p| p.parse::<u8>().is_ok());
    let has_tld = host_nport.contains('.') && !host_nport.starts_with('.') && !host_nport.ends_with('.');
    if is_local || is_ip || has_tld {
        let scheme = if is_local || is_ip { "http" } else { "https" };
        return Some(format!("{scheme}://{s}"));
    }
    None
}

/// Resolve what the user typed into a navigable URL.
pub fn resolve(input: &str, default_engine: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return "wisp://newtab".into();
    }
    if let Some(u) = looks_like_url(input) {
        return u;
    }
    if let Some((p, rest)) = input.split_once(' ') {
        if let Some(e) = ENGINES.iter().find(|e| e.prefix == p) {
            if !rest.trim().is_empty() {
                return e.template.replace("%s", &encode(rest.trim()));
            }
        }
    }
    engine_by_id(default_engine).template.replace("%s", &encode(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls() {
        assert_eq!(resolve("github.com", "ddg"), "https://github.com");
        assert_eq!(resolve("localhost:3000", "ddg"), "http://localhost:3000");
        assert_eq!(resolve("https://a.b/c", "ddg"), "https://a.b/c");
        assert_eq!(resolve("192.168.1.1", "ddg"), "http://192.168.1.1");
    }
    #[test]
    fn search() {
        assert_eq!(resolve("cats and dogs", "ddg"), "https://duckduckgo.com/?q=cats+and+dogs");
        assert_eq!(resolve("g rust lang", "ddg"), "https://www.google.com/search?q=rust+lang");
        assert_eq!(resolve("d cats", "google"), "https://duckduckgo.com/?q=cats");
        assert_eq!(resolve("", "ddg"), "wisp://newtab");
    }
}
