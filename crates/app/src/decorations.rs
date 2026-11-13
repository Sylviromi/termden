//! Can the desktop draw our title bar? On Wayland that takes the
//! xdg-decoration protocol, which GNOME (and Weston) do not offer. GPUI cannot
//! tell: it reports server-side decorations whenever they were requested. So
//! ask the compositor directly, once, on a short-lived connection.

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle};

struct Probe;

impl Dispatch<WlRegistry, GlobalListContents> for Probe {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Whether the window manager draws title bars. X11 window managers do; on
/// Wayland only compositors with xdg-decoration. Unknown answers count as yes
/// (that is what termden assumed before this check existed).
pub fn server_side_available() -> bool {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return true;
    }
    let Ok(conn) = Connection::connect_to_env() else {
        return true;
    };
    let Ok((globals, _queue)) = registry_queue_init::<Probe>(&conn) else {
        return true;
    };
    globals.contents().with_list(|list| {
        list.iter()
            .any(|global| global.interface == "zxdg_decoration_manager_v1")
    })
}
