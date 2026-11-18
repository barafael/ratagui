//! egui widgets inside a ratatui layout, sharing state with the ratatui parts.
//!
//! `cargo run --example hello`  (quit with Esc / q / Ctrl+Q)

mod common;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

#[derive(PartialEq, Clone, Copy, Debug)]
enum Flavour {
    Vanilla,
    Chocolate,
    Strawberry,
}

struct Hello {
    name: String,
    age: u32,
    counter: i32,
    subscribed: bool,
    flavour: Flavour,
    notes: String,
}

impl common::App for Hello {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        let [title, body, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        frame.render_widget(
            Line::from(" ratagui · egui inside ratatui ").bold().reversed().centered(),
            title,
        );

        let block = Block::bordered()
            .title(" egui ")
            .border_style(Style::new().fg(Color::Cyan));
        let inner = block.inner(body);
        frame.render_widget(block, body);

        let widget = EguiWidget::new(|ui| self.ui(ui));
        frame.render_stateful_widget(widget, inner, egui);

        let status_text = format!(
            " ratatui sees: name={:?} age={} counter={} flavour={:?} · press + / - when no text field is focused · Esc quits",
            self.name, self.age, self.counter, self.flavour
        );
        frame.render_widget(Paragraph::new(status_text).on_dark_gray(), status);
    }

    fn on_event(&mut self, event: &Event, egui: &mut EguiTerm) -> bool {
        // Keys go to ratatui unless an egui text field has focus.
        if egui.wants_keyboard_input() {
            return false;
        }
        let Event::Key(key) = event else { return false };
        let delta = match key.code {
            KeyCode::Char('+') => 1,
            KeyCode::Char('-') => -1,
            _ => return false,
        };
        // The egui UI shows the new value on the next draw.
        self.counter += delta;
        true
    }
}

impl Hello {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Hello from egui");
        ui.label("This UI is rendered by egui on the CPU and shown with a terminal graphics protocol.");
        ui.add_space(8.0);

        egui::Grid::new("form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.name);
            ui.end_row();

            ui.label("Age");
            ui.add(egui::Slider::new(&mut self.age, 0..=120).suffix(" years"));
            ui.end_row();

            ui.label("Flavour");
            egui::ComboBox::from_id_salt("flavour")
                .selected_text(format!("{:?}", self.flavour))
                .show_ui(ui, |ui| {
                    for f in [Flavour::Vanilla, Flavour::Chocolate, Flavour::Strawberry] {
                        ui.selectable_value(&mut self.flavour, f, format!("{f:?}"));
                    }
                });
            ui.end_row();

            ui.label("Newsletter");
            ui.checkbox(&mut self.subscribed, "Subscribe");
            ui.end_row();

            ui.label("Counter");
            ui.horizontal(|ui| {
                if ui.button("−").clicked() {
                    self.counter -= 1;
                }
                ui.monospace(format!("{:>4}", self.counter));
                if ui.button("+").clicked() {
                    self.counter += 1;
                }
            });
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.collapsing("Notes", |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.notes)
                    .desired_rows(3)
                    .hint_text("Multi-line text editing works too"),
            );
        });

        if !self.name.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("Hi {}, you are {} years old!", self.name, self.age))
                    .color(egui::Color32::LIGHT_GREEN)
                    .size(18.0),
            );
        }
    }
}

pub fn app() -> impl common::App {
    Hello {
        name: "Ferris".into(),
        age: 11,
        counter: 0,
        subscribed: true,
        flavour: Flavour::Chocolate,
        notes: String::new(),
    }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
