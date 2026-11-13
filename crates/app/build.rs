//! Fail fast, with install commands, when a system library termden (through
//! GPUI) links against is missing. Otherwise the build runs for minutes and
//! ends in a bare `cannot find -lxkbcommon-x11` from the linker.

use std::process::Command;

/// pkg-config name, then the dev package on Debian/Ubuntu, Fedora and Arch.
const LIBS: [[&str; 4]; 4] = [
    [
        "xkbcommon",
        "libxkbcommon-dev",
        "libxkbcommon-devel",
        "libxkbcommon",
    ],
    [
        "xkbcommon-x11",
        "libxkbcommon-x11-dev",
        "libxkbcommon-x11-devel",
        "libxkbcommon-x11",
    ],
    ["xcb", "libxcb1-dev", "libxcb-devel", "libxcb"],
    ["xcb-xkb", "libxcb-xkb-dev", "libxcb-devel", "libxcb"],
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_LIBDIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    if Command::new("pkg-config")
        .arg("--version")
        .output()
        .is_err()
    {
        println!(
            "cargo:warning=pkg-config not found; cannot check for the libxkbcommon/libxcb dev packages termden links against"
        );
        return;
    }
    let missing: Vec<_> = LIBS
        .iter()
        .filter(|lib| {
            pkg_config::Config::new()
                .cargo_metadata(false)
                .probe(lib[0])
                .is_err()
        })
        .collect();
    if missing.is_empty() {
        return;
    }
    let list = |column: usize| {
        let mut names: Vec<&str> = missing.iter().map(|lib| lib[column]).collect();
        names.dedup();
        names.join(" ")
    };
    panic!(
        "\n\ntermden links against system libraries whose development files are missing: {}\n\
         Install them, then build again:\n  \
         Debian/Ubuntu: sudo apt install {}\n  \
         Fedora:        sudo dnf install {}\n  \
         Arch:          sudo pacman -S {}\n\
         Or skip compiling entirely: cargo binstall termden\n\n",
        list(0),
        list(1),
        list(2),
        list(3),
    );
}
