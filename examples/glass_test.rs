//! Minimal probe: NSVisualEffectView insertion via objc2 0.6 msg_send.
//! Exit codes: 1=step1 fail 2=step2 fail 3=step3 fail 0=all ok
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;

fn main() {
    let el = EventLoop::new();
    let w = WindowBuilder::new()
        .with_title("glass-probe")
        .with_visible(false)
        .with_transparent(true)
        .with_decorations(false)
        .build(&el)
        .unwrap();

    let step = std::cell::Cell::new(0u8);
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    el.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        if step.get() == 0 {
            step.set(1);
            unsafe {
                use tao::platform::macos::WindowExtMacOS;
                let ns = w.ns_window() as *mut AnyObject;
                let cv: *mut AnyObject = msg_send![ns, contentView];
                println!("cv={cv:p}");
                if cv.is_null() {
                    step.set(2);
                    return;
                }
                let subs: *mut AnyObject = msg_send![cv, subviews];
                let n: usize = msg_send![subs, count];
                println!("subviews={n}");
                step.set(2);
            }
        } else if step.get() == 2 {
            step.set(3);
            unsafe {
                use objc2::ClassType;
                use objc2_app_kit::NSVisualEffectView;
                use tao::platform::macos::WindowExtMacOS;
                let ns = w.ns_window() as *mut AnyObject;
                let cv: *mut AnyObject = msg_send![ns, contentView];
                let mtm = objc2::MainThreadMarker::new().unwrap();
                let cv_ref: &objc2_app_kit::NSView = unsafe { &*(cv as *const objc2_app_kit::NSView) };
                let vev = NSVisualEffectView::initWithFrame(
                    mtm.alloc(),
                    cv_ref.bounds(),
                );
                println!("vev={vev:p}");
                vev.setMaterial(objc2_app_kit::NSVisualEffectMaterial::HUDWindow);
                vev.setBlendingMode(objc2_app_kit::NSVisualEffectBlendingMode::BehindWindow);
                vev.setState(objc2_app_kit::NSVisualEffectState::Active);
                vev.setAutoresizingMask(
                    objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
                        | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
                );
                println!("setup ok");
                let vev_ref: &objc2_app_kit::NSView = &*vev;
                cv_ref.addSubview_positioned_relativeTo(
                    vev_ref,
                    objc2_app_kit::NSWindowOrderingMode::Below,
                    None,
                );
                println!("inserted");
                step.set(0);
                *control_flow = ControlFlow::Exit;
            }
        }
    });
    println!("DONE");
}

fn view_setup(vev: *mut objc2::runtime::AnyObject) {
    use objc2_app_kit::{NSVisualEffectView, NSVisualEffectMaterial, NSVisualEffectBlendingMode, NSVisualEffectState, NSAutoresizingMaskOptions};
    unsafe {
        let view: &NSVisualEffectView = &*(vev as *const NSVisualEffectView);
        view.setMaterial(NSVisualEffectMaterial::HUDWindow);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::Active);
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    }
}
