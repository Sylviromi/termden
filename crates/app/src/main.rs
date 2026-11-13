mod actions;
mod canvas;
mod choices;
mod control;
mod launch;
mod mail;
mod mock;
mod panel;
mod popup;
mod pty;
mod settings;
mod state;
mod terminal;
mod theme;
mod tile;
mod update;
mod widgets;

use gpui::{
    App, AppContext, Application, Bounds, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowOptions, px, size,
};
use termden_core::Config;

fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("termden {}", update::VERSION);
        return;
    }
    let config = Config::load();
    let mode = if std::env::args().any(|arg| arg == "--demo") {
        state::Mode::Demo
    } else if std::env::var_os("TERMDEN_MIRROR").is_some() {
        state::Mode::Mirror
    } else {
        state::Mode::Live
    };
    // A second launch (app launcher, shell) focuses the running window instead.
    if mode == state::Mode::Live
        && std::env::var_os("TERMDEN_RELOADED").is_none()
        && std::os::unix::net::UnixStream::connect(termden_core::state::control_socket()).is_ok()
    {
        let _ = std::process::Command::new("hyprctl")
            .args(["-q", "dispatch", "focuswindow", "class:termden"])
            .status();
        return;
    }

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // Fully borderless: no OS titlebar, no client-side chrome. The
            // panel header doubles as the window drag handle.
            titlebar: None,
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some("termden".into()),
            window_background: WindowBackgroundAppearance::Opaque,
            window_min_size: Some(size(px(980.), px(600.))),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            let config = config.clone();
            let app = cx.new(|cx| state::TermdenApp::new(config, mode, cx));
            window.focus(&app.read(cx).focus);
            app
        })
        .expect("failed to open the termden window");

        // Closing the window quits; harnesses live on in tmux.
        cx.on_window_closed(|cx| cx.quit()).detach();
        cx.activate(true);
    });
}
