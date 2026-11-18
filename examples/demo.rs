//! All examples in one app: pick one from the sidebar on the right.
//!
//! Switch with PageUp / PageDown, the mouse wheel over the sidebar, or a
//! click on a name. Start on a given example with
//! `cargo run --example demo -- todo`.  (quit with Esc / q / Ctrl+Q)

mod common;

// The other examples, compiled in as modules. Their `main` functions are
// unused here; each one's `app()` builds the same app its `main` runs.
#[allow(dead_code)]
#[path = "calculator.rs"]
mod calculator;
#[allow(dead_code)]
#[path = "counter.rs"]
mod counter;
#[allow(dead_code)]
#[path = "dashboard.rs"]
mod dashboard;
#[allow(dead_code)]
#[path = "hello.rs"]
mod hello;
#[allow(dead_code)]
#[path = "paint.rs"]
mod paint;
#[allow(dead_code)]
#[path = "stopwatch.rs"]
mod stopwatch;
#[allow(dead_code)]
#[path = "temperature.rs"]
mod temperature;
#[allow(dead_code)]
#[path = "todo.rs"]
mod todo;
#[allow(dead_code)]
#[path = "widgets.rs"]
mod widgets;

use std::time::{Duration, Instant};

use ratagui::{App, EguiTerm};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListState, Padding, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

const SIDEBAR_WIDTH: u16 = 26;

struct Example {
    name: &'static str,
    about: &'static str,
    app: Box<dyn App>,
    /// When the app's next tick is due, if it has a tick rate.
    next_tick: Option<Instant>,
}

struct Demo {
    examples: Vec<Example>,
    list: ListState,
    /// Where the names were drawn, for mouse clicks.
    list_area: Rect,
    sidebar: Rect,
}

impl Demo {
    fn new() -> Self {
        let example = |name, about, app: Box<dyn App>| Example {
            name,
            about,
            next_tick: app.tick_rate().map(|rate| Instant::now() + rate),
            app,
        };
        let examples = vec![
            example(
                "hello",
                "An egui form inside a ratatui layout. The status line is ratatui and shows \
                 the same state; + and - change the counter from the ratatui side.",
                Box::new(hello::app()),
            ),
            example("counter", "The smallest app: a counter.", Box::new(counter::app())),
            example(
                "temperature",
                "Celsius and Fahrenheit: edit either field and the other follows.",
                Box::new(temperature::app()),
            ),
            example(
                "todo",
                "A todo list. Type and press Enter to add, tick to complete, filter below.",
                Box::new(todo::app()),
            ),
            example(
                "calculator",
                "A pocket calculator. Click the buttons or type digits and operators; \
                 Enter evaluates, c clears.",
                Box::new(calculator::app()),
            ),
            example(
                "stopwatch",
                "A stopwatch with laps and an animated dial. Space starts and stops, \
                 l takes a lap, r resets.",
                Box::new(stopwatch::app()),
            ),
            example(
                "widgets",
                "One of each common egui widget, a theme switch and a floating window.",
                Box::new(widgets::app()),
            ),
            example(
                "paint",
                "A drawing canvas: drag to paint. Run with --pixel-mouse for smooth strokes.",
                Box::new(paint::app()),
            ),
            example(
                "dashboard",
                "A ratatui list and log next to an egui panel with a live chart. It keeps \
                 ticking while you look at other examples.",
                Box::new(dashboard::app()),
            ),
        ];
        Self {
            examples,
            list: ListState::default().with_selected(Some(0)),
            list_area: Rect::default(),
            sidebar: Rect::default(),
        }
    }

    fn selected(&self) -> usize {
        self.list.selected().unwrap_or(0)
    }

    fn select(&mut self, index: usize, egui: &mut EguiTerm) {
        if index >= self.examples.len() || index == self.selected() {
            return;
        }
        self.list.select(Some(index));
        // The examples share one egui context. Forget widget state (scroll
        // offsets, open headers, ..) so it can't leak into the next example,
        // whose widgets may happen to get the same ids.
        egui.context().memory_mut(|memory| memory.data = Default::default());
        egui.request_repaint();
    }

    fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered()
            .title(" examples ")
            .border_style(Style::new().fg(Color::Cyan))
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [list, about, keys] = Layout::vertical([
            Constraint::Length(self.examples.len() as u16 + 1),
            Constraint::Fill(1),
            Constraint::Length(4),
        ])
        .areas(inner);
        self.list_area = list;

        let names = List::new(self.examples.iter().map(|example| example.name))
            .highlight_style(Style::new().bold().reversed())
            .highlight_symbol("› ");
        frame.render_stateful_widget(names, list, &mut self.list);

        let example = &self.examples[self.selected()];
        frame.render_widget(Paragraph::new(example.about).wrap(Wrap { trim: true }), about);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(format!("--example {}", example.name)).dim(),
                Line::default(),
                Line::from("PgUp/PgDn  switch").dim(),
                Line::from("Esc        quit").dim(),
            ]),
            keys,
        );
    }
}

impl App for Demo {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        let [main, sidebar] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDEBAR_WIDTH)])
                .areas(frame.area());
        self.sidebar = sidebar;
        self.draw_sidebar(frame, sidebar);
        if main.is_empty() {
            return;
        }

        // The examples draw to their whole frame, so the selected one gets a
        // smaller frame of its own, which is then copied into the real one.
        // `main` is in the top-left corner, so positions are the same in
        // both frames and mouse events need no translation.
        let selected = self.selected();
        let app = &mut self.examples[selected].app;
        let mut offscreen = Terminal::new(TestBackend::new(main.width, main.height)).unwrap();
        let drawn = offscreen.draw(|frame| app.draw(frame, egui)).unwrap();
        let buffer = frame.buffer_mut();
        for position in main.positions() {
            buffer[position] = drawn.buffer[position].clone();
        }
    }

    fn on_event(&mut self, event: &Event, egui: &mut EguiTerm) -> bool {
        let count = self.examples.len();
        let selected = self.selected();
        let previous = (selected + count - 1) % count;
        let next = (selected + 1) % count;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::PageUp => return self.switch(previous, egui),
                KeyCode::PageDown => return self.switch(next, egui),
                _ => {}
            },
            Event::Mouse(mouse) if self.sidebar.contains((mouse.column, mouse.row).into()) => {
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.select(selected.saturating_sub(1), egui),
                    MouseEventKind::ScrollDown => self.select(selected + 1, egui),
                    MouseEventKind::Down(MouseButton::Left)
                        if self.list_area.contains((mouse.column, mouse.row).into()) =>
                    {
                        let row = (mouse.row - self.list_area.y) as usize;
                        self.select(self.list.offset() + row, egui);
                    }
                    // Other presses fall through, so egui sees that the
                    // click was outside and can e.g. give up text focus.
                    MouseEventKind::Down(_) | MouseEventKind::Moved => return false,
                    _ => {}
                }
                return true;
            }
            _ => {}
        }
        self.examples[selected].app.on_event(event, egui)
    }

    fn tick(&mut self, egui: &mut EguiTerm) {
        // Every example with live data keeps its own pace, selected or not.
        let now = Instant::now();
        for example in &mut self.examples {
            if let (Some(due), Some(rate)) = (example.next_tick, example.app.tick_rate())
                && now >= due
            {
                example.app.tick(egui);
                example.next_tick = Some(now + rate);
            }
        }
    }

    fn tick_rate(&self) -> Option<Duration> {
        self.examples.iter().filter_map(|example| example.app.tick_rate()).min()
    }

    fn should_quit(&self) -> bool {
        self.examples[self.selected()].app.should_quit()
    }
}

impl Demo {
    fn switch(&mut self, index: usize, egui: &mut EguiTerm) -> bool {
        self.select(index, egui);
        true
    }
}

fn main() -> std::io::Result<()> {
    let mut demo = Demo::new();
    // `demo -- <name>` starts on that example.
    let start = std::env::args()
        .skip(1)
        .find_map(|arg| demo.examples.iter().position(|example| example.name == arg));
    if let Some(index) = start {
        demo.list.select(Some(index));
    }
    common::run(demo)
}
