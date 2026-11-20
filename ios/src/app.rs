//! UIKit / WebKit side of Wisp for iOS.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::NonNull;

use block2::{DynBlock, RcBlock};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSHomeDirectory, NSString, NSURLRequest, NSURL};
use objc2_ui_kit::{
    UIAction, UIApplication, UIApplicationDelegate, UIBlurEffect, UIBlurEffectStyle, UIButton, UIButtonConfiguration, UIColor,
    UIControlEvents, UIEdgeInsets, UIImage, UIMenu, UIMenuElement, UIMenuElementAttributes, UIMenuElementState, UIMenuOptions,
    UIPasteboard, UIScreen, UIView, UIViewAnimationOptions, UIViewController, UIVisualEffectView, UIWindow,
};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKScriptMessage, WKScriptMessageHandler, WKUIDelegate, WKUserContentController, WKUserScript, WKUserScriptInjectionTime,
    WKWebViewConfiguration, WKWebsiteDataStore, WKWindowFeatures,
};

use crate::core::{self, Changed, Session};
use crate::omni;
use serde_json::json;

use crate::glass::{spring, Glass, Spec};
use crate::wk::WKWebView;

const CHROME_HTML: &str = include_str!("../ui/chrome.html");
const NEWTAB_HTML: &str = include_str!("../ui/newtab.html");
/// Height of the bar, not counting the home-indicator / status-bar area.
const BAR_H: f64 = 54.0;
/// In the Glass style pages run under the floating bar; this much extra scroll room keeps their
/// content clear of it.
const PAGE_INSET: f64 = 52.0;
/// Height the bookmarks bar adds next to the address bar.
const BM_H: f64 = 44.0;
const DESKTOP_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

/// Injected into every page (main frame): reports title, address and icon to Wisp, including
/// changes made by single-page apps. Debounced, and only watches <head>, so it costs nothing idle.
const PAGE_JS: &str = r#"(()=>{if(window.__wisp)return;window.__wisp=1;let t=0;
const icon=()=>{try{const l=document.querySelector('link[rel~="icon"],link[rel="apple-touch-icon"]');return l?new URL(l.getAttribute('href'),location.href).href:(location.protocol.startsWith('http')?location.origin+'/favicon.ico':'')}catch(e){return''}};
const post=()=>{clearTimeout(t);t=setTimeout(()=>{try{webkit.messageHandlers.wispPage.postMessage(JSON.stringify({t:'meta',title:document.title,url:location.href,icon:icon()}))}catch(e){}},150)};
['pushState','replaceState'].forEach(k=>{const o=history[k];history[k]=function(){const r=o.apply(this,arguments);post();return r}});
addEventListener('popstate',post);addEventListener('hashchange',post);
if(document.head)new MutationObserver(post).observe(document.head,{subtree:true,childList:true,characterData:true});post()})()"#;

fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
    CGRect {
        origin: CGPoint { x, y },
        size: CGSize { width: w, height: h },
    }
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

struct Browser {
    mtm: MainThreadMarker,
    window: Retained<UIWindow>,
    root: Retained<UIView>,
    effect: Retained<UIVisualEffectView>,
    /// Full-screen holder of the interface that only takes touches inside the bar (or everywhere
    /// while a sheet is open), so the interface never has to resize.
    pass: Retained<PassView>,
    chrome: Retained<WKWebView>,
    /// Shared by every page view (scripts, message handler, blocker rules).
    page_ucc: Retained<WKUserContentController>,
    page_cfg: Retained<WKWebViewConfiguration>,
    delegate: Retained<Delegate>,
    views: HashMap<u64, Retained<WKWebView>>,
    s: Session,
    /// The interface covers the whole screen (search, tab grid, menu) instead of just the bar.
    full: bool,
    rules: Option<Retained<WKContentRuleList>>,
    path: String,
    last_push: String,
    /// The tab whose page is on screen (to cross-fade when it changes).
    shown: u64,
    /// Native glass: Liquid Glass buttons and shapes (iOS 26), or blur shapes before that.
    glass: Glass,
    /// Where the bar takes touches, as measured by the interface (the native glass buttons take
    /// their own touches).
    hot_bar: Vec<CGRect>,
    /// What the native menu was last built from.
    menu_sig: String,
    /// The open sheet wants the full-screen blur behind it (the menu uses a glass panel instead).
    blur: bool,
}

thread_local! {
    static B: RefCell<Option<Browser>> = const { RefCell::new(None) };
}

/// Run `f` on the browser, unless it is already borrowed (a UIKit callback arriving while Wisp is
/// itself calling into UIKit); such re-entrant calls are skipped and the caller lays out again.
fn with<R>(f: impl FnOnce(&mut Browser) -> R) -> Option<R> {
    B.with(|b| b.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

impl Browser {
    fn top_bar(&self) -> bool {
        self.s.settings.bar == "top"
    }

    /// The bar, including the status-bar (top) or home-indicator (bottom) area.
    fn bar_rect(&self) -> CGRect {
        let b = self.root.bounds();
        let i = self.root.safeAreaInsets();
        let bm = if self.s.bmbar_shown() { BM_H } else { 0.0 };
        if self.top_bar() {
            rect(0.0, 0.0, b.size.width, i.top + BAR_H + bm)
        } else {
            let h = BAR_H + bm + i.bottom;
            rect(0.0, b.size.height - h, b.size.width, h)
        }
    }

    fn page_frame(&self) -> CGRect {
        let b = self.root.bounds();
        if self.s.settings.glass {
            return b;
        }
        let bar = self.bar_rect().size.height;
        let h = (b.size.height - bar).max(0.0);
        rect(0.0, if self.top_bar() { bar } else { 0.0 }, b.size.width, h)
    }

    fn layout(&mut self) {
        let b = self.root.bounds();
        self.pass.setFrame(b);
        self.chrome.setFrame(b);
        self.set_hot();
        self.effect.setFrame(b);
        self.glass.layout(b);
        let pf = self.page_frame();
        for v in self.views.values() {
            v.setFrame(pf);
            self.inset_page(v);
        }
    }

    /// Extra scroll room so page content can scroll clear of the floating glass bar.
    fn inset_page(&self, v: &WKWebView) {
        let bm = if self.s.bmbar_shown() { BM_H } else { 0.0 };
        let extra = if self.s.settings.glass { PAGE_INSET + bm } else { 0.0 };
        let top = self.top_bar();
        let i = UIEdgeInsets {
            top: if top { extra } else { 0.0 },
            left: 0.0,
            bottom: if top { 0.0 } else { extra },
            right: 0.0,
        };
        let sv = unsafe { v.scrollView() };
        sv.setContentInset(i);
        sv.setVerticalScrollIndicatorInsets(i);
    }

    /// Where the interface takes touches: everything while a sheet is open, otherwise the bar.
    fn set_hot(&self) {
        let hot = if self.full {
            vec![self.root.bounds()]
        } else if self.hot_bar.is_empty() {
            vec![self.bar_rect()]
        } else {
            self.hot_bar.clone()
        };
        *self.pass.ivars().hot.borrow_mut() = hot;
    }

    /// Native Liquid Glass buttons are in use (iOS 26, Glass style).
    fn native(&self) -> bool {
        crate::glass::liquid_glass() && self.s.settings.glass
    }

    fn chrome_js(&self, js: &str) {
        unsafe { self.chrome.evaluateJavaScript_completionHandler(&ns(js), None) };
    }

    /// Keep the native glass buttons in step: back enabled, tab count, the menu's items.
    fn update_buttons(&mut self, back: bool, fwd: bool) {
        if let Some(b) = self.glass.button("back") {
            b.setEnabled(back);
        }
        if let Some(b) = self.glass.button("tabs") {
            let n = self.s.tabs.len();
            let name = if n <= 50 {
                format!("{n}.square")
            } else {
                "square.on.square".into()
            };
            if let Some(c) = b.configuration() {
                c.setImage(sym(&name).as_deref());
                b.setConfiguration(Some(&c));
            }
        }
        let Some(mb) = self.glass.button("menu") else { return };
        let a = self.s.tab(self.s.active).cloned().unwrap_or_default();
        let marked = self.s.bookmarks.iter().any(|b| b.url == a.url);
        let web = a.url.starts_with("http");
        let sig = format!("{back}{fwd}{marked}{}{}{web}", a.desktop, self.s.settings.blocker);
        if sig == self.menu_sig && mb.menu().is_some() {
            return;
        }
        self.menu_sig = sig;
        let mtm = self.mtm;
        let item = |title: &str, icon: &str, msg: serde_json::Value, enabled: bool, on: bool| -> Retained<UIMenuElement> {
            let blk = RcBlock::new(move |_a: NonNull<UIAction>| {
                let m = msg.clone();
                with(|b| b.on_chrome(&m));
            });
            let act = unsafe {
                UIAction::actionWithTitle_image_identifier_handler(&ns(title), sym(icon).as_deref(), None, RcBlock::as_ptr(&blk), mtm)
            };
            if !enabled {
                act.setAttributes(UIMenuElementAttributes::Disabled);
            }
            if on {
                act.setState(UIMenuElementState::On);
            }
            Retained::into_super(act)
        };
        let nav = [
            item("Back", "chevron.backward", json!({"t":"back"}), back, false),
            item("Forward", "chevron.forward", json!({"t":"fwd"}), fwd, false),
            item("Reload", "arrow.clockwise", json!({"t":"reload"}), web, false),
        ];
        let page = [
            item(
                if marked { "Remove Bookmark" } else { "Add Bookmark" },
                if marked { "star.slash" } else { "star" },
                json!({"t":"bookmark"}),
                web,
                false,
            ),
            item("Copy Link", "doc.on.doc", json!({"t":"copy"}), web, false),
            item(
                if a.desktop {
                    "Request Mobile Website"
                } else {
                    "Request Desktop Website"
                },
                if a.desktop { "iphone" } else { "desktopcomputer" },
                json!({"t":"desktop"}),
                web,
                false,
            ),
        ];
        let app = [
            item("New Tab", "plus.square.on.square", json!({"t":"new"}), true, false),
            item(
                "Block Ads & Trackers",
                "shield.lefthalf.filled",
                json!({"t":"set","k":"blocker","v":!self.s.settings.blocker}),
                true,
                self.s.settings.blocker,
            ),
            item("Settings", "gearshape", json!({"t":"ui","js":"open('set')"}), true, false),
        ];
        let group = |els: &[Retained<UIMenuElement>]| -> Retained<UIMenuElement> {
            let arr = NSArray::from_retained_slice(els);
            let m = UIMenu::menuWithTitle_image_identifier_options_children(&ns(""), None, None, UIMenuOptions::DisplayInline, &arr, mtm);
            Retained::into_super(m)
        };
        let all = NSArray::from_retained_slice(&[group(&nav), group(&page), group(&app)]);
        let menu = UIMenu::menuWithTitle_children(&ns(""), &all, mtm);
        mb.setMenu(Some(&menu));
    }

    /// Place the glass shapes the interface asked for (see glass.rs).
    fn set_shapes(&mut self, specs: Vec<Spec>, hot: Vec<CGRect>) {
        self.hot_bar = hot;
        self.set_hot();
        let mtm = self.mtm;
        self.glass.sync(&specs, &move |kind| make_button(mtm, kind));
        self.menu_sig.clear();
        let (back, fwd) = match self.views.get(&self.s.active) {
            Some(v) => unsafe { (v.canGoBack(), v.canGoForward()) },
            None => (false, false),
        };
        self.update_buttons(back, fwd);
    }

    /// Open or close a sheet. Sheets that want it get a full-screen blur, which fades in and out.
    fn set_full(&mut self, full: bool, blur: bool) {
        self.full = full;
        self.blur = blur;
        self.set_hot();
        let target = if full && blur && self.s.settings.glass { 1.0 } else { 0.0 };
        let effect = self.effect.clone();
        spring(self.mtm, if full { 0.3 } else { 0.22 }, move || effect.setAlpha(target), None);
    }

    fn apply_theme(&self) {
        let style: isize = match self.s.settings.theme.as_str() {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        };
        let _: () = unsafe { msg_send![&*self.window, setOverrideUserInterfaceStyle: style] };
    }

    /// Restyle New Tab pages that are open.
    fn restyle_newtabs(&self) {
        let js = format!("window.wispCfg&&wispCfg({})", self.s.settings.newtab());
        for t in self.s.tabs.iter().filter(|t| t.url.is_empty()) {
            if let Some(v) = self.views.get(&t.id) {
                unsafe { v.evaluateJavaScript_completionHandler(&ns(&js), None) };
            }
        }
    }

    fn push(&mut self) {
        let (back, fwd) = match self.views.get(&self.s.active) {
            Some(v) => unsafe { (v.canGoBack(), v.canGoForward()) },
            None => (false, false),
        };
        self.update_buttons(back, fwd);
        let mut v = self.s.view_state(back, fwd);
        v["native"] = json!(self.native());
        let st = v.to_string();
        if st == self.last_push {
            return;
        }
        self.last_push = st.clone();
        let js = format!("window.wisp&&wisp.state({st})");
        unsafe { self.chrome.evaluateJavaScript_completionHandler(&ns(&js), None) };
    }

    fn save(&self) {
        let _ = std::fs::write(&self.path, self.s.to_json());
    }

    fn tab_of(&self, wv: &WKWebView) -> Option<u64> {
        self.views.iter().find(|(_, v)| std::ptr::eq(&***v, wv)).map(|(id, _)| *id)
    }

    fn make_view(&mut self, id: u64) {
        let wv = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(self.mtm), self.page_frame(), &self.page_cfg) };
        unsafe {
            wv.setNavigationDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
            wv.setUIDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
            wv.setAllowsBackForwardNavigationGestures(true);
        }
        wv.setHidden(true);
        self.root.insertSubview_belowSubview(&wv, &self.effect);
        self.inset_page(&wv);
        let (url, desktop) = self.s.tab(id).map(|t| (t.url.clone(), t.desktop)).unwrap_or_default();
        if desktop {
            unsafe { wv.setCustomUserAgent(Some(&ns(DESKTOP_UA))) };
        }
        load(&wv, &url, &self.s.settings);
        self.views.insert(id, wv);
    }

    /// Make sure the active tab has a page, unload the oldest background pages, show the active one.
    fn show(&mut self) {
        let id = self.s.active;
        self.s.select(id);
        if !self.views.contains_key(&id) {
            self.make_view(id);
        }
        let live: Vec<u64> = self.views.keys().copied().collect();
        for gone in self.s.to_unload(&live) {
            if let Some(v) = self.views.remove(&gone) {
                v.removeFromSuperview();
            }
        }
        let changed = self.shown != id && self.shown != 0;
        self.shown = id;
        match (changed, self.views.get(&id).cloned()) {
            // Cross-fade: the new page fades in on top, the old one hides once it's covered.
            (true, Some(v)) => {
                self.root.insertSubview_belowSubview(&v, &self.effect);
                v.setAlpha(0.0);
                v.setHidden(false);
                let fade = v.clone();
                let anim = RcBlock::new(move || fade.setAlpha(1.0));
                let done = RcBlock::new(|_finished: objc2::runtime::Bool| {
                    with(|b| b.hide_others());
                });
                UIView::animateWithDuration_delay_options_animations_completion(
                    0.18,
                    0.0,
                    UIViewAnimationOptions::AllowUserInteraction | UIViewAnimationOptions::CurveEaseOut,
                    &anim,
                    Some(&done),
                    self.mtm,
                );
            }
            _ => self.hide_others(),
        }
        self.layout();
        self.push();
        self.save();
    }

    fn hide_others(&self) {
        for (tid, v) in &self.views {
            v.setHidden(*tid != self.shown);
            if *tid == self.shown {
                v.setAlpha(1.0);
            }
        }
    }

    fn navigate(&mut self, url: String) {
        let id = self.s.active;
        if let Some(t) = self.s.tab_mut(id) {
            t.url = url.clone();
        }
        match self.views.get(&id) {
            Some(v) => load(v, &url, &self.s.settings),
            None => self.make_view(id),
        }
        self.show();
    }

    fn unload_background(&mut self) {
        let active = self.s.active;
        let gone: Vec<u64> = self.views.keys().copied().filter(|id| *id != active).collect();
        for id in gone {
            if let Some(v) = self.views.remove(&id) {
                v.removeFromSuperview();
            }
        }
    }

    fn apply_rules(&self) {
        unsafe {
            self.page_ucc.removeAllContentRuleLists();
            if let (true, Some(r)) = (self.s.settings.blocker, &self.rules) {
                self.page_ucc.addContentRuleList(r);
            }
        }
    }

    /// Messages from the interface (bottom bar, tab grid, menus).
    fn on_chrome(&mut self, m: &serde_json::Value) {
        let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
        let id = m["id"].as_u64().unwrap_or(0);
        let active = self.views.get(&self.s.active).cloned();
        match s("t").as_str() {
            "ready" => {
                self.last_push.clear();
                self.push();
            }
            "go" => {
                let q = s("q");
                if !q.trim().is_empty() {
                    self.navigate(omni::resolve(&q, &self.s.settings.engine));
                }
            }
            "back" => {
                if let Some(v) = active {
                    unsafe { v.goBack() };
                }
            }
            "fwd" => {
                if let Some(v) = active {
                    unsafe { v.goForward() };
                }
            }
            "reload" => {
                if let Some(v) = active {
                    unsafe { v.reload() };
                }
            }
            "stop" => {
                if let Some(v) = active {
                    unsafe { v.stopLoading() };
                }
            }
            "new" => {
                let n = self.s.add_tab("");
                self.s.select(n);
                self.show();
            }
            "select" => {
                self.s.select(id);
                self.show();
            }
            "close" => {
                if let Some(v) = self.views.remove(&id) {
                    v.removeFromSuperview();
                }
                self.s.close(id);
                self.show();
            }
            "closeAll" => {
                for (_, v) in self.views.drain() {
                    v.removeFromSuperview();
                }
                let keep = self.s.settings.clone();
                self.s = Session {
                    settings: keep,
                    ..Session::default()
                };
                self.show();
            }
            "mode" => self.set_full(m["full"].as_bool().unwrap_or(false), m["blur"].as_bool().unwrap_or(true)),
            "shapes" => {
                let f = |v: &serde_json::Value, k: &str| v[k].as_f64().unwrap_or(0.0);
                let st = |v: &serde_json::Value, k: &str| v[k].as_str().filter(|x| !x.is_empty()).map(String::from);
                let specs: Vec<Spec> = m["r"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .take(24)
                            .filter_map(|r| {
                                Some(Spec {
                                    key: st(r, "k")?,
                                    frame: rect(f(r, "x"), f(r, "y"), f(r, "w"), f(r, "h")),
                                    radius: f(r, "r"),
                                    from: st(r, "from"),
                                    button: st(r, "btn").filter(|b| matches!(b.as_str(), "back" | "tabs" | "menu")),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let hot: Vec<CGRect> = m["hot"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|h| {
                                let n: Vec<f64> = h.as_array()?.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                                (n.len() == 4).then(|| rect(n[0], n[1], n[2], n[3]))
                            })
                            .take(8)
                            .collect()
                    })
                    .unwrap_or_default();
                self.set_shapes(specs, hot);
            }
            // Run a bit of the interface's own script (from native buttons and menu items).
            "ui" => self.chrome_js(&s("js")),
            "bookmark" => {
                self.s.toggle_bookmark();
                self.layout();
                self.save();
                self.push();
            }
            "unbookmark" => {
                self.s.remove_bookmark(&s("url"));
                self.layout();
                self.save();
                self.push();
            }
            // A bookmark chip: open it in the current tab.
            "open" => {
                let u = s("url");
                if u.starts_with("http") {
                    self.navigate(u);
                }
            }
            "copy" => {
                if let Some(t) = self.s.tab(self.s.active) {
                    unsafe { UIPasteboard::generalPasteboard().setString(Some(&ns(&t.url))) };
                }
            }
            "desktop" => {
                let aid = self.s.active;
                if let Some(t) = self.s.tab_mut(aid) {
                    t.desktop = !t.desktop;
                    let ua = t.desktop.then(|| ns(DESKTOP_UA));
                    if let Some(v) = &active {
                        unsafe {
                            v.setCustomUserAgent(ua.as_deref());
                            v.reload();
                        }
                    }
                }
                self.push();
            }
            "set" => {
                match self.s.settings.set(&s("k"), &m["v"]) {
                    Changed::Nothing => return,
                    Changed::Plain => {}
                    Changed::Blocker => {
                        self.apply_rules();
                        if let Some(v) = active {
                            unsafe { v.reload() };
                        }
                    }
                    Changed::Theme => self.apply_theme(),
                    // Glass on/off and bar position: pages and touch areas move.
                    Changed::Layout => self.layout(),
                    Changed::Look => self.restyle_newtabs(),
                }
                self.save();
                self.push();
            }
            _ => {}
        }
    }

    /// Messages from pages: title/address updates, and searches from the New Tab page.
    fn on_page(&mut self, wv: &WKWebView, m: &serde_json::Value) {
        let Some(id) = self.tab_of(wv) else { return };
        let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
        match s("t").as_str() {
            "meta" => {
                let url = s("url");
                if let Some(t) = self.s.tab_mut(id) {
                    if url.starts_with("http") {
                        t.url = url;
                        t.title = s("title");
                        t.icon = s("icon");
                    }
                }
                self.push();
            }
            // Only the New Tab page may ask Wisp to search or open an address.
            "go" if self.s.tab(id).is_some_and(|t| t.url.is_empty()) && id == self.s.active => {
                let q = s("q");
                if !q.trim().is_empty() {
                    self.navigate(omni::resolve(&q, &self.s.settings.engine));
                }
            }
            _ => {}
        }
    }

    /// Navigation progress of a page: keep the tab's address, title and loading state current.
    fn on_nav(&mut self, wv: &WKWebView, loading: bool) {
        let Some(id) = self.tab_of(wv) else { return };
        let url = unsafe { wv.URL() }
            .and_then(|u| u.absoluteString())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let title = unsafe { wv.title() }.map(|s| s.to_string()).unwrap_or_default();
        if let Some(t) = self.s.tab_mut(id) {
            t.loading = loading;
            if url.starts_with("http") {
                t.url = url;
                if !title.is_empty() {
                    t.title = title;
                }
            }
        }
        self.push();
        if !loading {
            self.save();
        }
    }
}

fn load(wv: &WKWebView, url: &str, settings: &core::Settings) {
    unsafe {
        if url.is_empty() {
            let html = NEWTAB_HTML.replacen("/*WISP_CFG*/", &format!("window.WISP_CFG={};", settings.newtab()), 1);
            wv.loadHTMLString_baseURL(&ns(&html), None);
        } else if let Some(u) = NSURL::URLWithString(&ns(url)) {
            wv.loadRequest(&NSURLRequest::requestWithURL(&u));
        }
    }
}

fn parse(body: Retained<AnyObject>) -> Option<serde_json::Value> {
    let s = body.downcast::<NSString>().ok()?;
    serde_json::from_str(&s.to_string()).ok()
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WispDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl WKScriptMessageHandler for Delegate {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _c: &WKUserContentController, msg: &WKScriptMessage) {
            let name = unsafe { msg.name() }.to_string();
            let Some(m) = parse(unsafe { msg.body() }) else { return };
            if name == "wisp" {
                with(|b| b.on_chrome(&m));
                return;
            }
            let wv: Option<Retained<WKWebView>> = unsafe { msg_send![msg, webView] };
            if let Some(wv) = wv {
                with(|b| b.on_page(&wv, &m));
            }
        }
    }

    unsafe impl WKNavigationDelegate for Delegate {
        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn started(&self, wv: &WKWebView, _n: Option<&WKNavigation>) {
            with(|b| b.on_nav(wv, true));
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn committed(&self, wv: &WKWebView, _n: Option<&WKNavigation>) {
            with(|b| b.on_nav(wv, true));
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, wv: &WKWebView, _n: Option<&WKNavigation>) {
            with(|b| b.on_nav(wv, false));
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed(&self, wv: &WKWebView, _n: Option<&WKNavigation>, _e: &NSError) {
            with(|b| b.on_nav(wv, false));
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_early(&self, wv: &WKWebView, _n: Option<&WKNavigation>, _e: &NSError) {
            with(|b| b.on_nav(wv, false));
        }

        /// iOS may kill a page's process to free memory; bring the page back.
        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn terminated(&self, wv: &WKWebView) {
            unsafe { wv.reload() };
        }

        /// Links to phone numbers, mail, maps, App Store etc. open in their apps.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(&self, _wv: &WKWebView, action: &WKNavigationAction, handler: &DynBlock<dyn Fn(WKNavigationActionPolicy)>) {
            let url = unsafe { action.request().URL() };
            let scheme = url
                .as_ref()
                .and_then(|u| u.scheme())
                .map(|s| s.to_string().to_lowercase())
                .unwrap_or_default();
            let web = matches!(scheme.as_str(), "http" | "https" | "about" | "data" | "blob" | "file" | "");
            if !web {
                if let Some(u) = url {
                    let app = UIApplication::sharedApplication(self.mtm());
                    unsafe { app.openURL_options_completionHandler(&u, &NSDictionary::new(), None) };
                }
                handler.call((WKNavigationActionPolicy::Cancel,));
                return;
            }
            handler.call((WKNavigationActionPolicy::Allow,));
        }
    }

    unsafe impl WKUIDelegate for Delegate {
        /// Links that open a new window (target=_blank, window.open) open in a new tab.
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create(
            &self,
            _wv: &WKWebView,
            _cfg: &WKWebViewConfiguration,
            action: &WKNavigationAction,
            _features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            let url = unsafe { action.request().URL() }
                .and_then(|u| u.absoluteString())
                .map(|s| s.to_string());
            if let Some(url) = url.filter(|u| u.starts_with("http")) {
                with(|b| {
                    let id = b.s.add_tab(&url);
                    b.s.select(id);
                    b.show();
                });
            }
            None
        }
    }
);

fn sym(name: &str) -> Option<Retained<UIImage>> {
    UIImage::systemImageNamed(&ns(name))
}

/// A native Liquid Glass button for the bar.
fn make_button(mtm: MainThreadMarker, kind: &str) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::glassButtonConfiguration(mtm);
    let icon = match kind {
        "back" => "chevron.backward",
        "tabs" => "square.on.square",
        _ => "ellipsis",
    };
    cfg.setImage(sym(icon).as_deref());
    cfg.setBaseForegroundColor(Some(&UIColor::labelColor()));
    let b = UIButton::buttonWithConfiguration_primaryAction(&cfg, None);
    let msg = match kind {
        "back" => Some(json!({"t":"back"})),
        "tabs" => Some(json!({"t":"ui","js":"open('tabs')"})),
        _ => None,
    };
    match msg {
        Some(m) => {
            let blk = RcBlock::new(move |_a: NonNull<UIAction>| {
                let m = m.clone();
                with(|b| b.on_chrome(&m));
            });
            let act = unsafe { UIAction::actionWithHandler(RcBlock::as_ptr(&blk), mtm) };
            b.addAction_forControlEvents(&act, UIControlEvents::PrimaryActionTriggered);
        }
        // ⋯ opens its menu straight away; on iOS 26 it morphs out of the button.
        None => b.setShowsMenuAsPrimaryAction(true),
    }
    b
}

pub struct PassIvars {
    /// Where the interface takes touches (the bar, or everything while a sheet is open).
    hot: RefCell<Vec<CGRect>>,
}

define_class!(
    // SAFETY: UIView subclass; ivars are plain data, no Drop impl.
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "WispPassView"]
    #[ivars = PassIvars]
    struct PassView;

    unsafe impl NSObjectProtocol for PassView {}

    impl PassView {
        /// Touches outside the hot area fall through to the page underneath.
        #[unsafe(method(hitTest:withEvent:))]
        fn hit_test(&self, p: CGPoint, ev: *mut AnyObject) -> *mut UIView {
            let inside = self.ivars().hot.borrow().iter().any(|r| {
                p.x >= r.origin.x && p.x < r.origin.x + r.size.width && p.y >= r.origin.y && p.y < r.origin.y + r.size.height
            });
            if !inside {
                return std::ptr::null_mut();
            }
            unsafe { msg_send![super(self), hitTest: p, withEvent: ev] }
        }
    }
);

impl PassView {
    fn new(mtm: MainThreadMarker, frame: CGRect) -> Retained<Self> {
        let this: Allocated<Self> = Self::alloc(mtm);
        let this = this.set_ivars(PassIvars {
            hot: RefCell::new(vec![frame]),
        });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

define_class!(
    // SAFETY: plain UIViewController subclass without ivars or Drop.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "WispViewController"]
    struct WispVC;

    unsafe impl NSObjectProtocol for WispVC {}

    impl WispVC {
        /// Rotation, split view and safe-area changes all end up here.
        #[unsafe(method(viewDidLayoutSubviews))]
        fn did_layout(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            with(|b| b.layout());
        }
    }
);

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WispAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl UIApplicationDelegate for AppDelegate {
        #[unsafe(method(application:didFinishLaunchingWithOptions:))]
        fn launched(&self, _app: &UIApplication, _opts: *mut AnyObject) -> bool {
            start(self.mtm());
            true
        }

        #[unsafe(method(applicationDidEnterBackground:))]
        fn backgrounded(&self, _app: &UIApplication) {
            with(|b| b.save());
        }

        /// Low on memory: drop every page except the one on screen.
        #[unsafe(method(applicationDidReceiveMemoryWarning:))]
        fn memory_warning(&self, _app: &UIApplication) {
            with(|b| b.unload_background());
        }
    }
);

fn start(mtm: MainThreadMarker) {
    // One window, set up the classic (non-scene) way: Wisp has a single window on iPhone and iPad.
    #[allow(deprecated)]
    let window = UIWindow::initWithFrame(UIWindow::alloc(mtm), UIScreen::mainScreen(mtm).bounds());
    let vc: Retained<WispVC> = unsafe { msg_send![WispVC::alloc(mtm), init] };
    window.setRootViewController(Some(&**vc));
    let root = vc.view().expect("view controller has a view");
    root.setBackgroundColor(Some(&UIColor::systemBackgroundColor()));
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(mtm), init] };

    // Pages: one shared configuration (cookies/storage in the default store).
    let page_ucc = unsafe { WKUserContentController::new(mtm) };
    let page_cfg = unsafe { WKWebViewConfiguration::new(mtm) };
    unsafe {
        page_ucc.addScriptMessageHandler_name(ProtocolObject::from_ref(&*delegate), &ns("wispPage"));
        let script = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
            WKUserScript::alloc(mtm),
            &ns(PAGE_JS),
            WKUserScriptInjectionTime::AtDocumentEnd,
            true,
        );
        page_ucc.addUserScript(&script);
        page_cfg.setUserContentController(&page_ucc);
        page_cfg.setWebsiteDataStore(&WKWebsiteDataStore::defaultDataStore(mtm));
        let _: () = msg_send![&*page_cfg, setAllowsInlineMediaPlayback: true];
    }

    // Interface: system blur with a transparent web view on top.
    let blur = UIBlurEffect::effectWithStyle(UIBlurEffectStyle::SystemChromeMaterial, mtm);
    let effect = UIVisualEffectView::initWithEffect(UIVisualEffectView::alloc(mtm), Some(&**blur));
    effect.setAlpha(0.0);
    effect.setUserInteractionEnabled(false);
    root.addSubview(&effect);
    let chrome_ucc = unsafe { WKUserContentController::new(mtm) };
    let chrome_cfg = unsafe { WKWebViewConfiguration::new(mtm) };
    unsafe {
        chrome_ucc.addScriptMessageHandler_name(ProtocolObject::from_ref(&*delegate), &ns("wisp"));
        chrome_cfg.setUserContentController(&chrome_ucc);
    }
    let pass = PassView::new(mtm, root.bounds());
    pass.setBackgroundColor(Some(&UIColor::clearColor()));
    let chrome = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), root.bounds(), &chrome_cfg) };
    chrome.setOpaque(false);
    chrome.setBackgroundColor(Some(&UIColor::clearColor()));
    unsafe {
        let sv = chrome.scrollView();
        sv.setBackgroundColor(Some(&UIColor::clearColor()));
        let _: () = msg_send![&*sv, setScrollEnabled: false];
        let _: () = msg_send![&*sv, setBounces: false];
        // Leave "tap the status bar to scroll to top" to the page.
        let _: () = msg_send![&*sv, setScrollsToTop: false];
        // The interface handles safe areas itself (env(safe-area-inset-*)).
        let _: () = msg_send![&*sv, setContentInsetAdjustmentBehavior: 2isize];
        chrome.loadHTMLString_baseURL(&ns(CHROME_HTML), None);
    }
    pass.addSubview(&chrome);
    root.addSubview(&pass);
    let glass = Glass::new(mtm, &root, &pass);

    let path = format!("{}/Documents/wisp.json", NSHomeDirectory());
    let s = Session::load(&std::fs::read_to_string(&path).unwrap_or_default());
    let b = Browser {
        mtm,
        window: window.clone(),
        root,
        effect,
        pass,
        chrome,
        page_ucc,
        page_cfg,
        delegate,
        views: HashMap::new(),
        s,
        full: false,
        rules: None,
        path,
        last_push: String::new(),
        shown: 0,
        glass,
        hot_bar: vec![],
        menu_sig: String::new(),
        blur: false,
    };
    B.with(|cell| *cell.borrow_mut() = Some(b));
    with(|b| {
        b.apply_theme();
        b.show();
    });
    window.makeKeyAndVisible();
    compile_blocker(mtm);
}

/// Compile the ad/tracker rules (WebKit caches the compiled list) and apply them to pages.
fn compile_blocker(mtm: MainThreadMarker) {
    let Some(store) = (unsafe { WKContentRuleListStore::defaultStore(mtm) }) else {
        return;
    };
    let done = RcBlock::new(|list: *mut WKContentRuleList, _err: *mut NSError| {
        // SAFETY: WebKit hands over a valid (or null) rule list.
        if let Some(list) = unsafe { Retained::retain(list) } {
            with(|b| {
                b.rules = Some(list);
                b.apply_rules();
            });
        }
    });
    unsafe {
        store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
            Some(&ns("wisp-blocker")),
            Some(&ns(&core::block_rules())),
            Some(&done),
        );
    }
}

pub fn run() -> ! {
    let mtm = MainThreadMarker::new().expect("Wisp starts on the main thread");
    let delegate_class = NSString::from_class(AppDelegate::class());
    UIApplication::main(None, Some(&delegate_class), mtm)
}
