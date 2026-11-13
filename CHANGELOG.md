# Changelog

What changed in each release, for people using termden. The release workflow
publishes the section for a tag as its release notes, and the in-app update
popup shows it: write a `## vX.Y.Z` section before tagging (CI refuses tags
without one). Keep it to what users notice, one line each.

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
