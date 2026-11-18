//! A drawing canvas: drag with the mouse to paint.
//!
//! `cargo run --example paint -- --pixel-mouse` for smooth strokes in
//! terminals that support pixel mouse reports (kitty, foot, WezTerm, Ghostty,
//! xterm); without it, strokes snap to cell centres.
//! Quit with Esc / q / Ctrl+Q.

mod common;

use ratagui::{EguiTerm, EguiWidget, MouseMode, egui};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::widgets::Paragraph;

struct Stroke {
    points: Vec<egui::Pos2>,
    stroke: egui::Stroke,
}

struct Paint {
    strokes: Vec<Stroke>,
    current: Option<Stroke>,
    brush: egui::Stroke,
    background: egui::Color32,
}

impl Paint {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Brush");
            ui.color_edit_button_srgba(&mut self.brush.color);
            ui.add(egui::Slider::new(&mut self.brush.width, 1.0..=24.0).text("width"));
            ui.separator();
            ui.label("Canvas");
            ui.color_edit_button_srgba(&mut self.background);
            ui.separator();
            if ui.add_enabled(!self.strokes.is_empty(), egui::Button::new("⟲ Undo")).clicked() {
                self.strokes.pop();
            }
            if ui.button("🗑 Clear").clicked() {
                self.strokes.clear();
            }
        });
        ui.horizontal(|ui| {
            for color in [
                egui::Color32::from_rgb(240, 240, 240),
                egui::Color32::from_rgb(235, 87, 87),
                egui::Color32::from_rgb(242, 153, 74),
                egui::Color32::from_rgb(242, 201, 76),
                egui::Color32::from_rgb(111, 207, 151),
                egui::Color32::from_rgb(86, 204, 242),
                egui::Color32::from_rgb(155, 81, 224),
            ] {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                ui.painter().circle_filled(rect.center(), 9.0, color);
                if self.brush.color == color {
                    ui.painter().circle_stroke(rect.center(), 10.5, (1.5_f32, ui.visuals().strong_text_color()));
                }
                if response.clicked() {
                    self.brush.color = color;
                }
            }
        });

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::drag());
        let rect = response.rect;
        painter.rect_filled(rect, 6.0, self.background);

        // Store points relative to the canvas so it survives resizes.
        let to_canvas = |p: egui::Pos2| (p - rect.min).to_pos2();
        let to_screen = |p: egui::Pos2| rect.min + p.to_vec2();

        if let Some(pos) = response.interact_pointer_pos() {
            let current = self.current.get_or_insert_with(|| Stroke {
                points: Vec::new(),
                stroke: self.brush,
            });
            let p = to_canvas(pos);
            if current.points.last() != Some(&p) {
                current.points.push(p);
            }
        } else if let Some(done) = self.current.take() {
            self.strokes.push(done);
        }

        for s in self.strokes.iter().chain(&self.current) {
            let points: Vec<egui::Pos2> = s.points.iter().copied().map(to_screen).collect();
            if let [p] = points.as_slice() {
                painter.circle_filled(*p, s.stroke.width / 2.0, s.stroke.color);
            } else {
                painter.add(egui::Shape::line(points, s.stroke));
            }
        }

        if self.strokes.is_empty() && self.current.is_none() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "drag here to paint",
                egui::FontId::proportional(20.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
}

impl common::App for Paint {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        let [canvas, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        frame.render_stateful_widget(EguiWidget::new(|ui| self.ui(ui)), canvas, egui);

        let mode = match egui.mouse_mode() {
            MouseMode::Pixels => "pixel mouse",
            MouseMode::Cells => "cell mouse (try --pixel-mouse)",
        };
        let points: usize = self.strokes.iter().map(|s| s.points.len()).sum();
        frame.render_widget(
            Paragraph::new(format!(
                " {} strokes · {points} points · {mode} · Esc quits",
                self.strokes.len()
            ))
            .on_dark_gray(),
            status,
        );
    }
}

pub fn app() -> impl common::App {
    // A wave so the screenshot isn't blank.
    let wave = (0..60)
        .map(|i| {
            let x = 40.0 + i as f32 * 12.0;
            egui::pos2(x, 220.0 + 60.0 * (i as f32 * 0.2).sin())
        })
        .collect();
    Paint {
        strokes: vec![Stroke {
            points: wave,
            stroke: egui::Stroke::new(6.0_f32, egui::Color32::from_rgb(86, 204, 242)),
        }],
        current: None,
        brush: egui::Stroke::new(4.0_f32, egui::Color32::from_rgb(235, 87, 87)),
        background: egui::Color32::from_rgb(20, 22, 28),
    }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
