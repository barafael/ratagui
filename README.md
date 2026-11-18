# ratagui

Run [egui](https://github.com/emilk/egui) user interfaces inside
[ratatui](https://ratatui.rs) terminal apps.

egui is rendered on the CPU ([`egui_software_backend`](https://crates.io/crates/egui_software_backend))
into an RGBA buffer sized exactly to the target area (cells × terminal font
size). That buffer is shown with [`ratatui-image`](https://crates.io/crates/ratatui-image)
through the best graphics protocol your terminal supports: Kitty, Sixel or
iTerm2, with unicode halfblocks as a (blurry) fallback. Terminal mouse,
keyboard and paste events are translated into egui input.

```rust
use ratagui::{App, EguiTerm, EguiWidget};
use ratatui::Frame;

struct Counter {
    clicks: u32,
}

impl App for Counter {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        // Any ratatui widgets can be drawn around (not over) the egui area.
        let widget = EguiWidget::new(|ui| {
            if ui.button("Click me").clicked() {
                self.clicks += 1;
            }
            ui.label(format!("{} clicks", self.clicks));
        });
        frame.render_stateful_widget(widget, frame.area(), egui);
    }
}

fn main() -> std::io::Result<()> {
    ratagui::run(Counter { clicks: 0 }) // Esc, q or Ctrl+Q quits
}
```

`ratagui::run` sets up the terminal, mouse and graphics protocol, runs the
event loop, forwards copied text to the clipboard and restores the terminal.
`App` also has `on_event` (handle input before egui does), `tick` /
`tick_rate` (live data) and `should_quit`; `run_with` takes `RunOptions`.

**Use `ratagui::egui`**, not your own `egui` dependency: ratagui renders
egui 0.34, and a separately added `egui` may resolve to an incompatible
version.

### Your own event loop, several egui areas

`EguiTerm` (the state of one egui area) and `EguiWidget` work in any ratatui
app; see `examples/two_panels.rs` for the complete loop. In short: create the
`EguiTerm` after `ratatui::init()` and before `ratagui::enable_mouse()`, pass
every event to `handle_event`, wait with `event::poll(egui.poll_timeout())`,
and after each draw send `take_copied_text()` to `copy_to_clipboard`. Each
egui area needs its own `EguiTerm`.

### Testing

`ratagui::render_headless(&mut app, columns, rows, frames)` draws an app
without a terminal and returns the egui image and the ratatui buffer, for
snapshot tests and screenshots.

## Examples

```sh
cargo run --example demo         # all of the examples below in one app, with a sidebar to switch

cargo run --example hello      # egui form inside a ratatui layout, shared state
cargo run --example dashboard  # ratatui list + log, egui panel with live chart
cargo run --example paint -- --pixel-mouse   # drawing canvas
cargo run --example two_panels # two egui areas, hand-written event loop

# the classic small egui apps
cargo run --example counter      # the minimal app: a counter
cargo run --example temperature  # Celsius <-> Fahrenheit converter
cargo run --example todo         # todo list with filters
cargo run --example calculator   # pocket calculator, buttons or keyboard
cargo run --example stopwatch    # stopwatch with laps and an animated dial
cargo run --example widgets      # widget gallery with a floating window
```

Flags for the examples (except `two_panels`): `--pixel-mouse`, `--halfblocks`,
`--protocol kitty|sixel|iterm2|halfblocks` (override detection), and
`--screenshot out.png` (renders without a terminal and saves the egui image).
On exit they print the protocol, font size and scale they used.

## How it behaves

- **Always current:** the UI closure runs on every draw, so egui shows your
  current state without further ado. Rasterising is skipped when egui produced
  the same shapes as last time, and an image is only re-sent to the terminal
  if its pixels changed. `poll_timeout()` tells an event loop how long it may
  block before egui wants to animate. If you draw very often,
  `set_lazy(true)` only runs the UI on input, resize, animation or
  `request_repaint()`.
- **Looks:** the background defaults to the terminal's background colour when
  the terminal reports it (and the light or dark egui theme is chosen to
  match), otherwise to the egui theme's. Change it with `set_background`,
  `set_theme` and `set_margin` (default 8 points; 0 for edge to edge).
- **Scale:** the default scale makes egui's body text about as large as the
  terminal's text (e.g. 1.25 px per point for a 10×20 px cell). Override with
  `set_pixels_per_point`.
- **Fonts:** egui's built-in fonts cover Latin text and a set of icons;
  missing glyphs (e.g. `●`, CJK) show as boxes. Add a font file with
  `add_fallback_font(name, bytes)`.
- **Konsole:** ratatui-image disables Kitty and Sixel there and would fall
  back to halfblocks; ratagui's `query_picker()` (used by
  `EguiTerm::from_query_stdio()` and `run`) picks the iTerm2 protocol
  instead, which Konsole renders at full resolution.
- **Kitty:** each `EguiTerm` reuses its own image id, so each frame replaces
  the previous image instead of piling up new ones.
- **Mouse:** by default terminals report cells, and egui sees the pointer at
  the cell centre. That's fine for buttons and sliders but coarse for
  drawing. Terminals with SGR-Pixels (mode 1016, e.g. kitty, foot, WezTerm,
  Ghostty, xterm) give exact positions: set `RunOptions::pixel_mouse`, or
  call `EguiTerm::enable_pixel_mouse()` in your own loop. All mouse events
  then carry pixel positions; `App::on_event` still gets cells, and in your
  own loop `EguiTerm::to_cell_event` converts them.
- **Keyboard:** keys are forwarded to egui unless `App::on_event` consumes
  them. Check `wants_keyboard_input()` first, so typing into an egui text
  field doesn't trigger your shortcuts. Ctrl+C / Ctrl+X in text fields copy
  and cut through the terminal's clipboard support (OSC 52).

## Limitations

- `egui_software_backend` 0.0.3 requires **egui 0.34**, not the latest egui.
- CPU rendering is slow in unoptimised builds. This crate's `Cargo.toml` sets
  `opt-level = 3` for dependencies in the dev profile; do the same in your app.
  The `rayon` feature rasterises on multiple threads.
- Every changed frame is sent to the terminal as a whole image, which is fine
  for interactive UIs but heavy for constant full-screen animation,
  especially with Sixel.
- Popups, dropdowns, tooltips and egui windows are clipped to the egui area;
  they can't extend over the ratatui parts of the screen.
- egui `PaintCallback`s (custom GPU drawing) are not supported.
- Without kitty's keyboard protocol, terminals don't report key releases or
  some key combinations, so egui only sees key presses.
- If a terminal doesn't answer the startup query at all, the first key press
  afterwards can get lost.
