use gpui::{AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};

mod link;
mod palette;
mod screen;
mod sidebar;
mod window;

use window::SeerWindow;

fn main() {
    Application::new().run(|cx| {
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
            ..Default::default()
        };
        // gpui on Wayland does not send the title from TitlebarOptions.
        let opened = cx.open_window(options, |window, cx| {
            window.set_window_title("Seer");
            cx.new(|cx| SeerWindow::new(window, cx))
        });
        if let Err(error) = opened {
            eprintln!("seer: could not open the window: {error}");
            cx.quit();
        }
        cx.activate(true);
    });
}
