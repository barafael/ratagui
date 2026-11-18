//! A todo list: type and press Enter to add, tick to complete, filter below.
//!
//! `cargo run --example todo`  (quit with Esc / Ctrl+Q)

mod common;

use ratagui::{EguiTerm, EguiWidget, egui};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::widgets::Paragraph;

#[derive(PartialEq, Clone, Copy)]
enum Filter {
    All,
    Active,
    Done,
}

struct Item {
    text: String,
    done: bool,
}

struct Todo {
    items: Vec<Item>,
    new_item: String,
    filter: Filter,
}

impl Todo {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Todo");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.new_item)
                    .hint_text("What needs to be done?")
                    .desired_width(260.0),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || entered) && !self.new_item.trim().is_empty() {
                let text = std::mem::take(&mut self.new_item).trim().to_owned();
                self.items.push(Item { text, done: false });
            }
            if entered {
                field.request_focus();
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.filter, Filter::All, "All");
            ui.selectable_value(&mut self.filter, Filter::Active, "Active");
            ui.selectable_value(&mut self.filter, Filter::Done, "Done");
            ui.separator();
            if ui.button("Clear done").clicked() {
                self.items.retain(|item| !item.done);
            }
        });
        ui.separator();

        let mut delete = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, item) in self.items.iter_mut().enumerate() {
                let visible = match self.filter {
                    Filter::All => true,
                    Filter::Active => !item.done,
                    Filter::Done => item.done,
                };
                if !visible {
                    continue;
                }
                ui.horizontal(|ui| {
                    let mut text = egui::RichText::new(&item.text);
                    if item.done {
                        text = text.strikethrough().weak();
                    }
                    ui.checkbox(&mut item.done, text);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                            delete = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = delete {
            self.items.remove(i);
        }
    }
}

impl common::App for Todo {
    fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
        let [body, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        frame.render_stateful_widget(EguiWidget::new(|ui| self.ui(ui)), body, egui);

        let left = self.items.iter().filter(|item| !item.done).count();
        frame.render_widget(
            Paragraph::new(format!(" {left} of {} left · Esc quits", self.items.len()))
                .on_dark_gray(),
            status,
        );
    }
}

pub fn app() -> impl common::App {
    let item = |text: &str, done| Item { text: text.into(), done };
    Todo {
        items: vec![
            item("Render egui in a terminal", true),
            item("Translate mouse and keyboard input", true),
            item("Write more examples", false),
            item("Take over the world", false),
        ],
        new_item: String::new(),
        filter: Filter::All,
    }
}

fn main() -> std::io::Result<()> {
    common::run(app())
}
