//! Two independent egui areas with a hand-written event loop.
//!
//! Shows what `ratagui::run` otherwise does for you, and how to use more
//! than one `EguiTerm`: each area has its own, every event goes to all of
//! them (each only reacts to the mouse inside its own area), and keys go to
//! the one that was clicked last.
//!
//! `cargo run --example two_panels`  (quit with Esc / Ctrl+Q)

use std::io;
use std::time::Duration;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Block;

struct State {
    volume: f32,
    muted: bool,
    notes: String,
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    // Query the terminal once, before enabling the mouse, and share the
    // result between both areas.
    let picker = ratagui::query_picker();
    let mut panels = [EguiTerm::new(picker.clone()), EguiTerm::new(picker)];

    let result = ratagui::enable_mouse().and_then(|()| event_loop(&mut terminal, &mut panels));

    // Always restore the terminal, even after an error.
    let disabled = ratagui::disable_mouse();
    ratatui::restore();
    result.and(disabled)
}

fn event_loop(terminal: &mut DefaultTerminal, panels: &mut [EguiTerm; 2]) -> io::Result<()> {
    let mut state = State { volume: 0.4, muted: false, notes: "Click here and type".into() };
    let mut focused = 0;

    loop {
        terminal.draw(|frame| {
            let areas: [_; 2] = Layout::horizontal([Constraint::Fill(1); 2]).areas(frame.area());
            let titles = [" settings (egui) ", " notes (egui) "];
            for (i, area) in areas.into_iter().enumerate() {
                let color = if i == focused { Color::Cyan } else { Color::DarkGray };
                let block = Block::bordered().title(titles[i]).border_style(Style::new().fg(color));
                let inner = block.inner(area);
                frame.render_widget(block, area);
                let widget = EguiWidget::new(|ui| match i {
                    0 => settings_ui(ui, &mut state),
                    _ => notes_ui(ui, &mut state),
                });
                frame.render_stateful_widget(widget, inner, &mut panels[i]);
            }
        })?;

        // Text copied in an egui text field (Ctrl+C) goes to the clipboard.
        for panel in panels.iter_mut() {
            if let Some(text) = panel.take_copied_text() {
                ratagui::copy_to_clipboard(&text)?;
            }
        }

        // Sleep until input arrives or one of the UIs wants to animate.
        let timeout = panels.iter().map(EguiTerm::poll_timeout).min().unwrap_or(Duration::MAX);
        if !event::poll(timeout)? {
            continue;
        }
        let ev = event::read()?;
        match &ev {
            Event::Key(key) => {
                let ctrl_q = key.code == KeyCode::Char('q')
                    && key.modifiers.contains(KeyModifiers::CONTROL);
                if ctrl_q || key.code == KeyCode::Esc {
                    return Ok(());
                }
                // Keys only go to the focused panel.
                panels[focused].handle_event(&ev);
            }
            Event::Mouse(mouse) => {
                if let MouseEventKind::Down(_) = mouse.kind {
                    let at = (mouse.column, mouse.row).into();
                    if let Some(i) = panels.iter().position(|p| p.area().contains(at)) {
                        focused = i;
                    }
                }
                // Each panel ignores the mouse outside its own area.
                for panel in panels.iter_mut() {
                    panel.handle_event(&ev);
                }
            }
            _ => {
                for panel in panels.iter_mut() {
                    panel.handle_event(&ev);
                }
            }
        }
    }
}

fn settings_ui(ui: &mut egui::Ui, state: &mut State) {
    ui.heading("Settings");
    ui.add_enabled(!state.muted, egui::Slider::new(&mut state.volume, 0.0..=1.0).text("volume"));
    ui.checkbox(&mut state.muted, "Muted");
    ui.separator();
    ui.label(format!("The notes panel holds {} characters.", state.notes.chars().count()));
    if ui.button("Clear notes").clicked() {
        state.notes.clear();
    }
}

fn notes_ui(ui: &mut egui::Ui, state: &mut State) {
    ui.heading("Notes");
    let volume = if state.muted { "muted".to_owned() } else { format!("{:.0}%", state.volume * 100.0) };
    ui.label(format!("Volume in the other panel: {volume}"));
    ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut state.notes));
}
