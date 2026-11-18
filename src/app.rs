//! A ready-made event loop for apps with one egui area: [`run`].
//!
//! If you already have an event loop, or need several egui areas, use
//! [`EguiTerm`] and [`EguiWidget`](crate::EguiWidget) directly instead (see
//! the `two_panels` example).

use std::io;
use std::time::{Duration, Instant};

use image::RgbaImage;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use crate::{EguiTerm, ProtocolType};

/// An app driven by [`run`].
pub trait App {
    /// Draw the whole screen. Render the egui part with
    /// `frame.render_stateful_widget(EguiWidget::new(|ui| ..), area, egui)`.
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm);

    /// Handle an event before egui sees it. Return `true` if the app
    /// consumed it; otherwise it is forwarded to egui.
    ///
    /// Mouse positions are always in cells here, also in pixel-mouse mode.
    /// Check [`EguiTerm::wants_keyboard_input`] before acting on keys, so
    /// typing into an egui text field doesn't trigger your shortcuts.
    fn on_event(&mut self, _event: &Event, _egui: &mut EguiTerm) -> bool {
        false
    }

    /// Called every [`App::tick_rate`], for apps with live data.
    fn tick(&mut self, _egui: &mut EguiTerm) {}

    /// How often to call [`App::tick`] (and redraw). `None`: only redraw on
    /// input and when egui animates.
    fn tick_rate(&self) -> Option<Duration> {
        None
    }

    /// Return `true` to end [`run`].
    fn should_quit(&self) -> bool {
        false
    }
}

/// Which keys end [`run`], besides [`App::should_quit`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuitKeys {
    /// Ctrl+Q always quits; Esc and `q` quit unless an egui text field has
    /// keyboard focus.
    #[default]
    Default,
    /// No built-in quit keys.
    None,
}

/// Options for [`run_with`].
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Ask the terminal for pixel-precise mouse reports (kitty, foot,
    /// WezTerm, Ghostty, xterm). Terminals without support keep reporting
    /// cells, which then get misread as pixels, so only enable this for
    /// terminals known to support it.
    pub pixel_mouse: bool,
    /// Use this graphics protocol instead of the detected one.
    /// [`ProtocolType::Halfblocks`] also skips querying the terminal.
    pub protocol: Option<ProtocolType>,
    pub quit_keys: QuitKeys,
    /// Print the protocol and font size used to stderr on exit.
    pub report: bool,
}

/// Run `app` in the terminal until it quits: sets up the terminal, mouse
/// and graphics protocol, runs the event loop, copies text egui puts on the
/// clipboard, and restores the terminal afterwards, also on errors.
pub fn run(app: impl App) -> io::Result<()> {
    run_with(app, RunOptions::default())
}

/// [`run`] with options.
pub fn run_with(mut app: impl App, options: RunOptions) -> io::Result<()> {
    let mut terminal = ratatui::init();
    // Query the terminal before mouse capture, so mouse reports can't get
    // mixed into the replies.
    let mut egui = match options.protocol {
        Some(ProtocolType::Halfblocks) => EguiTerm::halfblocks(),
        Some(protocol) => EguiTerm::with_protocol(protocol),
        None => EguiTerm::from_query_stdio(),
    };
    let mut setup_and_loop = || {
        crate::enable_mouse()?;
        if options.pixel_mouse {
            egui.enable_pixel_mouse()?;
        }
        event_loop(&mut terminal, &mut app, &mut egui, options.quit_keys)
    };
    let result = setup_and_loop();

    // Always restore the terminal, even if setup or the loop failed.
    let disabled = crate::disable_mouse();
    ratatui::restore();
    if options.report {
        eprintln!(
            "ratagui: protocol {:?}, font size {}x{} px, {} px per point",
            egui.picker().protocol_type(),
            egui.font_size().width,
            egui.font_size().height,
            egui.pixels_per_point(),
        );
    }
    result.and(disabled)
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut impl App,
    egui: &mut EguiTerm,
    quit_keys: QuitKeys,
) -> io::Result<()> {
    let mut next_tick = app.tick_rate().map(|rate| Instant::now() + rate);
    while !app.should_quit() {
        terminal.draw(|frame| app.draw(frame, egui))?;
        // egui produces copied text while drawing; send it right away rather
        // than after the next (possibly much later) event.
        if let Some(text) = egui.take_copied_text() {
            crate::copy_to_clipboard(&text)?;
        }

        let mut timeout = egui.poll_timeout();
        if let Some(at) = next_tick {
            timeout = timeout.min(at.saturating_duration_since(Instant::now()));
        }
        if event::poll(timeout)? {
            // Drain everything that's queued, then redraw once.
            loop {
                let ev = event::read()?;
                if quit_keys == QuitKeys::Default && is_quit(&ev, egui) {
                    return Ok(());
                }
                if !app.on_event(&egui.to_cell_event(&ev), egui) {
                    egui.handle_event(&ev);
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        if let (Some(at), Some(rate)) = (next_tick, app.tick_rate())
            && Instant::now() >= at
        {
            app.tick(egui);
            next_tick = Some(at.max(Instant::now() - rate) + rate);
        }
    }
    Ok(())
}

fn is_quit(ev: &Event, egui: &EguiTerm) -> bool {
    let Event::Key(KeyEvent { code, modifiers, .. }) = ev else {
        return false;
    };
    match code {
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => true,
        KeyCode::Esc | KeyCode::Char('q') => !egui.wants_keyboard_input(),
        _ => false,
    }
}

/// What [`render_headless`] drew.
pub struct Headless {
    /// The egui image, if the app drew an egui area.
    pub image: Option<RgbaImage>,
    /// Where the egui area was, in cells.
    pub egui_area: Rect,
    /// The whole screen as ratatui text (the egui area holds halfblocks).
    pub buffer: Buffer,
}

/// Draw `app` without a terminal, for tests and screenshots: `frames` draws
/// (each preceded by a [`App::tick`]) on a `columns` × `rows` screen with a
/// 10×20 px font. Save the result with `headless.image.unwrap().save(path)`.
pub fn render_headless(app: &mut impl App, columns: u16, rows: u16, frames: usize) -> Headless {
    let mut terminal =
        Terminal::new(TestBackend::new(columns, rows)).expect("test backend is infallible");
    let mut egui = EguiTerm::halfblocks();
    for _ in 0..frames.max(1) {
        app.tick(&mut egui);
        terminal
            .draw(|frame| app.draw(frame, &mut egui))
            .expect("test backend is infallible");
    }
    Headless {
        image: egui.image().cloned(),
        egui_area: egui.area(),
        buffer: terminal.backend().buffer().clone(),
    }
}
