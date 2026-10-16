use gpui::{
    AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowDecorations,
    WindowOptions, point, px, size,
};

mod appearance;
mod assets;
mod backdrop;
mod cap_icon;
mod chrome;
mod composition;
mod input;
mod invite;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod invite_e2e;
mod join;
mod keyboard;
mod label;
mod link;
mod menu;
mod motion;
mod name_input;
#[cfg(target_os = "macos")]
mod native_frame;
#[cfg(target_os = "macos")]
mod native_keys;
mod palette;
mod room;
mod screen;
mod sidebar;
mod tab_time;
mod tip;
mod urls;
mod window;

use window::SeerWindow;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod join_e2e;

fn main() {
    #[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
    let Some(fixture) = window::native_text_e2e::prepare() else {
        return;
    };
    if cfg!(all(test, not(target_os = "macos"))) {
        return;
    }
    let links = urls::Links::new();
    let incoming = links.clone();
    let application = Application::new().with_assets(assets::Assets);
    application.on_open_urls(move |urls| incoming.borrow_mut().open(urls));
    application.run(move |cx| {
        if let Err(error) = assets::load_fonts(cx) {
            eprintln!("seer: could not load the bundled fonts: {error}");
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                // gpui 0.2.2 on X11 runs this callback while it holds its
                // client state, and quit takes that state again. Quit on a
                // later turn of the main thread.
                cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach();
            }
        })
        .detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(960.), px(640.)),
                cx,
            ))),
            app_id: Some("seer".into()),
            titlebar: Some(TitlebarOptions {
                title: Some("Seer".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(19.), px(15.))),
            }),
            window_decorations: Some(WindowDecorations::Client),
            ..Default::default()
        };
        #[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
        let options = window::native_text_e2e::window_options(options, cx);
        // gpui on Wayland does not send the title from TitlebarOptions.
        let opened = cx.open_window(options, |window, cx| {
            window.set_window_title("Seer");
            let view = cx.new(|cx| SeerWindow::new(window, cx));
            #[cfg(target_os = "macos")]
            view.update(cx, |view, cx| {
                view.input._native_keys = native_keys::install(window, cx);
                view.input.frame = native_frame::NativeFrame::new(window);
            });
            view
        });
        match opened {
            Ok(window) => {
                links.borrow_mut().attach(window, cx);
                #[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
                window::native_text_e2e::exercise(window, fixture, cx);
            }
            Err(error) => {
                eprintln!("seer: could not open the window: {error}");
                cx.quit();
            }
        }
        cx.activate(true);
    });
}
