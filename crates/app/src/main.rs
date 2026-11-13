/// Call `f(args..)` through Subsecond's jump table when built with the
/// `hotpatch` feature, so the newest patched version runs; a plain call
/// otherwise. Entry points from long-lived tasks need it: code they reach
/// stays old until a restart.
macro_rules! hot {
    ($f:expr, $($arg:expr),* $(,)?) => {{
        #[cfg(feature = "hotpatch")]
        let out = subsecond::HotFn::current($f).call(($($arg,)*));
        #[cfg(not(feature = "hotpatch"))]
        let out = $f($($arg),*);
        out
    }};
}

mod actions;
mod canvas;
mod choices;
mod control;
mod decorations;
mod launch;
mod mail;
mod mock;
mod panel;
mod popup;
mod pty;
mod settings;
mod showcase;
mod state;
mod terminal;
mod theme;
mod themes;
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

    #[cfg(feature = "hotpatch")]
    let patched = {
        dioxus_devtools::connect_subsecond();
        let (tx, rx) = async_channel::unbounded::<()>();
        subsecond::register_handler(std::sync::Arc::new(move || {
            let _ = tx.send_blocking(());
        }));
        rx
    };

    let server_decorations = decorations::server_side_available();

    Application::new().run(move |cx: &mut App| {
        #[cfg(feature = "hotpatch")]
        cx.spawn(async move |cx| {
            while patched.recv().await.is_ok() {
                let _ = cx.update(|cx| cx.refresh_windows());
            }
        })
        .detach();

        // Bundled fonts (assets/fonts): a fresh machine renders correctly
        // without installing them; installed copies keep working.
        if let Err(error) = cx.text_system().add_fonts(
            [
                &include_bytes!("../assets/fonts/NotoSans-Regular.ttf")[..],
                include_bytes!("../assets/fonts/NotoSans-Medium.ttf"),
                include_bytes!("../assets/fonts/NotoSans-Bold.ttf"),
                include_bytes!("../assets/fonts/JetBrainsMonoNerdFont-Regular.ttf"),
                include_bytes!("../assets/fonts/JetBrainsMonoNerdFont-Bold.ttf"),
                include_bytes!("../assets/fonts/MaterialSymbolsRounded-termden.ttf"),
            ]
            .into_iter()
            .map(std::borrow::Cow::Borrowed)
            .collect(),
        ) {
            eprintln!("termden: could not load bundled fonts: {error}");
        }
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // `title_bar` (config): the desktop's own title bar, or none (the
            // panel header doubles as the drag handle). Desktops that never
            // draw one (GNOME) get termden's own buttons; see `own_chrome`.
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("termden".into()),
                ..Default::default()
            }),
            window_decorations: Some(if config.title_bar.wanted() && server_decorations {
                WindowDecorations::Server
            } else {
                WindowDecorations::Client
            }),
            app_id: Some("termden".into()),
            window_background: WindowBackgroundAppearance::Opaque,
            window_min_size: Some(size(px(980.), px(600.))),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            let config = config.clone();
            let app = cx.new(|cx| {
                let mut app = state::TermdenApp::new(config, mode, cx);
                app.server_decorations = server_decorations;
                app
            });
            window.focus(&app.read(cx).focus);
            app
        })
        .expect("failed to open the termden window");

        // Closing the window quits; harnesses live on in tmux.
        cx.on_window_closed(|cx| cx.quit()).detach();
        cx.activate(true);
    });
}
