//! Widget gallery: one of each common egui widget, plus a floating window.
//!
//! `cargo run --example widgets`  (quit with Esc / Ctrl+Q)

mod common;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;

#[derive(PartialEq, Clone, Copy, Debug)]
enum Choice {
    First,
    Second,
    Third,
}

struct Gallery {
    enabled: bool,
    checked: bool,
    choice: Choice,
    scalar: f32,
    text: String,
    color: egui::Color32,
    animate: bool,
    window_open: bool,
}

impl Gallery {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Widget gallery");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::widgets::global_theme_preference_switch(ui);
                ui.checkbox(&mut self.window_open, "Window");
                ui.checkbox(&mut self.enabled, "Enabled");
            });
        });
        ui.separator();

        ui.add_enabled_ui(self.enabled, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                egui::Grid::new("gallery")
                    .num_columns(2)
                    .spacing([40.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| self.rows(ui));
            });
        });

        egui::Window::new("A floating window")
            .open(&mut self.window_open)
            .default_pos([420.0, 120.0])
            .default_width(220.0)
            .show(ui.ctx(), |ui| {
                ui.label("Drag me by the title bar, resize me from the corner, or collapse me.");
                ui.add(egui::Slider::new(&mut self.scalar, 0.0..=360.0).text("shared value"));
            });
    }

    fn rows(&mut self, ui: &mut egui::Ui) {
        ui.label("Label");
        ui.label("Plain text, with a tooltip").on_hover_text("Tooltips work with the mouse");
        ui.end_row();

        ui.label("Rich text");
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("strong").strong());
            ui.label(egui::RichText::new("italics").italics());
            ui.label(egui::RichText::new("code").code());
            ui.label(egui::RichText::new("colour").color(self.color));
        });
        ui.end_row();

        ui.label("Button");
        if ui.button("Click me").clicked() {
            self.checked = !self.checked;
        }
        ui.end_row();

        ui.label("Checkbox");
        ui.checkbox(&mut self.checked, "Toggled by the button too");
        ui.end_row();

        ui.label("Radio buttons");
        ui.horizontal(|ui| {
            for choice in [Choice::First, Choice::Second, Choice::Third] {
                ui.radio_value(&mut self.choice, choice, format!("{choice:?}"));
            }
        });
        ui.end_row();

        ui.label("Selectable labels");
        ui.horizontal(|ui| {
            for choice in [Choice::First, Choice::Second, Choice::Third] {
                ui.selectable_value(&mut self.choice, choice, format!("{choice:?}"));
            }
        });
        ui.end_row();

        ui.label("Combo box");
        egui::ComboBox::from_id_salt("choice")
            .selected_text(format!("{:?}", self.choice))
            .show_ui(ui, |ui| {
                for choice in [Choice::First, Choice::Second, Choice::Third] {
                    ui.selectable_value(&mut self.choice, choice, format!("{choice:?}"));
                }
            });
        ui.end_row();

        ui.label("Slider");
        ui.add(egui::Slider::new(&mut self.scalar, 0.0..=360.0).suffix("°"));
        ui.end_row();

        ui.label("Drag value");
        ui.add(egui::DragValue::new(&mut self.scalar).speed(1.0).range(0.0..=360.0));
        ui.end_row();

        ui.label("Progress bar");
        ui.add(egui::ProgressBar::new(self.scalar / 360.0).show_percentage());
        ui.end_row();

        ui.label("Text edit");
        ui.add(egui::TextEdit::singleline(&mut self.text).hint_text("Write something"));
        ui.end_row();

        ui.label("Colour picker");
        ui.color_edit_button_srgba(&mut self.color);
        ui.end_row();

        ui.label("Spinner");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.animate, "Animate");
            if self.animate {
                // Spinners repaint continuously; keep that opt-in.
                ui.spinner();
            }
        });
        ui.end_row();

        ui.label("Collapsing header");
        ui.collapsing("Click to see what is hidden", |ui| {
            ui.label("Not much, as it turns out.");
        });
        ui.end_row();

        ui.label("Separator");
        ui.separator();
        ui.end_row();

        ui.label("Custom painting");
        let (rect, _) = ui.allocate_exact_size(egui::vec2(160.0, 24.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let points = (0..=80)
            .map(|i| {
                let t = i as f32 / 80.0;
                let y = (t * 12.0 + self.scalar.to_radians()).sin();
                egui::pos2(rect.left() + t * rect.width(), rect.center().y + y * 9.0)
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5_f32, self.color)));
        ui.end_row();
    }
}

impl common::App for Gallery {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        frame.render_stateful_widget(EguiWidget::new(|ui| self.ui(ui)), frame.area(), egui);
    }
}

pub fn app() -> impl common::App {
    Gallery {
        enabled: true,
        checked: true,
        choice: Choice::Second,
        scalar: 120.0,
        text: String::new(),
        color: egui::Color32::from_rgb(86, 204, 242),
        animate: false,
        window_open: true,
    }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
