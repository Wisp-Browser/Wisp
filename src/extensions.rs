//! Chrome extensions: download from the Chrome Web Store, unpack the CRX, and load it into every
//! WebView2 profile (WebView2's own extension support, the same engine Edge uses).
//!
//! Extensions live unpacked in `<data>/extensions/<id>/`. The CRX's public key is written into the
//! manifest as `"key"`, so the extension keeps its Web Store ID (OAuth logins, messaging between
//! extensions and sites that check the ID all depend on it), exactly as in Chrome.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const LIST: &str = "extensions.json";
/// Largest CRX we download (the biggest Web Store extensions are well under this).
const MAX_CRX: u64 = 300 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct Ext {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    /// Paths inside the extension folder ("" when it has none).
    pub icon: String,
    pub popup: String,
    pub options: String,
    /// What the extension can do, in Chrome's words (shown before installing).
    pub warnings: Vec<String>,
    /// Changes on every install/update so loaded profiles know to reload it.
    pub stamp: u64,
}

pub fn root() -> PathBuf {
    crate::store::real_data_dir().join("extensions")
}
pub fn dir_of(id: &str) -> PathBuf {
    root().join(id)
}

pub fn load() -> Vec<Ext> {
    let mut v: Vec<Ext> = crate::store::load_json(LIST);
    v.retain(|e| valid_id(&e.id) && dir_of(&e.id).join("manifest.json").is_file());
    v
}
pub fn save(list: &[Ext]) {
    crate::store::save_json(LIST, &list);
}

/// A Chrome extension ID: 32 letters a–p.
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// Pull an extension ID out of a Web Store link (old or new store) or a bare ID.
pub fn id_from_input(s: &str) -> Option<String> {
    let s = s.trim();
    if valid_id(s) {
        return Some(s.to_string());
    }
    let path = s.split(['?', '#']).next().unwrap_or("");
    path.split('/').rev().find(|p| valid_id(p)).map(String::from)
}

/// The ID Chrome derives from a public key: SHA-256, first 16 bytes, each nibble as a–p.
pub fn id_from_key(key: &[u8]) -> String {
    let h = Sha256::digest(key);
    h[..16].iter().flat_map(|b| [b >> 4, b & 15]).map(|n| (b'a' + n) as char).collect()
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Minimal protobuf reader: (field number, wire type, payload for length-delimited fields).
fn proto_fields(mut b: &[u8]) -> Vec<(u64, &[u8])> {
    fn varint(b: &mut &[u8]) -> Option<u64> {
        let (mut v, mut s) = (0u64, 0);
        loop {
            let (&x, rest) = b.split_first()?;
            *b = rest;
            v |= ((x & 0x7f) as u64) << s;
            if x & 0x80 == 0 {
                return Some(v);
            }
            s += 7;
            if s > 63 {
                return None;
            }
        }
    }
    let mut out = vec![];
    while !b.is_empty() {
        let Some(tag) = varint(&mut b) else { break };
        match tag & 7 {
            0 => {
                if varint(&mut b).is_none() {
                    break;
                }
            }
            1 if b.len() >= 8 => b = &b[8..],
            5 if b.len() >= 4 => b = &b[4..],
            2 => {
                let Some(n) = varint(&mut b) else { break };
                let n = n as usize;
                if n > b.len() {
                    break;
                }
                out.push((tag >> 3, &b[..n]));
                b = &b[n..];
            }
            _ => break,
        }
    }
    out
}

/// Split a CRX (version 2 or 3) into (public key, zip archive). For CRX3 the key whose ID matches
/// the signed CRX ID is picked.
pub fn parse_crx(data: &[u8]) -> Result<(Vec<u8>, &[u8]), String> {
    let u32_at = |i: usize| -> Result<usize, String> {
        data.get(i..i + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
            .ok_or_else(|| "The extension file is damaged".to_string())
    };
    if data.len() < 16 || &data[..4] != b"Cr24" {
        // Some downloads are a plain zip.
        if data.starts_with(b"PK") {
            return Ok((vec![], data));
        }
        return Err("That isn't a Chrome extension file".into());
    }
    match u32_at(4)? {
        2 => {
            let (kl, sl) = (u32_at(8)?, u32_at(12)?);
            let start = 16 + kl + sl;
            let key = data.get(16..16 + kl).ok_or("The extension file is damaged")?.to_vec();
            Ok((key, data.get(start..).ok_or("The extension file is damaged")?))
        }
        3 => {
            let hl = u32_at(8)?;
            let header = data.get(12..12 + hl).ok_or("The extension file is damaged")?;
            let zip = data.get(12 + hl..).ok_or("The extension file is damaged")?;
            let fields = proto_fields(header);
            // signed_header_data (10000) → SignedData.crx_id (1)
            let crx_id: Option<String> = fields.iter().find(|(f, _)| *f == 10000).and_then(|(_, sd)| {
                proto_fields(sd)
                    .into_iter()
                    .find(|(f, _)| *f == 1)
                    .map(|(_, id)| id.iter().flat_map(|b| [b >> 4, b & 15]).map(|n| (b'a' + n) as char).collect())
            });
            // sha256_with_rsa (2) / sha256_with_ecdsa (3) → AsymmetricKeyProof.public_key (1)
            let keys: Vec<Vec<u8>> = fields
                .iter()
                .filter(|(f, _)| *f == 2 || *f == 3)
                .filter_map(|(_, p)| proto_fields(p).into_iter().find(|(f, _)| *f == 1).map(|(_, k)| k.to_vec()))
                .collect();
            let key = keys
                .iter()
                .find(|k| crx_id.as_deref() == Some(id_from_key(k).as_str()))
                .or(keys.first())
                .cloned()
                .unwrap_or_default();
            Ok((key, zip))
        }
        v => Err(format!("Unsupported extension format (CRX{v})")),
    }
}

/// Chrome version to report to the Web Store, from the installed WebView2 engine.
fn prod_version() -> String {
    #[cfg(windows)]
    if let Ok(v) = wry::webview_version() {
        let v: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if v.contains('.') {
            return v;
        }
    }
    "140.0.0.0".into()
}

pub fn download_crx(id: &str) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&os=win&arch=x64&os_arch=x86_64&nacl_arch=x86-64&prod=chromecrx&prodchannel=&prodversion={}&lang=en-US&acceptformat=crx2,crx3&x=id%3D{id}%26installsource%3Dondemand%26uc",
        prod_version()
    );
    let resp = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .redirects(8)
        .build()
        .get(&url)
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) | ureq::Error::Status(204, _) => {
                "That extension isn't available from the Chrome Web Store".to_string()
            }
            ureq::Error::Status(c, _) => format!("The Chrome Web Store answered with error {c}"),
            ureq::Error::Transport(_) => "Couldn't reach the Chrome Web Store. Check your connection.".to_string(),
        })?;
    let mut buf = vec![];
    resp.into_reader()
        .take(MAX_CRX)
        .read_to_end(&mut buf)
        .map_err(|e| format!("Download failed: {e}"))?;
    if buf.is_empty() {
        return Err("That extension isn't available from the Chrome Web Store".into());
    }
    Ok(buf)
}

/// Unzip `zip` into `dest` (which must not exist yet), refusing paths that escape it.
fn unzip(zip: &[u8], dest: &Path) -> Result<(), String> {
    let mut ar = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| format!("The extension file is damaged ({e})"))?;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..ar.len() {
        let mut f = ar.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        let out = dest.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// `__MSG_name__` strings resolved from the extension's `_locales`.
fn localize(dir: &Path, manifest: &Value, s: &str) -> String {
    let Some(key) = s.strip_prefix("__MSG_").and_then(|k| k.strip_suffix("__")) else {
        return s.to_string();
    };
    let key = key.to_lowercase();
    let def = manifest["default_locale"].as_str().unwrap_or("en");
    let ui = std::env::var("LANG").unwrap_or_default();
    let mut locales = vec!["en_US".to_string(), "en".to_string(), def.to_string()];
    if !ui.is_empty() {
        locales.insert(0, ui.split('.').next().unwrap_or("").to_string());
    }
    for loc in locales {
        let Ok(t) = fs::read_to_string(dir.join("_locales").join(&loc).join("messages.json")) else {
            continue;
        };
        let Ok(m) = serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')) else {
            continue;
        };
        if let Some(obj) = m.as_object() {
            if let Some((_, v)) = obj.iter().find(|(k, _)| k.to_lowercase() == key) {
                if let Some(msg) = v["message"].as_str() {
                    return msg.to_string();
                }
            }
        }
    }
    s.to_string()
}

/// The largest icon up to 128px from an `icons`-style map or a single path.
fn best_icon(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    let obj = v.as_object()?;
    let mut sizes: Vec<(u32, &str)> = obj.iter().filter_map(|(k, p)| Some((k.parse().ok()?, p.as_str()?))).collect();
    sizes.sort_by_key(|(s, _)| *s);
    sizes
        .iter()
        .rev()
        .find(|(s, _)| *s <= 128)
        .or(sizes.last())
        .map(|(_, p)| p.to_string())
}

/// Chrome's install warnings for a manifest's permissions.
fn warnings(m: &Value) -> Vec<String> {
    let mut perms: Vec<String> = vec![];
    for k in ["permissions", "host_permissions"] {
        if let Some(a) = m[k].as_array() {
            perms.extend(a.iter().filter_map(|x| x.as_str().map(String::from)));
        }
    }
    if let Some(cs) = m["content_scripts"].as_array() {
        for c in cs {
            if let Some(a) = c["matches"].as_array() {
                perms.extend(a.iter().filter_map(|x| x.as_str().map(String::from)));
            }
        }
    }
    let all_sites = perms
        .iter()
        .any(|p| p == "<all_urls>" || p.starts_with("*://*/") || p.starts_with("http://*/") || p.starts_with("https://*/"));
    let mut w = vec![];
    if all_sites {
        w.push("Read and change all your data on all websites".to_string());
    } else {
        let mut hosts: Vec<String> = perms
            .iter()
            .filter(|p| p.contains("://"))
            .filter_map(|p| {
                p.split("://")
                    .nth(1)?
                    .split('/')
                    .next()
                    .map(|h| h.trim_start_matches("*.").to_string())
            })
            .collect();
        hosts.sort();
        hosts.dedup();
        match hosts.len() {
            0 => {}
            1..=3 => w.push(format!("Read and change your data on {}", hosts.join(", "))),
            n => w.push(format!("Read and change your data on {n} websites")),
        }
    }
    let has = |p: &str| perms.iter().any(|x| x == p);
    for (p, text) in [
        ("history", "Read and change your browsing history"),
        ("tabs", "Read your browsing history"),
        ("bookmarks", "Read and change your bookmarks"),
        ("downloads", "Manage your downloads"),
        ("clipboardRead", "Read data you copy and paste"),
        ("clipboardWrite", "Modify data you copy and paste"),
        ("geolocation", "Detect your physical location"),
        ("management", "Manage your apps, extensions, and themes"),
        ("nativeMessaging", "Communicate with cooperating native applications"),
        ("notifications", "Display notifications"),
        ("privacy", "Change your privacy-related settings"),
        ("proxy", "Read and change all your data on all websites"),
        ("debugger", "Access the page debugger backend"),
        ("cookies", "Read and change cookies"),
        ("webNavigation", "Read your browsing history"),
        ("declarativeNetRequest", "Block content on any page"),
    ] {
        if has(p) && !w.iter().any(|x| x == text) {
            w.push(text.to_string());
        }
    }
    w
}

/// Runs first in the extension's background context. WebView2 shows every Wisp page as a tab in
/// its own window of type "popup" (and extension popups are tabs too), so this makes the tab and
/// window model look the way extensions expect from Chrome: the active tab of the last focused
/// window is the page the user is on, windows are "normal", Wisp's own pages and the popup aren't
/// tabs, and messages from the popup don't carry a tab.
const COMPAT_JS: &str = r#"// Added by Wisp: Chrome compatibility for Wisp's engine (WebView2).
(()=>{const C=globalThis.chrome;if(globalThis.__wispCompat||!C||!C.tabs||!C.windows||!C.runtime)return;globalThis.__wispCompat=1;
const POPUP=__POPUP__,BASE='chrome-extension://'+C.runtime.id+'/';
const internal=u=>!!u&&(/^https?:\/\/wisp\.localhost\//.test(u)||(!!POPUP&&u.split(/[?#]/)[0]===BASE+POPUP));
const tq=C.tabs.query.bind(C.tabs),le=()=>void C.runtime.lastError;
const q=i=>new Promise(r=>{try{tq(i,x=>{le();r(x||[])})}catch(_){r([])}});
let real=null;
const note=async w=>{if(w==null||w<0)return;const t=(await q({windowId:w}))[0];if(t&&!internal(t.url||t.pendingUrl))real=w};
try{C.windows.onFocusChanged.addListener(note)}catch(_){}
try{C.windows.getLastFocused(w=>{le();if(w)note(w.id)})}catch(_){}
const cbOr=(p,a)=>{const cb=a.find(x=>typeof x==='function');if(cb){p.then(cb,()=>cb());return}return p};
C.tabs.query=function(info,cb){const i=Object.assign({},info||{}),focused=i.lastFocusedWindow||i.currentWindow;
 if(i.windowType==='normal'||i.windowType==='popup')delete i.windowType;
 return cbOr((async()=>{if(focused&&real!=null){const j=Object.assign({},i,{windowId:real});delete j.lastFocusedWindow;delete j.currentWindow;const r=await q(j);if(r.length)return r}
  return (await q(i)).filter(t=>!internal(t.url||t.pendingUrl))})(),[cb])};
const fixW=w=>{if(w&&w.type==='popup')w.type='normal';return w};
for(const n of ['get','getCurrent','getLastFocused','getAll']){const o=C.windows[n];if(!o)continue;const f=o.bind(C.windows);
 C.windows[n]=function(...a){const args=a.filter(x=>typeof x!=='function');
  return cbOr((async()=>{if((n==='getCurrent'||n==='getLastFocused')&&real!=null){try{return fixW(await f(real,...args))}catch(_){}}
   const r=await f(...args);return Array.isArray(r)?r.map(fixW):fixW(r)})(),a)}}
const strip=s=>{if(s&&s.tab&&internal(s.url)){s=Object.assign({},s);delete s.tab}return s};
const wrap=(ev,mk)=>{if(!ev||!ev.addListener)return;const add=ev.addListener.bind(ev),rm=ev.removeListener.bind(ev),has=ev.hasListener.bind(ev),m=new WeakMap();
 ev.addListener=f=>{const g=mk(f);m.set(f,g);add(g)};ev.removeListener=f=>rm(m.get(f)||f);ev.hasListener=f=>has(m.get(f)||f)};
wrap(C.runtime.onMessage,f=>(msg,s,r)=>f(msg,strip(s),r));
wrap(C.runtime.onConnect,f=>port=>{try{if(port.sender&&port.sender.tab&&internal(port.sender.url))Object.defineProperty(port,'sender',{value:strip(port.sender)})}catch(_){}return f(port)});
const guard=(o,n)=>{if(!o||!o[n])return;const f=o[n].bind(o);o[n]=function(d,...a){const id=d&&d.target&&d.target.tabId;
 return cbOr((async()=>{if(id!=null){const t=await new Promise(r=>C.tabs.get(id,x=>{le();r(x)}));if(t&&internal(t.url))throw new Error('Cannot access contents of the page.')}return f(d)})(),a)}};
guard(C.scripting,'executeScript');guard(C.scripting,'insertCSS');
})();
"#;

/// Wire `COMPAT_JS` into the extension's background (service worker or page) and keep content
/// scripts off Wisp's own pages, as Chrome does for its chrome:// pages.
fn add_compat(m: &mut Value, dir: &Path, popup: &str) {
    let _ = fs::write(
        dir.join("wisp-compat.js"),
        COMPAT_JS.replace("__POPUP__", &Value::String(popup.to_string()).to_string()),
    );
    if let Some(cs) = m["content_scripts"].as_array_mut() {
        for c in cs {
            let ex = c["exclude_matches"].as_array().cloned().unwrap_or_default();
            if !ex.iter().any(|x| x == "*://wisp.localhost/*") {
                let mut ex = ex;
                ex.push(Value::String("*://wisp.localhost/*".into()));
                c["exclude_matches"] = Value::Array(ex);
            }
        }
    }
    let bg = &mut m["background"];
    if let Some(sw) = bg["service_worker"].as_str().map(|x| x.trim_start_matches('/').to_string()) {
        if sw == "wisp-sw.js" {
            return;
        }
        let module = bg["type"].as_str() == Some("module");
        let lit = Value::String(format!("/{sw}")).to_string();
        let shim = if module {
            format!("import '/wisp-compat.js';\nimport {lit};\n")
        } else {
            format!("importScripts('/wisp-compat.js', {lit});\n")
        };
        if fs::write(dir.join("wisp-sw.js"), shim).is_ok() {
            bg["service_worker"] = Value::String("wisp-sw.js".into());
        }
    } else if let Some(list) = bg["scripts"].as_array_mut() {
        if !list.iter().any(|x| x == "wisp-compat.js") {
            list.insert(0, Value::String("wisp-compat.js".into()));
        }
    } else if let Some(page) = bg["page"].as_str().map(|x| x.trim_start_matches('/').to_string()) {
        let path = dir.join(&page);
        if let Ok(html) = fs::read_to_string(&path) {
            if !html.contains("wisp-compat.js") {
                let tag = "<script src=\"/wisp-compat.js\"></script>";
                let out = match html.to_ascii_lowercase().find("<head>") {
                    Some(i) => format!("{}{tag}{}", &html[..i + 6], &html[i + 6..]),
                    None => format!("{tag}{html}"),
                };
                let _ = fs::write(path, out);
            }
        }
    }
}

/// Download `id` and unpack it into a staging folder. Returns the extension's details and the
/// staging folder, which `commit` moves into place once the user agrees.
pub fn fetch(id: &str) -> Result<(Ext, PathBuf), String> {
    let data = download_crx(id)?;
    let (key, zip) = parse_crx(&data)?;
    let stage = root().join(format!(".stage-{id}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&stage);
    unzip(zip, &stage)?;
    let mpath = stage.join("manifest.json");
    let text = fs::read_to_string(&mpath).map_err(|_| "The extension has no manifest".to_string())?;
    let mut m: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("Bad manifest: {e}"))?;
    // Keep the Web Store ID (see the module docs).
    if !key.is_empty() && m.get("key").is_none() {
        m["key"] = Value::String(base64(&key));
    }
    if let Some(k) = (!key.is_empty()).then(|| id_from_key(&key)) {
        if k != id {
            let _ = fs::remove_dir_all(&stage);
            return Err("The downloaded extension doesn't match the one requested".into());
        }
    }
    if m["theme"].is_object() && m["background"].is_null() && m["content_scripts"].is_null() {
        let _ = fs::remove_dir_all(&stage);
        return Err("Chrome themes can't be installed. Wisp has its own themes in Settings → Appearance.".into());
    }
    let s = |v: &Value| v.as_str().unwrap_or("").to_string();
    let action = if m["action"].is_object() {
        &m["action"]
    } else if m["browser_action"].is_object() {
        &m["browser_action"]
    } else {
        &m["page_action"]
    };
    let icon = best_icon(&m["icons"])
        .or_else(|| best_icon(&action["default_icon"]))
        .unwrap_or_default();
    let options = if m["options_ui"]["page"].is_string() {
        s(&m["options_ui"]["page"])
    } else {
        s(&m["options_page"])
    };
    let popup = s(&action["default_popup"]).trim_start_matches('/').to_string();
    add_compat(&mut m, &stage, &popup);
    fs::write(&mpath, serde_json::to_vec_pretty(&m).unwrap_or_default()).map_err(|e| e.to_string())?;
    let ext = Ext {
        id: id.to_string(),
        name: localize(&stage, &m, &s(&m["name"])),
        version: s(&m["version"]),
        description: localize(&stage, &m, &s(&m["description"])),
        enabled: true,
        icon: icon.trim_start_matches('/').to_string(),
        popup,
        options: options.trim_start_matches('/').to_string(),
        warnings: warnings(&m),
        stamp: crate::library::now(),
    };
    Ok((ext, stage))
}

/// Move a staged download into the extension's folder (replacing an older version).
pub fn commit(stage: &Path, id: &str) -> Result<(), String> {
    let dest = dir_of(id);
    if dest.exists() {
        let old = root().join(format!(".old-{id}-{}", crate::library::now()));
        fs::rename(&dest, &old).map_err(|e| format!("Couldn't replace the old version: {e}"))?;
        let _ = fs::remove_dir_all(&old);
    }
    fs::rename(stage, &dest).map_err(|e| e.to_string())
}

/// Delete an extension's files (and any leftovers from interrupted installs).
pub fn delete_files(id: &str) {
    let _ = fs::remove_dir_all(dir_of(id));
    cleanup();
}

/// Keep each installed extension's compatibility script current with this version of Wisp.
pub fn refresh_compat(list: &[Ext]) {
    for e in list {
        let f = dir_of(&e.id).join("wisp-compat.js");
        if f.is_file() {
            let _ = fs::write(f, COMPAT_JS.replace("__POPUP__", &Value::String(e.popup.clone()).to_string()));
        }
    }
}

pub fn cleanup() {
    if let Ok(rd) = fs::read_dir(root()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with(".old-") || (n.starts_with(".stage-") && !n.ends_with(&format!("-{}", std::process::id()))) {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
}

/// A small icon as a data: URL (for the install dialog, before the extension is in place).
pub fn icon_data_url(dir: &Path, rel: &str) -> String {
    if rel.is_empty() || rel.contains("..") {
        return String::new();
    }
    match fs::read(dir.join(rel)) {
        Ok(b) if b.len() < 512 * 1024 => format!("data:{};base64,{}", crate::mime(rel), base64(&b)),
        _ => String::new(),
    }
}

/// Bring a WebView2 profile in line with `want` (id, folder, enabled, stamp): add what is missing,
/// remove what was uninstalled, enable/disable, and reload extensions whose stamp changed
/// (`seen` holds the stamps this profile already has loaded).
#[cfg(windows)]
pub fn sync_profile(
    view: &wry::WebView,
    want: Vec<(String, PathBuf, bool, u64)>,
    seen: std::collections::HashMap<String, u64>,
    on_err: Box<dyn Fn(String)>,
) -> bool {
    use std::rc::Rc;
    use webview2_com::{
        take_pwstr, BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler, Microsoft::Web::WebView2::Win32::*,
        ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler,
    };
    use windows::core::{Interface, HSTRING, PWSTR};
    use windows::Win32::Foundation::BOOL;
    use wry::WebViewExtWindows;

    // SAFETY: COM calls on a live WebView2 owned by this (UI) thread; callbacks run on it too.
    unsafe {
        let Ok(wv) = view.controller().CoreWebView2() else { return false };
        let Ok(p) = wv.cast::<ICoreWebView2_13>().and_then(|w| w.Profile()) else {
            return false;
        };
        let Ok(profile) = p.cast::<ICoreWebView2Profile7>() else {
            return false;
        };
        let on_err: Rc<dyn Fn(String)> = Rc::from(on_err);
        let prof2 = profile.clone();
        let add = move |prof: &ICoreWebView2Profile7, path: &Path, enabled: bool, err: Rc<dyn Fn(String)>| {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let _ = prof.AddBrowserExtension(
                &HSTRING::from(path.as_os_str()),
                &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |hr, ext| {
                    match ext {
                        Some(e) if hr.is_ok() => {
                            if !enabled {
                                let _ = e.Enable(BOOL(0), &BrowserExtensionEnableCompletedHandler::create(Box::new(|_| Ok(()))));
                            }
                        }
                        _ => err(name),
                    }
                    Ok(())
                })),
            );
        };
        let add = Rc::new(add);
        let r = profile.GetBrowserExtensions(&ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |hr, list| {
            if hr.is_err() {
                return Ok(());
            }
            let mut have: Vec<(String, ICoreWebView2BrowserExtension, bool)> = vec![];
            if let Some(list) = list {
                let mut n = 0u32;
                let _ = list.Count(&mut n);
                for i in 0..n {
                    if let Ok(e) = list.GetValueAtIndex(i) {
                        let mut id = PWSTR::null();
                        let mut on = BOOL(0);
                        let _ = e.Id(&mut id);
                        let _ = e.IsEnabled(&mut on);
                        have.push((take_pwstr(id), e, on.as_bool()));
                    }
                }
            }
            for (id, ext, on) in &have {
                match want.iter().find(|w| &w.0 == id) {
                    None => {
                        let _ = ext.Remove(&BrowserExtensionRemoveCompletedHandler::create(Box::new(|_| Ok(()))));
                    }
                    Some((_, path, enabled, stamp)) => {
                        if seen.get(id).is_some_and(|s| s != stamp) {
                            // Updated on disk: unload and load the new files.
                            let (prof, path, enabled, err, add) = (prof2.clone(), path.clone(), *enabled, on_err.clone(), add.clone());
                            let _ = ext.Remove(&BrowserExtensionRemoveCompletedHandler::create(Box::new(move |_| {
                                add(&prof, &path, enabled, err);
                                Ok(())
                            })));
                        } else if on != enabled {
                            let _ = ext.Enable(
                                BOOL(*enabled as i32),
                                &BrowserExtensionEnableCompletedHandler::create(Box::new(|_| Ok(()))),
                            );
                        }
                    }
                }
            }
            for (id, path, enabled, _) in &want {
                if !have.iter().any(|h| &h.0 == id) {
                    add(&prof2, path, *enabled, on_err.clone());
                }
            }
            Ok(())
        })));
        r.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids() {
        assert!(valid_id("eimadpbcbfnmbkopoojfekhnkhdbieeh"));
        assert!(!valid_id("eimadpbcbfnmbkopoojfekhnkhdbiee"));
        assert_eq!(
            id_from_input("https://chromewebstore.google.com/detail/dark-reader/eimadpbcbfnmbkopoojfekhnkhdbieeh?hl=en").as_deref(),
            Some("eimadpbcbfnmbkopoojfekhnkhdbieeh")
        );
        assert_eq!(
            id_from_input("https://chrome.google.com/webstore/detail/eimadpbcbfnmbkopoojfekhnkhdbieeh").as_deref(),
            Some("eimadpbcbfnmbkopoojfekhnkhdbieeh")
        );
        assert_eq!(id_from_input("hello"), None);
    }
    #[test]
    fn b64() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
    #[test]
    fn crx3_header() {
        // CrxFileHeader { sha256_with_rsa: [{ public_key: "KEY" }], signed_header_data: { crx_id } }
        let key = b"KEY".to_vec();
        let h = Sha256::digest(&key);
        let mut proof = vec![0x0a, key.len() as u8];
        proof.extend(&key);
        let mut sd = vec![0x0a, 16];
        sd.extend(&h[..16]);
        let mut header = vec![0x12, proof.len() as u8];
        header.extend(&proof);
        header.extend([0x82, 0xf1, 0x04, sd.len() as u8]); // field 10000, wire type 2
        header.extend(&sd);
        let mut crx = b"Cr24".to_vec();
        crx.extend(3u32.to_le_bytes());
        crx.extend((header.len() as u32).to_le_bytes());
        crx.extend(&header);
        crx.extend(b"PKzip");
        let (k, zip) = parse_crx(&crx).unwrap();
        assert_eq!(k, key);
        assert_eq!(zip, b"PKzip");
    }
}
