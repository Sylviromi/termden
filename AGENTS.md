# AGENTS.md

Instructions for anyone (human or agent) changing **termden**.

Termden is a single-window GPUI control surface for terminal coding agents:
workspaces on the left, a dwindle-tiled terminal canvas on the right. A
*workspace* is a named view over a folder (two workspaces may share a folder, or
point at subfolders). A *harness* is one terminal process running Claude Code,
opencode, pi, or a plain shell.

**Current state: self-hosting.** Every harness is a live terminal (tmux
client in a PTY, parsed by `alacritty_terminal`). State persists in
`$XDG_STATE_HOME/termden/state.json`; the mock fleet is `--demo`. The app
offers a restart when its binary is rebuilt and exposes a control socket for
`termdenctl`. No adapters or daemon yet (a future `termdend` would own the sessions).

## Working on termden from inside termden

You are probably running in a termden tile. Then `termdenctl` is on `PATH`,
`$TERMDEN_HARNESS` is your harness id (`self` in commands), and
`termdenctl help` lists everything. The loop:

1. Edit code, then `cargo build -p termden` (builds `termden` and
   `termdenctl`). The running app notices the new binary and shows a
   "New build ready · Restart" banner; **it never restarts by itself**, the
   user confirms (banner or ctrl+shift+r). `termdenctl reload` only offers the
   restart too. Tell the user a restart is waiting when your change needs it.
   Restarting execs the new build; your own process survives (tmux).
2. Check the result without touching the desktop:
   `termdenctl screenshot target/now.png` (headless mirror of the live app),
   `termdenctl state`, `termdenctl screen <h>`, `termdenctl history <h> 300`.
3. Test UI-affecting changes in a throwaway instance, never the user's:
   `TERMDEN_SHOT_MODE=fresh TERMDEN_SHOT_SETUP='termdenctl add shell; ...'
   ./scripts/headless-shot.sh` (own state, socket and tmux server). Run it
   under `systemd-run --user --scope -p MemoryMax=4G` if it spawns agents.
4. Other tiles are drivable: `add`, `wait`, `send --enter`, `key`, `close`.
   Do not close or restart tiles you did not create without being asked.

## Non-negotiables

1. **Never open a GUI window on the user's desktop during automated work.**
   Use `./scripts/headless-shot.sh` to render and inspect. The user works in
   the same Hyprland session; popping windows over it is disruptive.
2. **Do not remove the `libc = "=0.2.189"` pin** in `crates/app/Cargo.toml`
   without reproducing the dependency issue (see Gotchas).
3. **Do not add `gpui-component` / gpui-kit.** It targets `gpui-pre 0.3.11`, a
   different pinned Zed snapshot, and is incompatible with `gpui 0.2.2`.
4. **Keep `termden-core` free of GPUI.** It is shared by the daemon, CLI and
   (eventually) the MCP server.
5. **No global dotfile edits when launching harnesses.** Use per-process
   configuration (`claude --settings <file>`, `OPENCODE_CONFIG`, env vars).
6. **Agents may not move or resize tiles.** That is a deliberate permission
   boundary in the product design, not a UI limitation.
7. **Tautological labels, tests, and UI components are considered harmful.**

## Git

The repo uses `scripts/git-hooks` (`git config core.hooksPath scripts/git-hooks`,
set per repo, never global). Commits made from an agent tile are **refused**
until the agent has its own name (not "Claude 2", "opencode 2"...) and a tag
(`termden_set_name`, `termden_set_tag`). The `commit-msg` hook then appends
`> Written by <name> [<tag>].` as the last line, so write clean
`type(scope): summary` subjects and never add your own sign-off or
`Co-Authored-By`. Humans and plain shell tiles commit as usual. Commit only your
own files (`git add <paths>`), never `git add -A` while other agents are editing.

## Commands

```sh
cargo run -p termden          # real window (only when the user asks)
cargo check --workspace
cargo fmt
cargo build -p termden

scripts/dev.sh                    # build + start termden (or focus it); launcher entry
termdenctl state|screen [h]|add <kind>|reload|screenshot [out.png]
termdenctl stats [on|off]         # tokens/MCP/uptime JSON, or show the pane

# Headless render + capture; touches no real display:
./scripts/headless-shot.sh [out.png] [width] [height]
# default: target/termden-headless.png, 1440x900
```

Headless capture requirements: `weston`, `weston-screenshooter`, `python3`, and
preferably the lavapipe Vulkan ICD (`/usr/share/vulkan/icd.d/lvp_icd.json`). The
script always rebuilds first, so it doubles as the visual test.

After a capture, inspect the PNG for clipped text, missing glyphs, stray cursor
sprites, and tile overflow. Do not ship a visual change without a headless shot.

## Layout

```
Cargo.toml                 workspace, gpui 0.2.2, libc pin
crates/core/               no GPUI: ids, Harness/Workspace/Subagent, dwindle
                           tree (LayoutNode), Config + ThemeId, stats readers
crates/app/                package `termden`: GPUI binary `termden` + `termdenctl`
  main.rs                  Application::new(), borderless WindowOptions
  state.rs                 TermdenApp, sessions, input, 120ms frame ticker
  actions.rs               workspace/harness CRUD, persistence, hot reload
  control.rs               control socket (`termdenctl` commands; `HELP`)
  stats.rs                 per-workspace token/MCP/uptime cache (background)
                           and the right-hand stats pane
  theme.rs                 Catppuccin Mocha + Everforest palettes/fonts
  mock.rs                  the demo fleet (3 workspaces, 8 harnesses)
  panel.rs                 workspace tree, tips, footer
  activity.rs              activity screen: a canvas tile (layout leaf
                           `HarnessId::ACTIVITY`), mail bubbles, result links
  canvas.rs                canvas_rect, layout_rects (dwindle geometry)
  tile.rs                  tile chrome, drop zones, per-tile status bar
  terminal.rs              live grid renderer + mock ANSI buffers per kind
  pty.rs                   Session: tmux client PTY, alacritty Term, key map
  mail.rs                  mailboxes: send/read, idle "you have mail" nudge
  choices.rs               localhost HTTP server (token URL) and `document()`
                           page wrapper in theme colours
  showcase.rs              show_results: view-only gallery pages under `r/`
                           on the same server; files sandboxed (CSP)
  launch.rs                per-harness claude/opencode config (MCP + prompt)
  update.rs                GitHub release check, header button + popup
                           (notes from CHANGELOG.md, progress, confirm
                           restart); installed copies only.
                           TERMDEN_UPDATE_URL points it at a test JSON
  widgets.rs               dots, badges, pills, spinner, icon buttons
  bin/termdenctl/          second binary of the same package: socket client,
                           screenshot, `mail`, `mcp` stdio server
scripts/headless-shot.sh    private Weston compositor + capture
scripts/install.sh          curl | sh installer for the latest release
.github/workflows/release.yml  tag v* -> release tarball
assets/config.example.toml  sample config
```

## Architecture rules

### Geometry

- Panel width, gap and radii come from `Config` (defaults 268 / 12 / 12).
- `canvas::canvas_rect(config, viewport, inset)` is the single source of truth
  for where the tiled area starts. `inset` is room reserved on the right (the
  stats pane plus its gap, see `TermdenApp::canvas_rect`); rendering, focus,
  hit-testing and `visible_tiles` all go through it. Tiles are positioned
  absolutely in that space; `canvas::layout_rects` recursively splits it using
  `LayoutNode`.
- Drop hit-testing uses the same rects as rendering. If you change gaps or
  panel width, both the drawing and `TermdenApp::hit_test` move together
  automatically — do not introduce a second calculation.
- Keep a minimum tile size (~90px) in `layout_rects` when changing ratios.

### Rendering

- Render helpers that take `&mut Context<T>` must return `AnyElement`
  (`div()....into_any_element()`), **not** `impl IntoElement`. In edition 2024,
  RPIT captures every in-scope lifetime, including `&mut Context`, which causes
  "captured variable cannot escape `FnMut` closure body" errors when a helper is
  used inside `.map(...)`. If you prefer `impl Trait`, use precise capturing
  (`impl IntoElement + use<>`).
- `widgets::icon_button` returns `Stateful<Div>` because `.id()` changes the
  type. Callers rely on that to chain `.on_click(...)`.
- All interactive elements need `.id(...)`, including plain `hover` styles.
- Terminal text uses NBSP for spaces (`.replace(' ', "\u{00A0}")`) so runs of
  spaces survive GPUI text layout. Keep `whitespace_nowrap()`.
- Terminal content is bottom-anchored (`justify_end()`), like a real TUI.
- One 120ms ticker in `state.rs` drives every spinner and cursor blink. Do not
  add per-widget timers or animation loops. Only exception:
  `widgets::SmoothScroll` (activity feed) requests animation frames while a
  scroll glides, since GPUI 0.2.2 jumps whole wheel steps. Ask for the next
  frame with `window.request_animation_frame()`; a `cx.notify()` during
  render is cleared when that frame ends, so the animation only advances on
  unrelated redraws (that made the theme panel glide lag).
- Palette colors live only in `theme.rs`. Do not hardcode hex values in panel,
  canvas or tile code; add a token instead.

### Live terminals

- `TermdenApp::sync_sessions` runs at the top of every render: it spawns a
  `pty::Session` for each visible shell tile and resizes grids from
  `tile::grid_size`, which mirrors the tile chrome heights. Change chrome
  heights there too.
- Each session is `env -u TMUX tmux -L $TERMDEN_TMUX_SOCKET new-session -A
  -s td-<id>`. Dropping a `Session` detaches only. The headless script sets a
  throwaway socket and kills that server on exit; keep it that way.
- PTY events arrive on an async channel per session; `PtyWrite` must be
  echoed back to the PTY (tmux relies on terminal query replies).
- `sync_sessions` attaches every harness, not just visible ones (hidden
  tiles are sized to their own layout), so control commands reach them.
  Mirror instances only `attach-session`; they must never create sessions.
- New sessions get `TERMDEN_HARNESS`, `TERMDEN_CONTROL_SOCKET` and a `PATH`
  starting with termden's own bin dir.
- Keys go to the focused harness via the root `FocusHandle`;
  `pty::key_bytes` is the xterm translation (unit-tested).
- Terminal colors resolve through `Theme::term_color` (16 ANSI slots built
  from palette tokens, xterm 256 cube computed).

### Stats

- The right-hand stats pane (`app/src/stats.rs`) shows plan limits on top
  (per provider account, so the same in every workspace), then the **active**
  workspace: total tokens with a cached/in/out split, one row per agent
  (tokens, context fill, uptime) and the MCP servers enabled for its tiles.
  Shells are not agents: they are left out of the pane, `termdenctl stats`
  and the activity feed (`log_harness` drops them). Everything but the header
  and the MCP block scrolls, so short windows still reach the agents. The
  sidebar footer's STATS button toggles it; the choice lives in
  `Config::stats_open`. Collection covers every workspace, so switching costs
  no wait.
- Plan limits are read, never guessed. Claude: the status line input
  (`rate_limits.five_hour` / `seven_day`, plus `context_window`), saved by
  `termdenctl statusline <dir>/status.json`, which termden sets as the tile's
  `statusLine` in its `--settings` and which then runs the user's own
  `statusLine` command (project, then user settings). The newest file across
  Claude tiles wins. opencode Go: `opencode api --standalone GET
  /api/experimental/usage/limits` (opencode holds the key), at most once a
  minute and only while an opencode tile exists. Never read provider
  credentials from termden.
- Numbers are read, never counted by termden: Claude Code transcripts
  (`~/.claude/projects/*/<session>.jsonl`, summed per assistant message id) and
  opencode's `opencode.db` token columns (`session_v2`, falling back to
  `session`). Shells and pi report none. `core/src/stats.rs` holds all parsing
  and stays GPUI-free.
- Claude's launch settings carry a `SessionStart` hook,
  `termdenctl session-hook <dir>/session.id`, which appends each new session
  id (start, `/clear`, `--resume`) to
  `$XDG_RUNTIME_DIR/termden/harness/<id>/session.id`. A tile's usage is the
  sum over every session it went through, so `/clear` does not erase what the
  cleared conversation cost. The hook must print nothing (its stdout reaches
  the agent's context). Subagent transcripts in `<session>/subagents/` count
  too, deduplicated by message id across files.
- opencode gets **no** session flag: given an unknown `--session`, its TUI
  ignores the id and resumes the previous session (which also reuses that
  session's instructions). Its sessions are found in `opencode.db` by the
  per-harness instructions path recorded in `instruction_state` +
  `instruction_blob` (`stats::opencode_sessions`), every match since the tmux
  session started, so `/new` adds one.
- Token split: "in" is uncached input plus cache writes
  (`Usage::fresh_input`), "cached" is cache reads, the big figure is
  everything. Claude's `input_tokens` alone is near zero with caching on.
- Collection runs on the background executor every 5s (`stats::watch`), never
  on the UI thread: a long Claude transcript is read incrementally by offset and
  deduplicated by message id. Uptime comes from one `tmux list-sessions -F
  '#{session_name} #{session_created}'` call and so survives app restarts.
- MCP names come from the per-process config termden writes, the project
  files the agent loads (`.mcp.json`, `opencode.json[c]`), `~/.claude.json`
  (user scope and the tile's project scope) and the `.mcp.json` / manifest of
  Claude plugins enabled in `~/.claude/settings.json`. claude.ai connectors and
  servers only `opencode mcp list` sees are not listed; do not claim the list
  is exhaustive.
- `termdenctl stats` returns the same data as JSON (`plans`, per-agent
  `context_percent`); `stats on|off` toggles the pane. Keep the JSON and the
  pane in sync.
- The pane reserves `stats::width(viewport) + gap` (260–360px) on the right through
  `TermdenApp::canvas_rect`; tiles are re-flowed, not covered.

### State and interaction

- Clicking a workspace activates it; the chevron toggles expansion (separate
  hit targets, no bubbling conflicts).
- Clicking a harness focuses its tile and expands its workspace.
- Ctrl+click on an http(s) URL in a tile opens it with `xdg-open` instead of
  clicking into tmux (`Session::url_at`, single row only).
- `visible_tiles` (zoom-aware) is what renders, hit-tests and owns the
  mouse; `layout_tiles` ignores zoom (focus movement, new splits). Split
  dividers come from the same `canvas::walk`.
- Agents get no control command that moves, resizes or zooms tiles.
- Closing a tile from the UI needs a second click/press within 3s.
- Dragging a tile header starts a move after an 8px threshold. Edge zones split;
  centre swaps. `LayoutNode::move_leaf` performs remove-then-split; do not
  duplicate leaves.
- The panel header (logo + wordmark) moves the borderless window via
  `window.start_window_move()`; double-click maximizes.
- `Config::title_bar` (auto/system/none) picks server or client decorations.
  GPUI reports "server" whenever it was asked for, even where the compositor
  cannot draw it (GNOME, Weston), so `decorations.rs` probes for
  xdg-decoration first. With client decorations, `own_chrome` adds
  minimize/maximize buttons, plus resize edges unless the window is tiled.
  GNOME reports maximized/snapped windows as tiled, so tiling must never hide
  the buttons. `termdenctl state` -> `window` shows these values; ask users
  for it. Weston's kiosk shell (headless-shot) tiles the window like a
  maximized GNOME one; `--shell=desktop` gives a floating one.
- Settings popup (theme, text size) writes `~/.config/termden/config.toml`. Icons are Material Symbols Rounded codepoints (`widgets::icons`), from a bundled subset: add new codepoints to it (`crates/app/assets/fonts/README.md`). Keep config fields
  serde-defaulted so old files keep working.

### Themes

51 themes (36 dark, 15 light) live in the generated `app/src/themes.rs`.
To add one: put its Ghostty theme file name, id and display name in
`THEMES` in `scripts/gen-themes.py` and rerun it (it reads
`/usr/share/ghostty/themes`, the iTerm2-Color-Schemes collection).

- `ThemeId` is the theme's id string; unknown ids fall back to the first.
- `theme::derive` builds the UI palette from background/foreground/ANSI:
  shades are mixes, accents come from ANSI and are pushed to readable
  contrast (`readable`); terminals use the theme's own 16 colors and cursor.
- Catppuccin Mocha and Everforest keep hand-tuned palettes (`for_id`).
- `theme::tests::every_theme_is_usable` checks ids, contrast and cycling;
  keep it passing. Keep the accent order: workspaces store an index into it.

## Gotchas (learned the hard way)

- **Headless shots must not recurse.** `termdenctl screenshot` runs
  `headless-shot.sh` itself. The script unsets `TERMDEN_SHOT_SETUP` after
  reading it; an inherited SETUP that calls `screenshot` once fork-bombed the
  machine (one claude + Weston + termden per level) until systemd-oomd killed
  the Hyprland session. Keep that unset, and run experiments that spawn agents
  under `systemd-run --user --scope -p MemoryMax=4G`.

- **libc pin:** `gpui 0.2.2 -> gpui_http_client -> zed-async-tar -> xattr 0.2.3`
  references `libc::ENOATTR`, which libc removed for Linux in 0.2.190. Removing
  the `=0.2.189` pin breaks the build with `ENOATTR` not found.
- **GPUI versions:** crates.io `gpui 0.2.2` is the stable snapshot. The
  `gpui-component 0.9` ecosystem is built on `gpui-pre 0.3.11`. They cannot be
  mixed.
- **Wayland cannot embed kitty.** Foreign-window embedding is unavailable, and
  clients cannot position themselves. That is why the product plan is
  daemon-owned tmux sessions rendered by an embedded terminal surface, with
  `kitty attach` as the escape hatch.
- **GPUI `Application::headless()` cannot open windows** on Linux, so it is
  useless for screenshots. The script uses a private Weston compositor instead.
- **Headless Weston needs all four:** `--fake-seat` (or GPUI panics on
  `seat.unwrap()`), the lavapipe ICD (or blade fails with
  `ERROR_SURFACE_LOST_KHR`), `--debug` (or `weston-screenshooter` reports
  `unauthorized`), and a transparent cursor theme (or host cursor art is
  captured).
- **`on_click` requires an id.** `div().id(...)` returns `Stateful<Div>`;
  returning `Div` from a helper after `.id()` will not compile.
- **GPUI defaults:** Wayland/X11/font-kit/blade are already default features.
  Default fonts (`Noto Sans`, `JetBrainsMono Nerd Font`) and the icon font are
  bundled and registered in `main.rs`; without them GPUI panics at startup
  ("failed to resolve font") on machines that lack them.

## Product constraints for the next phases

- **Sessions:** daemon owns `tmux -L termden` sessions so app/daemon restarts do
  not kill harnesses. Use `window-size latest` / `largest` for kitty attach.
- **Adapters:** Claude Code via `--settings` hooks; opencode via our config +
  its SSE event stream. Never edit `~/.claude` or the user's global opencode
  config.
- **Messaging:** hybrid. Every agent has a mailbox; an idle harness gets a short
  "you have mail" notice in its terminal, and the full text is pulled via an MCP
  tool. Cross-workspace messages are allowed but explicitly addressed. The
  notice is typed only into a prompt that is empty and has been quiet for a
  while (`Session::draft`, `Session::last_human`, `Session::prompt_is_empty`):
  the human's unsent draft must never be merged with a nudge or submitted by it,
  and the activity feed says when mail is being held for that reason.
- **Control plane:** unix socket at `$XDG_RUNTIME_DIR/termden/sock` (0700, peer
  credentials checked), JSON-RPC, wrapped by `termdenctl` and an MCP server.
- **Permissions:** agents may spawn/kill harnesses in their own workspace,
  create workspaces under an allowed root, and message anyone. They may not
  move/resize tiles; destructive workspace deletion needs UI confirmation.

## Roadmap

1. **Terminal + daemon** — done: `alacritty_terminal` PTY + tmux private
   server, reattach on restart, for shell tiles. Next: mouse/scroll and
   selection, spawning harnesses from the `+` button, moving session
   ownership into `termdend`.
2. **Adapters** — Claude Code hooks + transcript JSONL; opencode SSE + child
   sessions; subagent tree and unread counts.
3. **Control plane** — socket, `termdenctl`, MCP, hybrid mailbox: done for
   claude/opencode tiles started by termden (mail persisted in state.json;
   MCP tools `termden_agents|send_mail|read_mail|read_screen|set_name|set_tag|set_pronouns|set_title|show_results`; tiles already
   running keep their old config until restarted). Next: move the socket to
   `termdend`, JSON-RPC, SO_PEERCRED.
4. **Polish** — command palette, persistence, optional translucent/blurred
   background (`WindowBackgroundAppearance::Blurred` + Hyprland rules).

## Done checklist

- `cargo check --workspace` clean (the only acceptable warning is the
  `proc-macro-error2` future-incompat note from a dependency).
- `cargo fmt` applied.
- Headless screenshot inspected at 1440x900 and, if layout is involved, at
  980x600 (minimum window size).
- Stats pane: the demo fleet shows tokens, cache, MCPs and uptime at both
  sizes; `termdenctl stats` is non-null for a tile started after this change.
- Mock fleet still demonstrates every state: working, idle, needs-input, error,
  unread mail, live subagents.
- README / AGENTS.md updated when commands or conventions change.
