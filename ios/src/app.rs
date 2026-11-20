//! UIKit / WebKit side of Wisp for iOS.

use std::cell::RefCell;
use std::collections::HashMap;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSDictionary, NSError, NSHomeDirectory, NSString, NSURLRequest, NSURL};
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIBlurEffect, UIBlurEffectStyle, UIColor, UIPasteboard, UIScreen, UIView, UIViewController,
    UIVisualEffectView, UIWindow,
};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKScriptMessage, WKScriptMessageHandler, WKUIDelegate, WKUserContentController, WKUserScript, WKUserScriptInjectionTime,
    WKWebViewConfiguration, WKWebsiteDataStore, WKWindowFeatures,
};

use crate::core::{self, Session};
use crate::omni;
use crate::wk::WKWebView;

const CHROME_HTML: &str = include_str!("../ui/chrome.html");
const NEWTAB_HTML: &str = include_str!("../ui/newtab.html");
/// Height of the bottom bar above the home-indicator area.
const BAR_H: f64 = 54.0;
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
    _window: Retained<UIWindow>,
    root: Retained<UIView>,
    effect: Retained<UIVisualEffectView>,
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
    fn page_frame(&self) -> CGRect {
        let b = self.root.bounds();
        let bar = BAR_H + self.root.safeAreaInsets().bottom;
        rect(0.0, 0.0, b.size.width, (b.size.height - bar).max(0.0))
    }

    fn layout(&mut self) {
        let b = self.root.bounds();
        let bar = BAR_H + self.root.safeAreaInsets().bottom;
        let cf = if self.full {
            b
        } else {
            rect(0.0, b.size.height - bar, b.size.width, bar)
        };
        self.effect.setFrame(cf);
        self.chrome.setFrame(cf);
        let pf = self.page_frame();
        for v in self.views.values() {
            v.setFrame(pf);
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
        let (url, desktop) = self.s.tab(id).map(|t| (t.url.clone(), t.desktop)).unwrap_or_default();
        if desktop {
            unsafe { wv.setCustomUserAgent(Some(&ns(DESKTOP_UA))) };
        }
        load(&wv, &url);
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
        for (tid, v) in &self.views {
            v.setHidden(*tid != id);
        }
        self.layout();
        self.push();
        self.save();
    }

    fn navigate(&mut self, url: String) {
        let id = self.s.active;
        if let Some(t) = self.s.tab_mut(id) {
            t.url = url.clone();
        }
        match self.views.get(&id) {
            Some(v) => load(v, &url),
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
            "mode" => {
                self.full = m["full"].as_bool().unwrap_or(false);
                self.layout();
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
                match s("k").as_str() {
                    "engine" => self.s.settings.engine = omni::engine_by_id(&s("v")).id.to_string(),
                    "blocker" => {
                        self.s.settings.blocker = m["v"].as_bool().unwrap_or(true);
                        self.apply_rules();
                        if let Some(v) = active {
                            unsafe { v.reload() };
                        }
                    }
                    _ => {}
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

fn load(wv: &WKWebView, url: &str) {
    unsafe {
        if url.is_empty() {
            wv.loadHTMLString_baseURL(&ns(NEWTAB_HTML), None);
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
    root.addSubview(&effect);
    let chrome_ucc = unsafe { WKUserContentController::new(mtm) };
    let chrome_cfg = unsafe { WKWebViewConfiguration::new(mtm) };
    unsafe {
        chrome_ucc.addScriptMessageHandler_name(ProtocolObject::from_ref(&*delegate), &ns("wisp"));
        chrome_cfg.setUserContentController(&chrome_ucc);
    }
    let chrome = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), root.bounds(), &chrome_cfg) };
    chrome.setOpaque(false);
    chrome.setBackgroundColor(Some(&UIColor::clearColor()));
    unsafe {
        let sv = chrome.scrollView();
        sv.setBackgroundColor(Some(&UIColor::clearColor()));
        let _: () = msg_send![&*sv, setScrollEnabled: false];
        let _: () = msg_send![&*sv, setBounces: false];
        // The interface handles safe areas itself (env(safe-area-inset-*)).
        let _: () = msg_send![&*sv, setContentInsetAdjustmentBehavior: 2isize];
        chrome.loadHTMLString_baseURL(&ns(CHROME_HTML), None);
    }
    root.addSubview(&chrome);

    let path = format!("{}/Documents/wisp.json", NSHomeDirectory());
    let s = Session::load(&std::fs::read_to_string(&path).unwrap_or_default());
    let b = Browser {
        mtm,
        _window: window.clone(),
        root,
        effect,
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
    };
    B.with(|cell| *cell.borrow_mut() = Some(b));
    with(|b| b.show());
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
