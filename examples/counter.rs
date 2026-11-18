//! The smallest egui app: a counter filling the whole terminal.
//!
//! `cargo run --example counter`  (quit with Esc / q / Ctrl+Q)

mod common;

use ratagui::{EguiTerm, EguiWidget};
use ratatui::Frame;

struct Counter {
    value: i32,
}

impl common::App for Counter {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        let widget = EguiWidget::new(|ui| {
            ui.heading("Counter");
            ui.horizontal(|ui| {
                if ui.button("−").clicked() {
                    self.value -= 1;
                }
                ui.label(self.value.to_string());
                if ui.button("+").clicked() {
                    self.value += 1;
                }
            });
            if ui.button("Reset").clicked() {
                self.value = 0;
            }
        });
        frame.render_stateful_widget(widget, frame.area(), egui);
    }
}

pub fn app() -> impl common::App {
    Counter { value: 0 }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
