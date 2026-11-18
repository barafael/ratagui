//! A stopwatch with laps and an animated dial.
//!
//! While it runs, egui asks for repaints itself; when stopped, nothing is
//! redrawn until the next input.
//! Keys: Space start/stop, `l` lap, `r` reset.
//! `cargo run --example stopwatch`  (quit with Esc / q / Ctrl+Q)

mod common;

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind};

struct Stopwatch {
    /// Time accumulated before the current run.
    banked: Duration,
    running_since: Option<Instant>,
    /// Total elapsed time at each lap.
    laps: Vec<Duration>,
}

fn format(d: Duration) -> String {
    let centis = d.as_millis() / 10;
    format!("{:02}:{:02}.{:02}", centis / 6000, centis / 100 % 60, centis % 100)
}

impl Stopwatch {
    fn elapsed(&self) -> Duration {
        self.banked + self.running_since.map_or(Duration::ZERO, |since| since.elapsed())
    }

    fn toggle(&mut self) {
        match self.running_since.take() {
            Some(since) => self.banked += since.elapsed(),
            None => self.running_since = Some(Instant::now()),
        }
    }

    fn lap(&mut self) {
        if self.running_since.is_some() {
            self.laps.push(self.elapsed());
        }
    }

    fn reset(&mut self) {
        self.banked = Duration::ZERO;
        self.running_since = None;
        self.laps.clear();
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let elapsed = self.elapsed();
        let running = self.running_since.is_some();
        if running {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        ui.horizontal_top(|ui| {
            self.dial(ui, elapsed);
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(format(elapsed)).size(40.0).monospace().strong());
                ui.horizontal(|ui| {
                    if ui.button(if running { "⏸ Stop" } else { "▶ Start" }).clicked() {
                        self.toggle();
                    }
                    if ui.add_enabled(running, egui::Button::new("⚑ Lap")).clicked() {
                        self.lap();
                    }
                    if ui.button("⟲ Reset").clicked() {
                        self.reset();
                    }
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    egui::Grid::new("laps").striped(true).spacing([24.0, 4.0]).show(ui, |ui| {
                        let mut previous = Duration::ZERO;
                        for (i, &lap) in self.laps.iter().enumerate() {
                            ui.label(format!("Lap {}", i + 1));
                            ui.monospace(format(lap - previous));
                            ui.weak(format(lap));
                            ui.end_row();
                            previous = lap;
                        }
                    });
                });
            });
        });
    }

    fn dial(&self, ui: &mut egui::Ui, elapsed: Duration) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(180.0, 180.0), egui::Sense::hover());
        let painter = ui.painter();
        let center = rect.center();
        let radius = rect.width() / 2.0 - 4.0;
        let visuals = ui.visuals();
        let at = |turn: f32, r: f32| {
            let angle = turn * TAU - TAU / 4.0;
            center + r * egui::vec2(angle.cos(), angle.sin())
        };

        painter.circle(center, radius, visuals.extreme_bg_color, visuals.window_stroke());
        for tick in 0..60 {
            let (len, stroke) = if tick % 5 == 0 {
                (10.0, egui::Stroke::new(2.0_f32, visuals.text_color()))
            } else {
                (5.0, egui::Stroke::new(1.0_f32, visuals.weak_text_color()))
            };
            let turn = tick as f32 / 60.0;
            painter.line_segment([at(turn, radius - len), at(turn, radius - 2.0)], stroke);
        }

        let seconds = elapsed.as_secs_f32();
        let minutes = seconds / 60.0;
        painter.line_segment(
            [center, at(minutes / 60.0, radius * 0.5)],
            egui::Stroke::new(3.0_f32, visuals.strong_text_color()),
        );
        let hand = egui::Color32::from_rgb(235, 87, 87);
        painter.line_segment(
            [at(seconds / 60.0, -radius * 0.15), at(seconds / 60.0, radius * 0.85)],
            egui::Stroke::new(2.0_f32, hand),
        );
        painter.circle_filled(center, 4.0, hand);
    }
}

impl common::App for Stopwatch {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        frame.render_stateful_widget(EguiWidget::new(|ui| self.ui(ui)), frame.area(), egui);
    }

    fn on_event(&mut self, event: &Event, egui: &mut EguiTerm) -> bool {
        let Event::Key(key) = event else { return false };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        match key.code {
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Char('l') => self.lap(),
            KeyCode::Char('r') => self.reset(),
            _ => return false,
        }
        egui.request_repaint();
        true
    }
}

pub fn app() -> impl common::App {
    // Start with some time on the clock so the screenshot isn't all zeros.
    Stopwatch {
        banked: Duration::from_millis(83_420),
        running_since: None,
        laps: vec![Duration::from_millis(31_250), Duration::from_millis(64_800)],
    }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
