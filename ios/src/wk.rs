//! `WKWebView` for iOS. The objc2-web-kit bindings only declare it for macOS (as an NSView), so the
//! UIKit version and the methods Wisp uses are declared here.

#![allow(non_snake_case)]

use block2::DynBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{extern_class, extern_methods, MainThreadOnly};
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSError, NSString, NSURLRequest, NSURL};
use objc2_ui_kit::{UIResponder, UIScrollView, UIView};
use objc2_web_kit::{WKNavigation, WKNavigationDelegate, WKUIDelegate, WKWebViewConfiguration};

extern_class!(
    /// WebKit's page view on iOS (a UIView).
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WKWebView"]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct WKWebView;
);

impl WKWebView {
    extern_methods!(
        #[unsafe(method(initWithFrame:configuration:))]
        pub unsafe fn initWithFrame_configuration(
            this: Allocated<Self>,
            frame: CGRect,
            configuration: &WKWebViewConfiguration,
        ) -> Retained<Self>;

        #[unsafe(method(setNavigationDelegate:))]
        pub unsafe fn setNavigationDelegate(&self, delegate: Option<&ProtocolObject<dyn WKNavigationDelegate>>);

        #[unsafe(method(setUIDelegate:))]
        pub unsafe fn setUIDelegate(&self, delegate: Option<&ProtocolObject<dyn WKUIDelegate>>);

        #[unsafe(method(setAllowsBackForwardNavigationGestures:))]
        pub unsafe fn setAllowsBackForwardNavigationGestures(&self, allow: bool);

        #[unsafe(method(setCustomUserAgent:))]
        pub unsafe fn setCustomUserAgent(&self, ua: Option<&NSString>);

        #[unsafe(method(loadRequest:))]
        pub unsafe fn loadRequest(&self, request: &NSURLRequest) -> Option<Retained<WKNavigation>>;

        #[unsafe(method(loadHTMLString:baseURL:))]
        pub unsafe fn loadHTMLString_baseURL(&self, html: &NSString, base: Option<&NSURL>) -> Option<Retained<WKNavigation>>;

        #[unsafe(method(canGoBack))]
        pub unsafe fn canGoBack(&self) -> bool;

        #[unsafe(method(canGoForward))]
        pub unsafe fn canGoForward(&self) -> bool;

        #[unsafe(method(goBack))]
        pub unsafe fn goBack(&self) -> Option<Retained<WKNavigation>>;

        #[unsafe(method(goForward))]
        pub unsafe fn goForward(&self) -> Option<Retained<WKNavigation>>;

        #[unsafe(method(reload))]
        pub unsafe fn reload(&self) -> Option<Retained<WKNavigation>>;

        #[unsafe(method(stopLoading))]
        pub unsafe fn stopLoading(&self);

        #[unsafe(method(URL))]
        pub unsafe fn URL(&self) -> Option<Retained<NSURL>>;

        #[unsafe(method(title))]
        pub unsafe fn title(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(scrollView))]
        pub unsafe fn scrollView(&self) -> Retained<UIScrollView>;

        #[unsafe(method(evaluateJavaScript:completionHandler:))]
        pub unsafe fn evaluateJavaScript_completionHandler(
            &self,
            script: &NSString,
            done: Option<&DynBlock<dyn Fn(*mut AnyObject, *mut NSError)>>,
        );
    );
}
