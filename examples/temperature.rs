//! Temperature converter: edit either field and the other follows.
//!
//! `cargo run --example temperature`  (quit with Esc / Ctrl+Q)

mod common;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;

struct Temperature {
    celsius: String,
    fahrenheit: String,
}

fn format(value: f64) -> String {
    // One decimal, without a trailing ".0".
    let s = format!("{value:.1}");
    s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
}

impl Temperature {
    fn set_celsius(&mut self, c: f64) {
        self.celsius = format(c);
        self.fahrenheit = format(c * 9.0 / 5.0 + 32.0);
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Temperature converter");
        ui.add_space(8.0);

        let celsius = self.celsius.trim().parse::<f64>().ok();
        let fahrenheit = self.fahrenheit.trim().parse::<f64>().ok();
        let invalid = ui.visuals().error_fg_color;

        ui.horizontal(|ui| {
            let mut field = egui::TextEdit::singleline(&mut self.celsius).desired_width(80.0);
            if celsius.is_none() {
                field = field.text_color(invalid);
            }
            if ui.add(field).changed()
                && let Ok(c) = self.celsius.trim().parse::<f64>()
            {
                self.fahrenheit = format(c * 9.0 / 5.0 + 32.0);
            }
            ui.label("°C  =");

            let mut field = egui::TextEdit::singleline(&mut self.fahrenheit).desired_width(80.0);
            if fahrenheit.is_none() {
                field = field.text_color(invalid);
            }
            if ui.add(field).changed()
                && let Ok(f) = self.fahrenheit.trim().parse::<f64>()
            {
                self.celsius = format((f - 32.0) * 5.0 / 9.0);
            }
            ui.label("°F");
        });

        ui.add_space(8.0);
        if let Some(mut c) = celsius {
            let slider = egui::Slider::new(&mut c, -50.0..=150.0).suffix(" °C").step_by(0.5);
            if ui.add(slider).changed() {
                self.set_celsius(c);
            }
            ui.label(format!("{} K", format(c + 273.15)));
        } else {
            ui.colored_label(invalid, "Not a number");
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (name, c) in [("Freezing", 0.0), ("Room", 21.0), ("Body", 37.0), ("Boiling", 100.0)] {
                if ui.button(name).clicked() {
                    self.set_celsius(c);
                }
            }
        });
    }
}

impl common::App for Temperature {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        frame.render_stateful_widget(EguiWidget::new(|ui| self.ui(ui)), frame.area(), egui);
    }
}

pub fn app() -> impl common::App {
    let mut app = Temperature {
        celsius: String::new(),
        fahrenheit: String::new(),
    };
    app.set_celsius(21.0);
    app
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
