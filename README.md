# termden

A workspace and harness control surface for terminal coding agents.

> Agent-facing conventions, commands and gotchas live in [AGENTS.md](AGENTS.md).

`termden` is one borderless GPUI window: a Hyprland-flavoured panel on the left
listing workspaces and their harnesses, and a dwindle-tiled terminal canvas on
the right. A *workspace* is a named view over a folder (two workspaces may point
at the same folder, or at subfolders). A *harness* is a real terminal process —
Claude Code, opencode, pi, or any command.

Every harness is a real terminal: each runs in a session on a private tmux
server (`tmux -L termden`, session `td-<harness id>`) and is rendered through
`alacritty_terminal`. Closing or reloading termden only detaches; reopening
reattaches. Workspaces, layouts and focus persist in
`$XDG_STATE_HOME/termden/state.json`. The mock fleet lives behind `--demo`.

## Run

```sh
cd /mnt/Projects/termden
cargo run -p termden-app
```

## Developing termden inside termden

```sh
scripts/dev.sh            # in a termden shell tile: rebuild on every save
scripts/dev.sh --launch   # same, and start termden if it is not running
```

A running termden watches its own binary. When a build replaces it, termden
saves state, detaches from tmux and execs the new build, so every harness
(including the agent editing the code) survives. `ctrl+shift+r` reloads by hand.

`termdenctl` talks to the running app over `$XDG_RUNTIME_DIR/termden/app.sock`.
Every harness has it on `PATH` and knows its own id (`$TERMDEN_HARNESS`, or
just `self`). Harnesses and workspaces are named by id or name:

```sh
termdenctl state                      # JSON: window, workspaces, harnesses
termdenctl screen [h]                 # visible text of a tile (default: focused)
termdenctl history <h> [lines]        # scrollback via tmux
termdenctl add claude|opencode|pi|shell [ws]   # prints the new id
termdenctl wait <h>                   # until it stops printing (after add, too)
termdenctl send --enter <h> 'cargo test'       # type into it; key <h> ctrl-c
termdenctl close|restart|focus <h> | rename <h> <name>
termdenctl add-workspace <dir> | activate <ws> | rename-workspace <ws> <name>
termdenctl log "note"                 # to the activity feed, as `self`
termdenctl screenshot [out.png]       # headless mirror of the live app
termdenctl screenshot --live          # grim grab of the real window
termdenctl theme | reload | ping
```

`dev.sh` posts failed builds to the activity feed. Agents can drive and read
tiles but cannot move, resize or zoom them; layout is yours.

### Shortcuts (ctrl+shift + …)

| key | action |
| --- | --- |
| `T` | new shell (`+` on a workspace picks an agent) |
| arrows / `hjkl` | focus the neighbouring tile |
| `Z` or double-click a header | zoom the tile to the whole canvas |
| `W` | close the tile (press twice; kills its process) |
| `PgUp` / `PgDn` | previous / next workspace |
| `V` | paste (drag in a tile selects; release copies) |
| `+` `-` `0` | terminal text size |
| `R` | reload termden |

Drag a tile header to split or swap; drag the gap between tiles to resize.
Enter on an exited tile restarts it.

### App launcher

```sh
install -Dm644 assets/termden.desktop ~/.local/share/applications/termden.desktop
install -Dm644 assets/termden.svg ~/.local/share/icons/hicolor/scalable/apps/termden.svg
```

The entry runs `scripts/launch.sh`: it focuses a running termden (Hyprland),
otherwise starts the dev build, which reloads itself on every rebuild.

Requires `tmux`, `inotify-tools` for `dev.sh`, a working Vulkan driver, Wayland or X11, and the fonts named in the
config (`Noto Sans`, `JetBrainsMono Nerd Font` by default).

## Headless screenshots

No display needed, and it never touches your session:

```sh
./scripts/headless-shot.sh [out.png] [width] [height]
```

It builds the app, starts a private Weston compositor on the headless backend
(kiosk shell, no panel or decorations), runs termden against the software
Vulkan driver, and captures the output with `weston-screenshooter`. Default
output: `target/termden-headless.png`.

GPUI's own `Application::headless()` is not enough: on Linux it deliberately
refuses to open windows, so there is nothing to capture. The script uses a
virtual compositor instead. It also generates a transparent Xcursor theme so
the host's cursor art cannot leak into captures.

Requires `weston`, `weston-screenshooter`, `python3`, and ideally the lavapipe
Vulkan ICD (`lvp_icd.json`).

## What works right now

- Live harnesses of every kind, added with `+` on a workspace or
  `ctrl+shift+t`; close (confirmed), restart and zoom on each tile.
- Resizable splits, keyboard focus movement, adjustable terminal text size.
- Mouse, scroll and selection via tmux; drag to copy, `ctrl+shift+v` pastes.
- `--demo`: three mock workspaces, eight harnesses, live subagent rows.
- Click a workspace to activate it, the chevron to collapse it.
- Click a harness (panel or tile) to focus its tile with the workspace accent.
- Drag a tile by its header: edge zones split, centre swaps. The dwindle tree
  mutates for real, in memory.
- The settings popup (gear in the panel header, or ctrl+shift+,) switches Catppuccin Mocha ↔ Everforest, sets text size, lists shortcuts, and persists the
  choice to `~/.config/termden/config.toml`.
- Tiles resize their tmux session; typing goes to the focused tile.
  `tmux -L termden ls` lists them; `tmux -L termden kill-server` resets them.
- Borderless window; drag the logo/wordmark to move it.

Not wired yet: agent adapters (status comes from terminal activity), the
daemon, MCP, agent messaging.

## Configuration

`$XDG_CONFIG_HOME/termden/config.toml` (defaults shown):

```toml
theme = "catppuccin-mocha"   # or "everforest"
ui_font = "Noto Sans"
mono_font = "JetBrainsMono Nerd Font"
font_size = 16.0             # terminal text; ctrl+shift +/-/0
panel_width = 268.0
gap = 12.0
corner_radius = 12.0
```

The settings popup writes the same file. Icons use the system font "Material Symbols Rounded".

## Layout

```
crates/
  core/    domain types, dwindle tree, config — no GPUI dependency
  app/     GPUI front end (this skeleton)
    theme.rs    palette + fonts, Catppuccin and Everforest
    mock.rs     the demo fleet (`--demo`)
    state.rs    app state, sessions, input, frame ticker
    actions.rs  workspace/harness CRUD, persistence, hot reload
    control.rs  control socket for `termdenctl`
    pty.rs      tmux client PTY, alacritty Term, key map
    panel.rs    workspace/harness/subagent tree
    canvas.rs   canvas rect + dwindle geometry
    tile.rs     tile chrome and drop zones
    terminal.rs terminal grid renderer + demo buffers
    widgets.rs  dots, badges, pills, spinner
  daemon/  stub — will own tmux sessions, PTYs, socket, MCP
  cli/     `termdenctl` for humans and agents
```

`core` holds no GPUI types so the daemon, CLI and MCP server can reuse the same
model and layout tree.

## Design decisions

- **Terminal strategy:** daemon-owned tmux sessions on a private server
  (`tmux -L termden`), rendered by an embedded terminal surface. `kitty attach`
  remains available for the real thing. Wayland cannot embed a foreign kitty
  window inside a GPUI window, so this is the closest honest equivalent.
- **GPUI 0.2.2 only.** `gpui-component` 0.9 is built on `gpui-pre` 0.3.11, a
  separate pinned Zed snapshot. The design system here is intentionally local.
- **Messaging:** hybrid. Messages land in a per-agent mailbox; an idle harness
  gets a short notice and the agent pulls the full text through an MCP tool.
- **Agent permissions:** agents may spawn/kill harnesses in their own
  workspace, create workspaces under an allowed root, and message anyone.
  They may **not** move or resize tiles.
- **Themes:** palette plus metrics are resolved once at startup; switching
  rebuilds the theme from config.

## Hyprland

The window declares `app_id = "termden"` and requests client-side decorations
with no titlebar, so it is already borderless. Optional rice:

```conf
windowrulev2 = rounding 12, class:^(termden)$
windowrulev2 = noblur, class:^(termden)$
```

The app paints its own 12px outer gap, panel radius and tile borders.

## Roadmap

1. **Terminal + daemon** — `portable-pty` + `alacritty_terminal`, tmux on a
   private socket, reattach after restart, `window-size latest` for kitty
   attach.
2. **Adapters** — Claude Code launched with our `--settings` hooks; opencode
   with our config and SSE events. Subagent trees, unread counts, status.
3. **Control plane** — JSON-RPC over `$XDG_RUNTIME_DIR/termden/sock`,
   `termdenctl`, MCP tools, hybrid mailbox.
4. **Polish** — command palette, persistence, optional translucent/blurred
   window background.
