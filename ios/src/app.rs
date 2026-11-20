//! UIKit / WebKit side of Wisp for iOS.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use block2::{DynBlock, RcBlock};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSDictionary, NSError, NSHomeDirectory, NSString, NSURLRequest, NSURL};
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIBlurEffect, UIBlurEffectStyle, UIColor, UIEdgeInsets, UIPasteboard, UIScreen, UIView,
    UIViewAnimationOptions, UIViewController, UIVisualEffect, UIVisualEffectView, UIWindow,
};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKScriptMessage, WKScriptMessageHandler, WKUIDelegate, WKUserContentController, WKUserScript, WKUserScriptInjectionTime,
    WKWebViewConfiguration, WKWebsiteDataStore, WKWindowFeatures,
};

use crate::core::{self, Changed, Session};
use crate::omni;
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
    /// Native glass shapes behind the interface's buttons and panels (Liquid Glass on iOS 26).
    shapes: Vec<Shape>,
    /// Where the bar takes touches, as measured by the interface.
    hot_bar: Option<CGRect>,
    /// The open sheet wants the full-screen blur behind it (the menu uses a glass panel instead).
    blur: bool,
}

struct Shape {
    view: Retained<UIVisualEffectView>,
    on: bool,
}

/// Apple's Liquid Glass (`UIGlassEffect`, iOS 26) when available, otherwise the system blur.
fn glass_effect(mtm: MainThreadMarker) -> Retained<UIVisualEffect> {
    if let Some(cls) = AnyClass::get(c"UIGlassEffect") {
        let e: Option<Retained<UIVisualEffect>> = unsafe { msg_send![cls, effectWithStyle: 0isize] };
        if let Some(e) = e {
            return e;
        }
    }
    Retained::into_super(UIBlurEffect::effectWithStyle(UIBlurEffectStyle::SystemChromeMaterial, mtm))
}

fn set_corner(v: &UIView, r: f64) {
    unsafe {
        let layer: Retained<AnyObject> = msg_send![v, layer];
        let _: () = msg_send![&*layer, setCornerRadius: r];
        let _: () = msg_send![&*layer, setCornerCurve: &*ns("continuous")];
    }
}

fn spring(mtm: MainThreadMarker, secs: f64, f: impl Fn() + 'static) {
    let anim = RcBlock::new(f);
    UIView::animateWithDuration_delay_usingSpringWithDamping_initialSpringVelocity_options_animations_completion(
        secs,
        0.0,
        0.86,
        0.0,
        UIViewAnimationOptions::AllowUserInteraction | UIViewAnimationOptions::BeginFromCurrentState,
        &anim,
        None,
        mtm,
    );
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
        let area = if self.full {
            b
        } else {
            self.hot_bar.unwrap_or_else(|| self.bar_rect())
        };
        self.pass.ivars().hot.set(area);
        self.effect.setFrame(b);
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

    /// Place the native glass shapes (screen rects + corner radius) the interface asked for.
    /// Existing shapes glide to their new place, new ones fade in, unused ones fade out.
    fn set_shapes(&mut self, rects: &[[f64; 5]], hot: Option<CGRect>) {
        self.hot_bar = hot;
        if !self.full {
            if let Some(h) = hot {
                self.pass.ivars().hot.set(h);
            }
        }
        for (i, r) in rects.iter().enumerate() {
            let f = rect(r[0], r[1], r[2], r[3]);
            let radius = r[4].min(r[2] / 2.0).min(r[3] / 2.0).max(0.0);
            if i == self.shapes.len() {
                let v = UIVisualEffectView::initWithEffect(UIVisualEffectView::alloc(self.mtm), Some(&glass_effect(self.mtm)));
                v.setClipsToBounds(true);
                v.setUserInteractionEnabled(false);
                v.setAlpha(0.0);
                self.root.insertSubview_belowSubview(&v, &self.pass);
                self.shapes.push(Shape { view: v, on: false });
            }
            let sh = &mut self.shapes[i];
            let v = sh.view.clone();
            if sh.on {
                spring(self.mtm, 0.36, move || {
                    v.setFrame(f);
                    set_corner(&v, radius);
                });
            } else {
                sh.on = true;
                v.setFrame(f);
                set_corner(&v, radius);
                spring(self.mtm, 0.25, move || v.setAlpha(1.0));
            }
        }
        for sh in self.shapes.iter_mut().skip(rects.len()).filter(|s| s.on) {
            sh.on = false;
            let v = sh.view.clone();
            spring(self.mtm, 0.2, move || v.setAlpha(0.0));
        }
    }

    /// Open or close a sheet. Sheets that want it get a full-screen blur, which fades in and out.
    fn set_full(&mut self, full: bool, blur: bool) {
        self.full = full;
        self.blur = blur;
        let area = if full {
            self.root.bounds()
        } else {
            self.hot_bar.unwrap_or_else(|| self.bar_rect())
        };
        self.pass.ivars().hot.set(area);
        let target = if full && blur && self.s.settings.glass { 1.0 } else { 0.0 };
        let effect = self.effect.clone();
        spring(self.mtm, if full { 0.3 } else { 0.22 }, move || effect.setAlpha(target));
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
        let st = self.s.view_state(back, fwd).to_string();
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
                let nums = |v: &serde_json::Value| -> Vec<f64> {
                    v.as_array()
                        .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
                        .unwrap_or_default()
                };
                let rects: Vec<[f64; 5]> = m["r"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(nums)
                            .filter(|r| r.len() == 5)
                            .map(|r| [r[0], r[1], r[2], r[3], r[4]])
                            .take(16)
                            .collect()
                    })
                    .unwrap_or_default();
                let h = nums(&m["hot"]);
                let hot = (h.len() == 4).then(|| rect(h[0], h[1], h[2], h[3]));
                self.set_shapes(&rects, hot);
            }
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

pub struct PassIvars {
    /// Where the interface takes touches (the bar, or everything while a sheet is open).
    hot: Cell<CGRect>,
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
            let r = self.ivars().hot.get();
            let inside = p.x >= r.origin.x && p.x < r.origin.x + r.size.width && p.y >= r.origin.y && p.y < r.origin.y + r.size.height;
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
        let this = this.set_ivars(PassIvars { hot: Cell::new(frame) });
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
        shapes: vec![],
        hot_bar: None,
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
