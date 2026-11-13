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

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Sylviromi/termden/main/scripts/install.sh | sh
```

The script installs `termden` and `termdenctl` into `~/.local/bin`, plus a launcher entry
and icon; no root and no Rust toolchain. Linux x86_64 (Wayland or X11, glibc
2.35+). It also needs `tmux`, a Vulkan driver and `xdg-open`; the installer
lists whatever is missing. Fonts are built in. `sh install.sh --uninstall` removes it again.

On Arch (AUR):

```sh
paru -S termden-bin        # or yay -S termden-bin; updates come with -Syu
```

With Rust, without compiling ([cargo-binstall](https://github.com/cargo-bins/cargo-binstall)
fetches the release binary):

```sh
cargo binstall termden            # installs termden and termdenctl
```

With Rust (1.90+), building from source:

```sh
cargo install --locked termden    # installs termden and termdenctl
```

This compiles C and C++ code from GPUI's dependencies, so it needs a C/C++
toolchain and a few dev packages first:

- Arch: `sudo pacman -S --needed base-devel pkgconf libxkbcommon-x11 wayland vulkan-icd-loader freetype2 fontconfig`
- Debian/Ubuntu: `sudo apt install build-essential pkg-config clang libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev libxcb1-dev libxcb-xkb-dev`
- Fedora: `sudo dnf install gcc gcc-c++ clang pkgconf-pkg-config libxkbcommon-x11-devel wayland-devel vulkan-loader-devel fontconfig-devel freetype-devel libxcb-devel`

`failed to find tool "c++"` means the C++ compiler is missing (`build-essential`,
`gcc-c++` or `base-devel`). `linking with cc failed` with `cannot find -lxyz`
in its notes means that library's dev package is missing (on Debian/Ubuntu
`-lxcb-xkb` is `libxcb-xkb-dev`, `-lxkbcommon-x11` is `libxkbcommon-x11-dev`). Updates arrive the same way as below (or re-run the command).

**Updates:** termden checks GitHub for a new release every few hours. When one
is out, a download button appears next to Settings: it shows what's new
(`CHANGELOG.md`), installs with a progress bar (or run `termdenctl update`),
then asks before restarting into it; every tile keeps running. Re-running the installer works too.
Launching termden again while it runs focuses the existing window (Hyprland).

## Build from source

```sh
git clone https://github.com/Sylviromi/termden && cd termden
git config core.hooksPath scripts/git-hooks   # agent-commit sign-off hook
cargo run -p termden
```

Source builds never self-update (rebuild instead; see below).

## Developing termden inside termden

```sh
scripts/dev.sh            # build and start termden, or focus it (launcher entry)
cargo build -p termden    # after editing: builds termden + termdenctl
```

A running termden notices when a build replaces its binary and shows
**New build ready · Restart** in the sidebar. It never restarts by itself:
click Restart (or ctrl+shift+r) when it suits you. Restarting saves state,
detaches from tmux and execs the new build, so every harness, including the
agent editing the code, survives. Agents' `termdenctl reload` only offers the
restart.

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

Agents can drive and read
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
| `R` | restart termden (into a new build, when the banner offers one) |

Drag a tile header to split or swap; drag the gap between tiles to resize.
Enter on an exited tile restarts it.

### Releasing

1. Write the `## v0.2.0` section in `CHANGELOG.md` (user-facing, one line
   per change); it becomes the release notes and the update popup's text.
2. Bump `version` in the root `Cargo.toml`, and the `termden-core` version in
   `[workspace.dependencies]` to match; commit.
3. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
   `.github/workflows/release.yml` checks the changelog section, tests, builds
   on Ubuntu 22.04 and publishes `termden-x86_64-linux.tar.gz`; installed
   copies show a download button next to Settings.
4. crates.io: `cargo publish --workspace`.
5. AUR: in `packaging/aur/termden-bin`, set `pkgver`, run `updpkgsums` and
   `makepkg --printsrcinfo > .SRCINFO`, then push both to the AUR repo.

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
- The settings popup (gear in the panel header, or ctrl+shift+,) picks one of 51 themes (Catppuccin, Tokyo Night, Gruvbox, Nord, Dracula, Rosé Pine, Kanagawa, GitHub, Solarized, … dark and light), sets text size, lists shortcuts, and persists the
  choice to `~/.config/termden/config.toml`.
- Tiles resize their tmux session; typing goes to the focused tile.
  `tmux -L termden ls` lists them; `tmux -L termden kill-server` resets them.
- Borderless window; drag the logo/wordmark to move it.

- Ctrl+click an http(s) link in a tile to open it in your browser.

Not wired yet: agent adapters (status comes from terminal activity) and the
daemon.

## Agents

Claude Code and opencode tiles started by termden get a `termden` MCP server
(per-process config; your global settings are untouched):

- `termden_agents`, `termden_read_screen`: see the other tiles.
- `termden_send_mail`, `termden_read_mail`: mailboxes. An idle agent gets a
  one-line `📬 sender: preview…` typed into its prompt, never over your draft.
- `termden_set_name|tag|pronouns|title`: identity shown in the sidebar.
- `termden_ask_user_choice`: shows you 2-8 options (with images) on a local
  web page in termden's colours. The agent posts the link; Ctrl+click it,
  pick, and the answer is mailed back. The page lives on `127.0.0.1` behind a
  random token kept beside the control socket.
- `termden_show_results`: shows you what an agent made or captured (images,
  screenshots, videos, audio, PDFs, files) as a gallery on the same local
  server, view only. The agent posts the link; `…/r/` lists the last 20 sets.

## Configuration

`$XDG_CONFIG_HOME/termden/config.toml` (defaults shown):

```toml
theme = "catppuccin-mocha"   # `termdenctl theme list` shows all ids
ui_font = "Noto Sans"
mono_font = "JetBrainsMono Nerd Font"
font_size = 16.0             # terminal text; ctrl+shift +/-/0
panel_width = 268.0
gap = 12.0
corner_radius = 12.0
title_bar = "auto"           # or "system" / "none"; settings popup too
```

`title_bar = "auto"` shows no title bar under tiling window managers
(Hyprland, Sway, i3, niri) and the desktop's own everywhere else. Desktops
that never draw one for apps (GNOME on Wayland) get termden's own minimize,
maximize and close buttons in the panel header, and resizable window edges.

The settings popup writes the same file. The default fonts and the icon font
are bundled in the binary (`crates/app/assets/fonts`); any installed font can
be named here instead.

## Layout

```
crates/
  core/    domain types, dwindle tree, config — no GPUI dependency
  app/     package `termden`: the app, plus `termdenctl` (src/bin) for
           humans and agents; GPUI front end
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

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
