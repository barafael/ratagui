//! A pocket calculator: click the buttons or type.
//!
//! Keys: digits, `.`, `+ - * /`, Enter or `=`, Backspace, `c` to clear.
//! `cargo run --example calculator`  (quit with Esc / q / Ctrl+Q)

mod common;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind};

const KEYS: [[char; 4]; 5] = [
    ['C', '±', '%', '÷'],
    ['7', '8', '9', '×'],
    ['4', '5', '6', '−'],
    ['1', '2', '3', '+'],
    ['0', '.', '⬅', '='],
];

struct Calculator {
    display: String,
    /// Left operand and pending operator.
    pending: Option<(f64, char)>,
    /// The next digit starts a new number.
    fresh: bool,
}

impl Calculator {
    fn value(&self) -> f64 {
        self.display.parse().unwrap_or(0.0)
    }

    fn show(&mut self, value: f64) {
        self.display = if value == 0.0 {
            "0".to_owned()
        } else if value.is_finite() {
            // Round away float noise such as 0.1 + 0.2.
            let rounded = format!("{value:.10}");
            rounded.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            "Error".to_owned()
        };
        self.fresh = true;
    }

    fn evaluate(&mut self) {
        if let Some((left, op)) = self.pending.take() {
            let right = self.value();
            self.show(match op {
                '+' => left + right,
                '−' => left - right,
                '×' => left * right,
                _ => left / right,
            });
        }
    }

    fn press(&mut self, key: char) {
        match key {
            '0'..='9' => {
                if self.fresh || self.display == "0" {
                    self.display.clear();
                    self.fresh = false;
                }
                self.display.push(key);
            }
            '.' => {
                if self.fresh {
                    self.display = "0".to_owned();
                    self.fresh = false;
                }
                if !self.display.contains('.') {
                    self.display.push('.');
                }
            }
            '+' | '−' | '×' | '÷' => {
                // Chained operators evaluate left to right: 2 + 3 × → 5 ×.
                if !self.fresh {
                    self.evaluate();
                }
                self.pending = Some((self.value(), key));
                self.fresh = true;
            }
            '=' => self.evaluate(),
            '±' => self.show(-self.value()),
            '%' => self.show(self.value() / 100.0),
            '⬅' => {
                if !self.fresh {
                    self.display.pop();
                }
                if self.fresh || self.display.is_empty() || self.display == "-" {
                    self.display = "0".to_owned();
                }
            }
            'C' => {
                self.display = "0".to_owned();
                self.pending = None;
                self.fresh = true;
            }
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let button = egui::vec2(52.0, 40.0);
        let spacing = 6.0;
        let width = 4.0 * button.x + 3.0 * spacing;
        ui.spacing_mut().item_spacing = egui::vec2(spacing, spacing);

        // Display: pending operation on top, current number below.
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .corner_radius(6.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(width - 16.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    let pending = match self.pending {
                        Some((left, op)) => format!("{left} {op}"),
                        None => " ".to_owned(),
                    };
                    ui.label(egui::RichText::new(pending).weak().monospace());
                    ui.label(egui::RichText::new(&self.display).size(28.0).monospace().strong());
                });
            });

        for row in KEYS {
            ui.horizontal(|ui| {
                for key in row {
                    let mut text = egui::RichText::new(key).size(18.0);
                    let mut fill = ui.visuals().widgets.inactive.weak_bg_fill;
                    if key == '=' {
                        fill = ui.visuals().selection.bg_fill;
                        text = text.strong();
                    } else if !key.is_ascii_digit() && key != '.' {
                        text = text.color(ui.visuals().warn_fg_color);
                    }
                    if self.pending.is_some_and(|(_, op)| op == key) && self.fresh {
                        fill = ui.visuals().widgets.active.weak_bg_fill;
                    }
                    if ui.add_sized(button, egui::Button::new(text).fill(fill)).clicked() {
                        self.press(key);
                    }
                }
            });
        }
    }
}

impl common::App for Calculator {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        frame.render_stateful_widget(EguiWidget::new(|ui| self.ui(ui)), frame.area(), egui);
    }

    fn on_event(&mut self, event: &Event, egui: &mut EguiTerm) -> bool {
        let Event::Key(key) = event else { return false };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let pressed = match key.code {
            KeyCode::Char(c @ ('0'..='9' | '.' | '+' | '=' | '%')) => c,
            KeyCode::Char('-') => '−',
            KeyCode::Char('*' | 'x') => '×',
            KeyCode::Char('/') => '÷',
            KeyCode::Char('c') | KeyCode::Delete => 'C',
            KeyCode::Enter => '=',
            KeyCode::Backspace => '⬅',
            _ => return false,
        };
        self.press(pressed);
        egui.request_repaint();
        true
    }
}

pub fn app() -> impl common::App {
    Calculator {
        display: "0".to_owned(),
        pending: None,
        fresh: true,
    }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
