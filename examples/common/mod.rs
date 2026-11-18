//! Command-line flags shared by the examples. The event loop itself is
//! `ratagui::run_with`; an app normally just calls `ratagui::run(app)`.
//!
//! * `--pixel-mouse`: request pixel-precise mouse reports (mode 1016)
//! * `--protocol kitty|sixel|iterm2|halfblocks`: override the detected
//!   graphics protocol
//! * `--halfblocks`: same as `--protocol halfblocks`
//! * `--screenshot <file>`: render a few frames without a terminal and save
//!   the egui image as PNG (10x20 px font)
//!
//! On exit the examples print the protocol and font size they used.

use std::io;

pub use ratagui::App;
use ratagui::{ProtocolType, RunOptions};

pub fn run(mut app: impl App) -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let value_of = |flag: &str| {
        let i = args.iter().position(|a| a == flag)?;
        Some(args.get(i + 1).unwrap_or_else(|| panic!("{flag} needs a value")).as_str())
    };

    if let Some(path) = value_of("--screenshot") {
        return screenshot(&mut app, path);
    }
    let protocol = match value_of("--protocol") {
        Some("kitty") => Some(ProtocolType::Kitty),
        Some("sixel") => Some(ProtocolType::Sixel),
        Some("iterm2") => Some(ProtocolType::Iterm2),
        Some("halfblocks") => Some(ProtocolType::Halfblocks),
        Some(other) => panic!("--protocol kitty|sixel|iterm2|halfblocks, got {other:?}"),
        None if args.iter().any(|a| a == "--halfblocks") => Some(ProtocolType::Halfblocks),
        None => None,
    };
    ratagui::run_with(
        app,
        RunOptions {
            pixel_mouse: args.iter().any(|a| a == "--pixel-mouse"),
            protocol,
            report: true,
            ..Default::default()
        },
    )
}

fn screenshot(app: &mut impl App, path: &str) -> io::Result<()> {
    let shot = ratagui::render_headless(app, 100, 32, 5);
    let image = shot.image.expect("the app drew an egui area");
    image.save(path).map_err(io::Error::other)?;
    // Also print the ratatui text around the egui area.
    for y in 0..shot.buffer.area.height {
        let line: String = (0..shot.buffer.area.width)
            .map(|x| {
                if shot.egui_area.contains((x, y).into()) {
                    "·"
                } else {
                    shot.buffer[(x, y)].symbol()
                }
            })
            .collect();
        println!("{line}");
    }
    eprintln!("saved {}x{} egui image to {path}", image.width(), image.height());
    Ok(())
}
