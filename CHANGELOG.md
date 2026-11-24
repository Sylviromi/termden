# Changelog

What changed in each release, for people using termden. The release workflow
publishes the section for a tag as its release notes, and the in-app update
popup shows it: write a `## vX.Y.Z` section before tagging (CI refuses tags
without one). Keep it to what users notice, one line each.

## v0.4.0

- Activity is now a screen: a resizable tile in the canvas you can drag, zoom and close, shaped like a group chat. Messages between agents are collapsed bubbles you click to read, and shared results carry an "open results" link; app and workspace notices are left out
- Stats is a tile too, opened per workspace beside the focused tile instead of a fixed strip on the right
- A Home workspace with its own agent, a right-click menu on workspaces, and workspaces you can put away (unload) without stopping their agents
- `termdenctl activity on|off`

## v0.3.0

- New Sidebar setting: Pinned (as before) or Auto-hide, which hides the sidebar until you move the pointer to the left screen edge and slides it out over the tiles
- Desktop notification when an agent needs input or shares results, with the termden icon, grouped under its desktop entry
- Agents keep the same color across restarts
- A divider between the ACTIVITY and STATS toggles

## v0.2.0

- The activity feed collapses: ACTIVITY and STATS toggles now sit in the sidebar footer (the "working / harnesses" tally is gone), and the choice is remembered
- Rounded outer window corners under GNOME (square when maximized, snapped or fullscreen)
- Removing a workspace takes a second click within 3 seconds instead of an inline confirm row
- New workspaces start empty instead of with a shell tile
- Agents no longer get `termden_ask_user_choice`: they use their own question tool, and `termden_show_results` still shows images and files in your browser
- The activity feed says "shared N results" without listing the titles

## v0.1.7

- 51 colour themes, dark and light: click the theme in Settings to open a searchable panel of all of them
- Agents can show you what they made (images, screenshots, videos, files) as a gallery in your browser
- termden never restarts on its own any more: new builds and agents only offer a Restart in the sidebar
- Updates now appear as a button next to Settings, with what's new, a progress bar, and a restart you confirm
- F11 toggles fullscreen
- Smooth scrolling in the activity feed and the theme list
- The activity feed no longer jumps to the bottom while you are reading older entries
- Fixed: collapsed workspaces showed an arrow pointing the wrong way; empty workspaces lacked spacing

## v0.1.6

- Window buttons now show on maximized windows under GNOME
- `termdenctl state` reports title bar details, to help with bug reports

## v0.1.5

- Smaller download: release binaries are stripped

## v0.1.4

- Fonts and icons are built in: termden no longer crashes on machines that lack them
- New Title bar setting (Auto, System, None); on desktops that draw no title bar, termden adds its own minimize, maximize and close buttons and resizable edges

## v0.1.3

- `cargo binstall termden` installs the prebuilt binary without compiling
- Building from source explains which system packages are missing

## v0.1.2

- `termdenctl` now ships inside the `termden` package

## v0.1.1

- Much lower CPU use while idle

## v0.1.0

- First release
