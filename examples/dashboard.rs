//! A ratatui list + log next to an egui detail panel with a live chart.
//!
//! `cargo run --example dashboard`
//! ↑/↓ or click to pick a service; buttons in the egui panel write to the
//! ratatui log. Quit with Esc / q / Ctrl+Q.

mod common;

use std::collections::VecDeque;
use std::time::Duration;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, ListState};

const HISTORY: usize = 120;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Running,
    Stopped,
}

struct Service {
    name: &'static str,
    status: Status,
    cpu: VecDeque<f32>,
    memory_mb: f32,
    memory_limit_mb: f32,
    requests: u64,
    uptime_ms: u64,
}

struct Dashboard {
    services: Vec<Service>,
    list: ListState,
    list_area: Rect,
    log: Vec<String>,
    rng: u64,
    elapsed_s: f32,
}

impl Dashboard {
    fn new() -> Self {
        let names = ["api-gateway", "auth", "billing", "search", "thumbnailer", "mailer"];
        let mut dash = Self {
            services: names
                .iter()
                .enumerate()
                .map(|(i, name)| Service {
                    name,
                    status: if i == 4 { Status::Stopped } else { Status::Running },
                    cpu: VecDeque::with_capacity(HISTORY),
                    memory_mb: 200.0 + 90.0 * i as f32,
                    memory_limit_mb: 1024.0,
                    requests: 1000 * (i as u64 + 1),
                    uptime_ms: 3_600_000 * (i as u64 + 2),
                })
                .collect(),
            list: ListState::default().with_selected(Some(0)),
            list_area: Rect::default(),
            log: vec!["dashboard started".into()],
            rng: 0x2545_f491_4f6c_dd1d,
            elapsed_s: 0.0,
        };
        // Pre-fill some history so the chart isn't empty at start.
        for _ in 0..HISTORY {
            dash.step();
        }
        dash
    }

    fn random(&mut self) -> f32 {
        // xorshift64
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Advance the simulated metrics by one sample.
    fn step(&mut self) {
        self.elapsed_s += 0.5;
        for i in 0..self.services.len() {
            let noise = self.random();
            let t = self.elapsed_s;
            let s = &mut self.services[i];
            let cpu = match s.status {
                Status::Running => {
                    let base = 25.0 + 15.0 * i as f32;
                    (base + 20.0 * (t * 0.2 + i as f32).sin() + 25.0 * noise).clamp(0.0, 100.0)
                }
                Status::Stopped => 0.0,
            };
            if s.cpu.len() == HISTORY {
                s.cpu.pop_front();
            }
            s.cpu.push_back(cpu);
            if s.status == Status::Running {
                s.requests += (cpu * 3.0) as u64;
                s.uptime_ms += 500;
                s.memory_mb = (s.memory_mb + (noise - 0.5) * 20.0).clamp(50.0, s.memory_limit_mb);
            }
        }
    }

    fn selected(&self) -> usize {
        self.list.selected().unwrap_or(0)
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui) {
        let idx = self.selected();
        let service = &mut self.services[idx];
        let mut log = None;

        ui.horizontal(|ui| {
            ui.heading(service.name);
            let (text, color) = match service.status {
                Status::Running => ("running", egui::Color32::from_rgb(90, 200, 120)),
                Status::Stopped => ("stopped", egui::Color32::from_rgb(220, 90, 90)),
            };
            // Status dot drawn with the painter (the default font has no "●").
            let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 5.0, color);
            ui.label(egui::RichText::new(text).color(color).strong());
        });
        ui.separator();

        ui.label("CPU usage, last 60 s");
        cpu_chart(ui, &service.cpu);
        ui.add_space(6.0);

        egui::Grid::new("stats").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
            ui.label("Memory");
            ui.add(
                egui::ProgressBar::new(service.memory_mb / service.memory_limit_mb)
                    .text(format!("{:.0} / {:.0} MB", service.memory_mb, service.memory_limit_mb))
                    .desired_width(260.0),
            );
            ui.end_row();
            ui.label("Requests");
            ui.monospace(service.requests.to_string());
            ui.end_row();
            ui.label("Uptime");
            let u = service.uptime_ms / 1000;
            ui.monospace(format!("{}h {:02}m {:02}s", u / 3600, u / 60 % 60, u % 60));
            ui.end_row();
            ui.label("Memory limit");
            ui.add(egui::Slider::new(&mut service.memory_limit_mb, 256.0..=4096.0).suffix(" MB"));
            ui.end_row();
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let running = service.status == Status::Running;
            if ui.add_enabled(!running, egui::Button::new("▶ Start")).clicked() {
                service.status = Status::Running;
                log = Some(format!("started {}", service.name));
            }
            if ui.add_enabled(running, egui::Button::new("■ Stop")).clicked() {
                service.status = Status::Stopped;
                service.uptime_ms = 0;
                log = Some(format!("stopped {}", service.name));
            }
            if ui.add_enabled(running, egui::Button::new("⟳ Restart")).clicked() {
                service.uptime_ms = 0;
                log = Some(format!("restarted {}", service.name));
            }
        });

        if let Some(line) = log {
            self.log.push(line);
        }
    }
}

/// A small line chart drawn directly with egui's painter.
fn cpu_chart(ui: &mut egui::Ui, samples: &VecDeque<f32>) {
    let size = egui::vec2(ui.available_width(), 140.0_f32.min(ui.available_height() * 0.5));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);

    let grid = egui::Stroke::new(1.0_f32, visuals.weak_text_color().gamma_multiply(0.3));
    for pct in [25.0, 50.0, 75.0] {
        let y = rect.bottom() - rect.height() * pct / 100.0;
        painter.hline(rect.x_range(), y, grid);
        painter.text(
            egui::pos2(rect.left() + 4.0, y),
            egui::Align2::LEFT_BOTTOM,
            format!("{pct:.0}%"),
            egui::FontId::proportional(10.0),
            visuals.weak_text_color(),
        );
    }

    let point = |i: usize, v: f32| {
        egui::pos2(
            rect.left() + rect.width() * i as f32 / (HISTORY - 1) as f32,
            rect.bottom() - rect.height() * v / 100.0,
        )
    };
    let offset = HISTORY - samples.len();
    let line: Vec<egui::Pos2> =
        samples.iter().enumerate().map(|(i, &v)| point(i + offset, v)).collect();
    let color = egui::Color32::from_rgb(80, 160, 255);
    // Filled area under the curve, one quad per segment.
    for pair in line.windows(2) {
        let fill = egui::Shape::convex_polygon(
            vec![pair[0], pair[1], egui::pos2(pair[1].x, rect.bottom()), egui::pos2(pair[0].x, rect.bottom())],
            color.gamma_multiply(0.15),
            egui::Stroke::NONE,
        );
        painter.add(fill);
    }
    painter.add(egui::Shape::line(line.clone(), egui::Stroke::new(2.0_f32, color)));

    // Hover readout.
    if let Some(pos) = response.hover_pos() {
        let i = (((pos.x - rect.left()) / rect.width()) * (HISTORY - 1) as f32).round() as usize;
        if let Some(&v) = i.checked_sub(offset).and_then(|j| samples.get(j)) {
            let p = point(i, v);
            painter.circle_filled(p, 4.0, color);
            painter.text(
                p + egui::vec2(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{v:.1}%"),
                egui::FontId::monospace(12.0),
                visuals.strong_text_color(),
            );
        }
    }
}

impl common::App for Dashboard {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        let [main, log_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(8)]).areas(frame.area());
        let [list_area, detail_area] =
            Layout::horizontal([Constraint::Length(26), Constraint::Fill(1)]).areas(main);

        let items: Vec<ListItem> = self
            .services
            .iter()
            .map(|s| {
                let (dot, color) = match s.status {
                    Status::Running => ("●", Color::Green),
                    Status::Stopped => ("●", Color::Red),
                };
                let cpu = s.cpu.back().copied().unwrap_or(0.0);
                ListItem::new(Line::from(vec![
                    dot.fg(color),
                    format!(" {:<13}{:>5.1}%", s.name, cpu).into(),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(" services (ratatui) "))
            .highlight_style(Style::new().reversed());
        frame.render_stateful_widget(list, list_area, &mut self.list);
        self.list_area = list_area;

        let block = Block::bordered().title(" details (egui) ").cyan();
        let inner = block.inner(detail_area);
        frame.render_widget(block, detail_area);
        frame.render_stateful_widget(EguiWidget::new(|ui| self.detail_ui(ui)), inner, egui);

        let shown = log_area.height.saturating_sub(2) as usize;
        let lines: Vec<ListItem> = self
            .log
            .iter()
            .rev()
            .take(shown)
            .rev()
            .map(|l| ListItem::new(format!("› {l}")))
            .collect();
        frame.render_widget(
            List::new(lines).block(Block::bordered().title(" log (ratatui) · Esc quits ")),
            log_area,
        );
    }

    fn on_event(&mut self, event: &Event, egui: &mut EguiTerm) -> bool {
        match event {
            Event::Key(key) if !egui.wants_keyboard_input() => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.list.select_previous(),
                KeyCode::Down | KeyCode::Char('j') => self.list.select_next(),
                _ => return false,
            },
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                // Positions are cells here, also with --pixel-mouse.
                let (col, row) = (m.column, m.row);
                let inner = self.list_area.inner(ratatui::layout::Margin::new(1, 1));
                if !inner.contains((col, row).into()) {
                    return false;
                }
                let index = self.list.offset() + (row - inner.y) as usize;
                if index < self.services.len() {
                    self.list.select(Some(index));
                }
            }
            _ => return false,
        }
        let selected = self.selected().min(self.services.len() - 1);
        self.list.select(Some(selected));
        true
    }

    fn tick(&mut self, _egui: &mut EguiTerm) {
        self.step();
    }

    fn tick_rate(&self) -> Option<Duration> {
        Some(Duration::from_millis(500))
    }
}

pub fn app() -> impl common::App {
    Dashboard::new()
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
