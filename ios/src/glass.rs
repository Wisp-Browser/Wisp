//! Native glass for the interface.
//!
//! On iOS 26 this is Apple's Liquid Glass: `UIGlassEffect` views and glass `UIButton`s
//! (`UIButtonConfiguration.glassButtonConfiguration`) inside one `UIGlassContainerEffect`, so nearby
//! glass blends and morphs together. Glass materializes and dematerializes by animating each view's
//! `effect` (not its alpha). Before iOS 26 the same shapes use the system blur.
//!
//! The web interface draws the content (text, icons) and tells Rust where each glass shape goes,
//! by name, so a shape keeps its identity while it moves (the menu panel grows out of the ⋯
//! button and shrinks back into it).

use std::cell::RefCell;
use std::collections::HashMap;

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool, NSObjectProtocol};
use objc2::{define_class, msg_send, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    UIBlurEffect, UIBlurEffectStyle, UIButton, UICornerConfiguration, UICornerRadius, UIGlassContainerEffect, UIGlassEffect,
    UIGlassEffectStyle, UIView, UIViewAnimationOptions, UIVisualEffect, UIVisualEffectView,
};

/// Liquid Glass is available (iOS 26 or later).
pub fn liquid_glass() -> bool {
    AnyClass::get(c"UIGlassEffect").is_some() && AnyClass::get(c"UIGlassContainerEffect").is_some()
}

/// A short spring, the way system glass moves.
pub fn spring(mtm: MainThreadMarker, secs: f64, f: impl Fn() + 'static, done: Option<RcBlock<dyn Fn(Bool)>>) {
    let anim = RcBlock::new(f);
    UIView::animateWithDuration_delay_usingSpringWithDamping_initialSpringVelocity_options_animations_completion(
        secs,
        0.0,
        0.82,
        0.0,
        UIViewAnimationOptions::AllowUserInteraction | UIViewAnimationOptions::BeginFromCurrentState,
        &anim,
        done.as_deref(),
        mtm,
    );
}

fn effect(mtm: MainThreadMarker) -> Retained<UIVisualEffect> {
    if liquid_glass() {
        let g = UIGlassEffect::effectWithStyle(UIGlassEffectStyle::Regular, mtm);
        g.setInteractive(true);
        return Retained::into_super(g);
    }
    Retained::into_super(UIBlurEffect::effectWithStyle(UIBlurEffectStyle::SystemChromeMaterial, mtm))
}

fn round(v: &UIView, r: f64, h: f64) {
    if liquid_glass() {
        let c = if r * 2.0 >= h - 0.5 {
            UICornerConfiguration::capsuleConfiguration()
        } else {
            UICornerConfiguration::configurationWithUniformRadius(&UICornerRadius::fixedRadius(r))
        };
        v.setCornerConfiguration(&c);
    } else {
        v.setClipsToBounds(true);
        unsafe {
            let layer: Retained<AnyObject> = msg_send![v, layer];
            let _: () = msg_send![&*layer, setCornerRadius: r];
            let _: () = msg_send![&*layer, setCornerCurve: &*NSString::from_str("continuous")];
        }
    }
}

define_class!(
    // SAFETY: UIView subclass without ivars or Drop.
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "WispGlassHolder"]
    struct Holder;

    unsafe impl NSObjectProtocol for Holder {}

    impl Holder {
        /// Only the glass buttons take touches; everywhere else falls through to the page.
        #[unsafe(method(hitTest:withEvent:))]
        fn hit_test(&self, p: CGPoint, ev: *mut AnyObject) -> *mut UIView {
            let v: *mut UIView = unsafe { msg_send![super(self), hitTest: p, withEvent: ev] };
            if v.is_null() || std::ptr::eq(v, &**self as *const UIView) || is_effect_part(v) {
                return std::ptr::null_mut();
            }
            v
        }
    }
);

/// Glass views and their internals never take touches (only buttons do).
fn is_effect_part(v: *mut UIView) -> bool {
    let mut cur = v;
    while !cur.is_null() {
        let o = unsafe { &*cur };
        let is_button: bool = unsafe { msg_send![o, isKindOfClass: UIButton::class()] };
        if is_button {
            return false;
        }
        let is_holder: bool = unsafe { msg_send![o, isKindOfClass: Holder::class()] };
        if is_holder {
            return true;
        }
        cur = unsafe { msg_send![o, superview] };
    }
    true
}

impl Holder {
    fn new(mtm: MainThreadMarker, frame: CGRect) -> Retained<Self> {
        let this: Allocated<Self> = Self::alloc(mtm);
        unsafe { msg_send![this, initWithFrame: frame] }
    }
}

/// Where one glass shape goes, as asked for by the interface.
pub struct Spec {
    pub key: String,
    pub frame: CGRect,
    pub radius: f64,
    /// Grow out of (and shrink back into) this other shape.
    pub from: Option<String>,
    /// A native glass button of this kind ("back", "tabs", "menu") instead of a plain glass shape.
    pub button: Option<String>,
}

struct Item {
    view: Retained<UIView>,
    effect: Option<Retained<UIVisualEffectView>>,
    button: Option<Retained<UIButton>>,
    frame: CGRect,
    from: Option<String>,
}

pub struct Glass {
    mtm: MainThreadMarker,
    holder: Retained<Holder>,
    /// iOS 26: the container that blends all glass together.
    container: Option<Retained<UIVisualEffectView>>,
    items: RefCell<HashMap<String, Item>>,
}

impl Glass {
    /// Adds the glass layer to `root`, just below `above` (the interface).
    pub fn new(mtm: MainThreadMarker, root: &UIView, above: &UIView) -> Glass {
        let b = root.bounds();
        let holder = Holder::new(mtm, b);
        root.insertSubview_belowSubview(&holder, above);
        let container = liquid_glass().then(|| {
            let ce = UIGlassContainerEffect::new(mtm);
            // Shapes 8pt apart stay separate; they blend while they move past each other.
            ce.setSpacing(6.0);
            let v = UIVisualEffectView::initWithEffect(UIVisualEffectView::alloc(mtm), Some(&Retained::into_super(ce)));
            v.setFrame(b);
            holder.addSubview(&v);
            v
        });
        Glass {
            mtm,
            holder,
            container,
            items: RefCell::new(HashMap::new()),
        }
    }

    fn parent(&self) -> Retained<UIView> {
        match &self.container {
            Some(c) => c.contentView(),
            None => Retained::into_super(self.holder.clone()),
        }
    }

    pub fn layout(&self, bounds: CGRect) {
        self.holder.setFrame(bounds);
        if let Some(c) = &self.container {
            c.setFrame(bounds);
        }
    }

    pub fn button(&self, key: &str) -> Option<Retained<UIButton>> {
        self.items.borrow().get(key).and_then(|i| i.button.clone())
    }

    /// Bring the glass in line with `specs`: shapes that exist glide to their new place, new ones
    /// materialize (out of their `from` shape, if any), missing ones dematerialize.
    pub fn sync(&self, specs: &[Spec], make_button: &dyn Fn(&str) -> Retained<UIButton>) {
        let mtm = self.mtm;
        let mut items = self.items.borrow_mut();
        let frame_of = |items: &HashMap<String, Item>, k: &Option<String>| k.as_ref().and_then(|k| items.get(k)).map(|i| i.frame);
        for s in specs {
            let (f, r) = (s.frame, s.radius);
            if let Some(it) = items.get_mut(&s.key) {
                if it.frame != f {
                    it.frame = f;
                    let v = it.view.clone();
                    let rounded = it.effect.is_some();
                    spring(
                        mtm,
                        0.42,
                        move || {
                            v.setFrame(f);
                            if rounded {
                                round(&v, r, f.size.height);
                            }
                        },
                        None,
                    );
                }
                continue;
            }
            let start = frame_of(&items, &s.from).unwrap_or(f);
            let item = if let Some(kind) = &s.button {
                let b = make_button(kind);
                b.setFrame(start);
                b.setAlpha(0.0);
                self.parent().addSubview(&b);
                let v = b.clone();
                spring(
                    mtm,
                    0.42,
                    move || {
                        v.setAlpha(1.0);
                        v.setFrame(f);
                    },
                    None,
                );
                Item {
                    view: Retained::into_super(Retained::into_super(b.clone())),
                    effect: None,
                    button: Some(b),
                    frame: f,
                    from: s.from.clone(),
                }
            } else {
                let e = UIVisualEffectView::initWithEffect(UIVisualEffectView::alloc(mtm), None);
                e.setUserInteractionEnabled(false);
                e.setFrame(start);
                round(&e, r, start.size.height);
                self.parent().addSubview(&e);
                let v = e.clone();
                // Materialize: animate the effect itself in.
                spring(
                    mtm,
                    0.45,
                    move || {
                        v.setEffect(Some(&effect(mtm)));
                        v.setFrame(f);
                        round(&v, r, f.size.height);
                    },
                    None,
                );
                Item {
                    view: Retained::into_super(e.clone()),
                    effect: Some(e),
                    button: None,
                    frame: f,
                    from: s.from.clone(),
                }
            };
            items.insert(s.key.clone(), item);
        }
        let gone: Vec<String> = items.keys().filter(|k| !specs.iter().any(|s| &s.key == *k)).cloned().collect();
        for k in gone {
            let Some(it) = items.remove(&k) else { continue };
            let back_to = frame_of(&items, &it.from);
            let v = it.view.clone();
            let e = it.effect.clone();
            let remove = it.view.clone();
            spring(
                mtm,
                0.36,
                move || {
                    match &e {
                        // Dematerialize the glass.
                        Some(e) => e.setEffect(None),
                        None => v.setAlpha(0.0),
                    }
                    if let Some(f) = back_to {
                        v.setFrame(f);
                    }
                },
                Some(RcBlock::new(move |_| remove.removeFromSuperview())),
            );
        }
    }
}
