#!/usr/bin/env python3
"""Generate crates/app/src/themes.rs from Ghostty's bundled theme files.

Ghostty ships the iTerm2-Color-Schemes collection (MIT; colors from each
theme's authors) in /usr/share/ghostty/themes. To add a theme, add its file
name to THEMES and rerun:  python3 scripts/gen-themes.py
"""
import pathlib
import sys

SRC = pathlib.Path("/usr/share/ghostty/themes")
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/app/src/themes.rs"

# (Ghostty file name, termden id, display name). Order is the settings order.
THEMES = [
    ("Catppuccin Mocha", "catppuccin-mocha", "Catppuccin Mocha"),
    ("Catppuccin Macchiato", "catppuccin-macchiato", "Catppuccin Macchiato"),
    ("Catppuccin Frappe", "catppuccin-frappe", "Catppuccin Frappé"),
    ("Everforest Dark Med", "everforest", "Everforest"),
    ("Everforest Dark Hard", "everforest-hard", "Everforest Hard"),
    ("TokyoNight Night", "tokyo-night", "Tokyo Night"),
    ("TokyoNight Storm", "tokyo-night-storm", "Tokyo Night Storm"),
    ("TokyoNight Moon", "tokyo-night-moon", "Tokyo Night Moon"),
    ("Gruvbox Dark", "gruvbox-dark", "Gruvbox Dark"),
    ("Gruvbox Material Dark", "gruvbox-material-dark", "Gruvbox Material"),
    ("Nord", "nord", "Nord"),
    ("Dracula", "dracula", "Dracula"),
    ("Rose Pine", "rose-pine", "Rosé Pine"),
    ("Rose Pine Moon", "rose-pine-moon", "Rosé Pine Moon"),
    ("Kanagawa Wave", "kanagawa-wave", "Kanagawa Wave"),
    ("Kanagawa Dragon", "kanagawa-dragon", "Kanagawa Dragon"),
    ("Atom One Dark", "one-dark", "One Dark"),
    ("GitHub Dark Default", "github-dark", "GitHub Dark"),
    ("GitHub Dark Dimmed", "github-dark-dimmed", "GitHub Dark Dimmed"),
    ("iTerm2 Solarized Dark", "solarized-dark", "Solarized Dark"),
    ("Ayu", "ayu-dark", "Ayu Dark"),
    ("Ayu Mirage", "ayu-mirage", "Ayu Mirage"),
    ("Nightfox", "nightfox", "Nightfox"),
    ("Carbonfox", "carbonfox", "Carbonfox"),
    ("Monokai Pro", "monokai-pro", "Monokai Pro"),
    ("Night Owl", "night-owl", "Night Owl"),
    ("Oxocarbon", "oxocarbon", "Oxocarbon"),
    ("Poimandres", "poimandres", "Poimandres"),
    ("Vesper", "vesper", "Vesper"),
    ("Sonokai", "sonokai", "Sonokai"),
    ("Flexoki Dark", "flexoki-dark", "Flexoki Dark"),
    ("Modus Vivendi", "modus-vivendi", "Modus Vivendi"),
    ("Melange Dark", "melange-dark", "Melange Dark"),
    ("Iceberg Dark", "iceberg-dark", "Iceberg Dark"),
    ("Material Ocean", "material-ocean", "Material Ocean"),
    ("Tomorrow Night", "tomorrow-night", "Tomorrow Night"),
    ("Catppuccin Latte", "catppuccin-latte", "Catppuccin Latte"),
    ("TokyoNight Day", "tokyo-night-day", "Tokyo Night Day"),
    ("Gruvbox Light", "gruvbox-light", "Gruvbox Light"),
    ("Everforest Light Med", "everforest-light", "Everforest Light"),
    ("Rose Pine Dawn", "rose-pine-dawn", "Rosé Pine Dawn"),
    ("Kanagawa Lotus", "kanagawa-lotus", "Kanagawa Lotus"),
    ("One Half Light", "one-light", "One Light"),
    ("GitHub Light Default", "github-light", "GitHub Light"),
    ("iTerm2 Solarized Light", "solarized-light", "Solarized Light"),
    ("Ayu Light", "ayu-light", "Ayu Light"),
    ("Dayfox", "dayfox", "Dayfox"),
    ("Flexoki Light", "flexoki-light", "Flexoki Light"),
    ("Modus Operandi", "modus-operandi", "Modus Operandi"),
    ("Melange Light", "melange-light", "Melange Light"),
    ("Monokai Pro Light", "monokai-pro-light", "Monokai Pro Light"),
]


def hex6(value: str) -> str:
    value = value.strip().lstrip("#")
    if len(value) != 6:
        raise ValueError(f"not a #rrggbb color: {value}")
    return "0x" + value.lower()


def parse(path: pathlib.Path) -> dict:
    ansi, keys = [None] * 16, {}
    for line in path.read_text().splitlines():
        if "=" not in line or line.lstrip().startswith("#"):
            continue
        key, value = (part.strip() for part in line.split("=", 1))
        if key == "palette":
            index, color = value.split("=", 1)
            ansi[int(index)] = hex6(color)
        else:
            keys[key] = value
    if None in ansi:
        raise ValueError(f"{path.name}: incomplete palette")
    return {
        "ansi": ansi,
        "bg": hex6(keys["background"]),
        "fg": hex6(keys["foreground"]),
        "cursor": hex6(keys.get("cursor-color", keys["foreground"])),
    }


def luminance(hex_value: str) -> float:
    v = int(hex_value, 16)
    r, g, b = (v >> 16) & 255, (v >> 8) & 255, v & 255
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255


def main() -> int:
    rows, ids = [], set()
    for file, theme_id, name in THEMES:
        assert theme_id not in ids, theme_id
        ids.add(theme_id)
        t = parse(SRC / file)
        dark = "true" if luminance(t["bg"]) < 0.5 else "false"
        ansi = ", ".join(t["ansi"])
        rows.append(
            f'    ThemeDef {{\n        id: "{theme_id}",\n        name: "{name}",\n'
            f"        dark: {dark},\n        bg: {t['bg']},\n        fg: {t['fg']},\n"
            f"        cursor: {t['cursor']},\n"
            f"        ansi: [{ansi}],\n    }},"
        )
    OUT.write_text(
        "//! Generated by scripts/gen-themes.py from Ghostty's theme files\n"
        "//! (iTerm2-Color-Schemes, MIT). Do not edit by hand; rerun the script.\n\n"
        "/// A theme as published: terminal palette plus background/foreground.\n"
        "pub struct ThemeDef {\n"
        "    pub id: &'static str,\n    pub name: &'static str,\n    pub dark: bool,\n"
        "    pub bg: u32,\n    pub fg: u32,\n    pub cursor: u32,\n"
        "    pub ansi: [u32; 16],\n}\n\n"
        f"pub const THEMES: [ThemeDef; {len(rows)}] = [\n" + "\n".join(rows) + "\n];\n"
    )
    print(f"wrote {len(rows)} themes to {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
