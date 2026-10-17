// macOS chrome installed at startup, clean-room over AppKit (objc2):
// 1) status-bar item — Alfred lives in the menu bar, not the Dock
// 2) a standard Edit menu so ⌘X/⌘C/⌘V/⌘A reach webview text fields
//    (wry without a main menu never receives those key equivalents).

use crate::gui::{UserEvent, PREFS, SHOW};
use std::sync::OnceLock;
use tao::event_loop::EventLoopProxy;

static PROXY: OnceLock<EventLoopProxy<UserEvent>> = OnceLock::new();

#[cfg(target_os = "macos")]
pub fn install(proxy: EventLoopProxy<UserEvent>, mtm: objc2_foundation::MainThreadMarker) {
    use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
    use objc2::{define_class, msg_send, ClassType, MainThreadOnly};
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSMenu, NSMenuItem, NSStatusItem, NSStatusBar,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSString};

    define_class!(
        /// Target for status-bar / menu actions. AppKit calls these on the
        /// main thread, where sending a tao UserEvent is safe.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "RalfredTrayTarget"]
        struct TrayTarget;

        unsafe impl NSObjectProtocol for TrayTarget {}

        impl TrayTarget {
            #[unsafe(method(showPanel:))]
            fn show_panel(&self, _sender: Option<&AnyObject>) {
                if let Some(p) = PROXY.get() {
                    let _ = p.send_event(UserEvent(SHOW));
                }
            }
            #[unsafe(method(openPrefs:))]
            fn open_prefs(&self, _sender: Option<&AnyObject>) {
                if let Some(p) = PROXY.get() {
                    let _ = p.send_event(UserEvent(PREFS));
                }
            }
            #[unsafe(method(quitRalfred:))]
            fn quit_ralfred(&self, _sender: Option<&AnyObject>) {
                if let Some(mtm) = MainThreadMarker::new() {
                    let app = NSApplication::sharedApplication(mtm);
                    app.terminate(None);
                }
            }
        }
    );

    unsafe {
        let _ = PROXY.set(proxy);

        // ---- status bar item ----
        let bar = NSStatusBar::systemStatusBar();
        let item: Retained<NSStatusItem> = bar.statusItemWithLength(-1.0);
        if let Some(btn) = item.button(mtm) {
            // "R!" is legible in screenshots so the tray can be verified
            // remotely (a 16px template glyph was indistinguishable among
            // the other bar icons). Template-rendered so it follows the
            // menu-bar appearance (light/dark/notch).
            btn.setTitle(&NSString::from_str("R!"));
            
        }
        let target: Retained<TrayTarget> =
            Retained::retain(msg_send![<TrayTarget as ClassType>::class(), new]).unwrap();

        let menu = NSMenu::new(mtm);
        let mk = |title: &str, sel_name: &'static str, m: &NSMenu| {
            let s = Sel::register(&std::ffi::CString::new(sel_name).unwrap());
            let mi = m.addItemWithTitle_action_keyEquivalent(
                &NSString::from_str(title),
                Some(s),
                &NSString::from_str(""),
            );
            mi.setTarget(Some(&*target));
        };
        mk("Open Ralfred Panel", "showPanel:", &menu);
        mk("Preferences…", "openPrefs:", &menu);
        mk("Quit Ralfred", "quitRalfred:", &menu);
        item.setMenu(Some(&menu));
        // AppKit releases the status item the moment our last Rust reference
        // dies — leak the pieces deliberately so the icon lives for the
        // lifetime of the process (the menu is retained by the item).
        std::mem::forget(menu);
        std::mem::forget(target.clone());
        std::mem::forget(item);

        // ---- main menu ----
        // macOS turns the FIRST item of the main menu into the application
        // menu (its title is replaced by the app name). Last round the Edit
        // menu was first, so it silently *became* the app menu — the real
        // Edit menu needs to be the second item.
        let main = NSMenu::new(mtm);
        let app_item = NSMenuItem::new(mtm);
        app_item.setTitle(&NSString::from_str("Ralfred"));
        let app_menu = NSMenu::new(mtm);
        let show_mi = app_menu.addItemWithTitle_action_keyEquivalent(
            &NSString::from_str("Show Panel"),
            Some(Sel::register(&std::ffi::CString::new("showPanel:").unwrap())),
            &NSString::from_str(""),
        );
        show_mi.setTarget(Some(&*target));
        let pref_mi = app_menu.addItemWithTitle_action_keyEquivalent(
            &NSString::from_str("Settings\u{2026}"),
            Some(Sel::register(&std::ffi::CString::new("openPrefs:").unwrap())),
            &NSString::from_str(","),
        );
        pref_mi.setTarget(Some(&*target));
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit_mi = app_menu.addItemWithTitle_action_keyEquivalent(
            &NSString::from_str("Quit Ralfred"),
            Some(Sel::register(&std::ffi::CString::new("quitRalfred:").unwrap())),
            &NSString::from_str("q"),
        );
        quit_mi.setTarget(Some(&*target));
        app_item.setSubmenu(Some(&app_menu));
        main.addItem(&app_item);
        let forget_app_menu = true;
        std::mem::forget(app_menu);
        let _ = forget_app_menu;

        // ---- Edit menu: key equivalents for text fields ----
        let edit = NSMenu::new(mtm);
        edit.setTitle(&NSString::from_str("Edit"));
        let add = |title: &str, action: &str, key: &str, m: &NSMenu| {
            let s = Sel::register(&std::ffi::CString::new(action).unwrap());
            m.addItemWithTitle_action_keyEquivalent(
                &NSString::from_str(title),
                Some(s),
                &NSString::from_str(key),
            );
        };
        add("Undo", "undo:", "z", &edit);
        add("Redo", "redo:", "Z", &edit);
        edit.addItem(&NSMenuItem::separatorItem(mtm));
        add("Cut", "cut:", "x", &edit);
        add("Copy", "copy:", "c", &edit);
        add("Paste", "paste:", "v", &edit);
        add("Select All", "selectAll:", "a", &edit);
        let edit_item = NSMenuItem::new(mtm);
        edit_item.setTitle(&NSString::from_str("Edit"));
        edit_item.setSubmenu(Some(&edit));
        main.addItem(&edit_item);
        let app = NSApplication::sharedApplication(mtm);
        app.setMainMenu(Some(&main));
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install(_proxy: EventLoopProxy<UserEvent>, _mtm: objc2_foundation::MainThreadMarker) {}
