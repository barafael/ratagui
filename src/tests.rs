use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyEventState, MouseEvent};

/// Halfblocks picker with a fixed 10x20 px font, works without a terminal.
fn egui_term() -> EguiTerm {
    let mut term = EguiTerm::halfblocks();
    term.set_pixels_per_point(1.0);
    term
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE })
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

/// Cell containing the centre of an egui rect (in points).
fn cell_of(term: &EguiTerm, rect: egui::Rect) -> (u16, u16) {
    let fs = term.font_size();
    let c = rect.center() * term.pixels_per_point;
    (
        term.area().x + (c.x / fs.width as f32) as u16,
        term.area().y + (c.y / fs.height as f32) as u16,
    )
}

#[test]
fn renders_a_frame_of_the_area_size() {
    let mut term = egui_term();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let area = Rect::new(2, 1, 30, 10);
    terminal
        .draw(|f| {
            f.render_stateful_widget(EguiWidget::new(|ui| { ui.heading("Hi"); }), area, &mut term)
        })
        .unwrap();

    let frame = term.image().expect("a frame was rendered");
    assert_eq!((frame.width(), frame.height()), (30 * 10, 10 * 20));
    // Something other than the background was drawn (the panel + text).
    assert!(frame.pixels().any(|p| p.0 != term.background));
    // Halfblocks draw into the buffer inside the area only.
    let buf = terminal.backend().buffer();
    assert_eq!(buf[(0, 0)].symbol(), " ");
    assert_ne!(buf[(2, 1)].symbol(), " ");
}

#[test]
fn clicking_a_button_through_terminal_mouse_events() {
    let mut term = egui_term();
    let area = Rect::new(5, 3, 40, 10);
    let mut button_rect = egui::Rect::NOTHING;
    let mut clicks = 0;
    let frame = |term: &mut EguiTerm, button_rect: &mut egui::Rect, clicks: &mut i32| {
        term.update(area, |ui| {
            let response = ui.button("Press");
            *button_rect = response.rect;
            if response.clicked() {
                *clicks += 1;
            }
        });
    };

    frame(&mut term, &mut button_rect, &mut clicks);
    let (col, row) = cell_of(&term, button_rect);
    assert!(term.handle_event(&mouse(MouseEventKind::Moved, col, row)));
    frame(&mut term, &mut button_rect, &mut clicks);
    assert!(term.handle_event(&mouse(MouseEventKind::Down(MouseButton::Left), col, row)));
    assert!(term.handle_event(&mouse(MouseEventKind::Up(MouseButton::Left), col, row)));
    frame(&mut term, &mut button_rect, &mut clicks);
    assert_eq!(clicks, 1);
}

#[test]
fn typing_into_a_text_edit() {
    let mut term = egui_term();
    let area = Rect::new(0, 0, 40, 10);
    let mut text = String::new();
    let mut rect = egui::Rect::NOTHING;
    let frame = |term: &mut EguiTerm, text: &mut String, rect: &mut egui::Rect| {
        term.update(area, |ui| *rect = ui.text_edit_singleline(text).rect);
    };

    frame(&mut term, &mut text, &mut rect);
    let (col, row) = cell_of(&term, rect);
    term.handle_event(&mouse(MouseEventKind::Down(MouseButton::Left), col, row));
    term.handle_event(&mouse(MouseEventKind::Up(MouseButton::Left), col, row));
    frame(&mut term, &mut text, &mut rect);
    assert!(term.wants_keyboard_input());

    for c in "hey".chars() {
        term.handle_event(&key(KeyCode::Char(c), KeyModifiers::NONE));
    }
    term.handle_event(&key(KeyCode::Backspace, KeyModifiers::NONE));
    term.handle_event(&key(KeyCode::Char('Y'), KeyModifiers::SHIFT));
    frame(&mut term, &mut text, &mut rect);
    assert_eq!(text, "heY");
}

#[test]
fn mouse_outside_the_area_is_not_forwarded() {
    let mut term = egui_term();
    term.update(Rect::new(10, 10, 10, 5), |_| {});
    assert!(!term.handle_event(&mouse(MouseEventKind::Moved, 0, 0)));
    assert!(!term.handle_event(&mouse(MouseEventKind::Moved, 20, 12)));
    assert!(term.handle_event(&mouse(MouseEventKind::Moved, 19, 14)));
    assert!(term.handle_event(&mouse(MouseEventKind::Moved, 10, 10)));
}

#[test]
fn pixel_mouse_mode() {
    let mut term = egui_term();
    term.set_mouse_mode(MouseMode::Pixels);
    term.update(Rect::new(1, 1, 10, 5), |_| {});
    // Cell (1,1) starts at pixel (10,20) with the 10x20 halfblocks font.
    assert_eq!(term.mouse_pos(10, 20), (egui::pos2(0.5, 0.5), true));
    assert!(!term.mouse_pos(9, 20).1);
    assert_eq!(term.mouse_cell(25, 45), (2, 2));
}

#[test]
fn runs_the_ui_on_every_draw_by_default() {
    let mut term = egui_term();
    let area = Rect::new(0, 0, 20, 5);
    let mut label = "one";
    term.update(area, |ui| drop(ui.label(label)));
    let first = term.image().unwrap().clone();
    // State changed outside egui, no input, no request_repaint().
    label = "two";
    term.update(area, |ui| drop(ui.label(label)));
    assert_ne!(term.image().unwrap(), &first, "the new state is shown");
}

#[test]
fn unchanged_ui_is_not_rasterised_again() {
    let mut term = egui_term();
    let area = Rect::new(0, 0, 20, 5);
    for _ in 0..5 {
        term.update(area, |ui| drop(ui.label("same")));
    }
    // Poison the pixel buffer: it is only rewritten when rasterising.
    term.pixels.fill([1, 2, 3, 4]);
    term.update(area, |ui| drop(ui.label("same")));
    assert_eq!(term.pixels[0], [1, 2, 3, 4]);
    term.update(area, |ui| drop(ui.label("different")));
    assert_ne!(term.pixels[0], [1, 2, 3, 4]);
}

#[test]
fn background_and_margin() {
    let mut term = egui_term();
    let area = Rect::new(0, 0, 20, 5);
    term.set_background([200, 30, 40, 255]);
    let fill = |ui: &mut egui::Ui| {
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
    };
    term.update(area, fill);
    // The default margin shows the background at the corner...
    assert_eq!(term.image().unwrap().get_pixel(0, 0).0, [200, 30, 40, 255]);
    // ...and without a margin the UI goes edge to edge.
    term.set_margin(0.0);
    term.update(area, fill);
    assert_eq!(term.image().unwrap().get_pixel(0, 0).0, [255, 255, 255, 255]);
}

#[test]
fn defaults_follow_the_terminal() {
    // Halfblocks' 10x20 px font: egui text scaled up to match terminal text.
    assert_eq!(EguiTerm::halfblocks().pixels_per_point(), 1.25);
    assert_eq!(default_pixels_per_point(FontSize::new(8, 16)), 1.0);
    assert_eq!(default_pixels_per_point(FontSize::new(20, 40)), 2.5);
    assert_eq!(default_pixels_per_point(FontSize::new(4, 8)), 1.0, "never below 1");

    assert!(luminance([255, 255, 255, 255]) > 0.5);
    assert!(luminance([30, 30, 46, 255]) < 0.5);
    // Without a known terminal colour the background follows the theme.
    let mut term = egui_term();
    let dark = term.background();
    term.set_theme(egui::Theme::Light);
    assert!(luminance(term.background()) > luminance(dark));
    // An explicit background is kept.
    term.set_background([1, 2, 3, 255]);
    term.set_theme(egui::Theme::Dark);
    assert_eq!(term.background(), [1, 2, 3, 255]);
}

#[test]
fn cell_events_in_both_mouse_modes() {
    let mut term = egui_term();
    let ev = mouse(MouseEventKind::Moved, 25, 45);
    assert_eq!(term.to_cell_event(&ev), ev);
    term.set_mouse_mode(MouseMode::Pixels);
    assert_eq!(term.to_cell_event(&ev), mouse(MouseEventKind::Moved, 2, 2));
    let k = key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(term.to_cell_event(&k), k);
}

#[test]
fn headless_rendering_of_an_app() {
    struct Hello;
    impl App for Hello {
        fn draw(&mut self, frame: &mut ratatui::Frame, egui: &mut EguiTerm) {
            let area = Rect::new(1, 1, 20, 6);
            frame.render_stateful_widget(EguiWidget::new(|ui| drop(ui.label("hi"))), area, egui);
        }
    }
    let shot = render_headless(&mut Hello, 30, 10, 2);
    assert_eq!(shot.egui_area, Rect::new(1, 1, 20, 6));
    let image = shot.image.unwrap();
    assert_eq!((image.width(), image.height()), (20 * 10, 6 * 20));
    assert_eq!(shot.buffer.area, Rect::new(0, 0, 30, 10));
}

#[test]
fn lazy_mode_only_reruns_when_needed() {
    let mut term = egui_term();
    term.set_lazy(true);
    let area = Rect::new(0, 0, 20, 5);
    let mut runs = 0;
    term.update(area, |_| runs += 1);
    // egui may ask for a few passes after the first frame; let it settle.
    for _ in 0..10 {
        if term.poll_timeout() > Duration::from_secs(1) {
            break;
        }
        std::thread::sleep(term.poll_timeout());
        term.update(area, |_| runs += 1);
    }
    let settled = runs;
    term.update(area, |_| runs += 1);
    assert_eq!(runs, settled, "no input, no repaint request: UI is not rebuilt");

    term.handle_event(&key(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(term.poll_timeout(), Duration::ZERO);
    term.update(area, |_| runs += 1);
    assert_eq!(runs, settled + 1);

    term.update(Rect::new(0, 0, 21, 5), |_| runs += 1);
    assert_eq!(runs, settled + 2, "resizing reruns");
}

#[test]
fn key_translation() {
    let mut term = egui_term();
    term.handle_event(&key(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(matches!(
        term.events.as_slice(),
        [
            egui::Event::Key { key: egui::Key::A, pressed: true, .. },
            egui::Event::Key { key: egui::Key::A, pressed: false, .. },
            egui::Event::Text(t),
        ] if t == "a"
    ));

    term.events.clear();
    term.handle_event(&key(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(term.events.as_slice(), [egui::Event::Copy]));

    term.events.clear();
    term.handle_event(&key(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(matches!(
        term.events.first(),
        Some(egui::Event::Key { key: egui::Key::Tab, modifiers, .. }) if modifiers.shift
    ));
}

#[test]
fn pointer_gone_is_sent_once_when_leaving() {
    let mut term = egui_term();
    term.update(Rect::new(10, 10, 10, 5), |_| {});
    // Never inside: moving around outside does not wake egui.
    assert!(!term.handle_event(&mouse(MouseEventKind::Moved, 0, 0)));
    assert!(term.events.is_empty());
    assert!(term.handle_event(&mouse(MouseEventKind::Moved, 12, 12)));
    term.events.clear();
    assert!(term.handle_event(&mouse(MouseEventKind::Moved, 0, 0)));
    assert!(matches!(term.events.as_slice(), [egui::Event::PointerGone]));
    term.events.clear();
    assert!(!term.handle_event(&mouse(MouseEventKind::Moved, 1, 0)));
    assert!(term.events.is_empty());
}

#[test]
fn paste_normalises_newlines() {
    let mut term = egui_term();
    term.handle_event(&Event::Paste("a\r\nb\rc".into()));
    assert!(matches!(term.events.as_slice(), [egui::Event::Paste(t)] if t == "a\nb\nc"));
}

#[test]
fn konsole_uses_iterm2_instead_of_halfblocks() {
    use ratatui_image::picker::ProtocolType::*;
    assert_eq!(corrected_protocol(Halfblocks, true, false), Iterm2);
    assert_eq!(corrected_protocol(Halfblocks, true, true), Halfblocks, "not inside tmux");
    assert_eq!(corrected_protocol(Halfblocks, false, false), Halfblocks);
    assert_eq!(corrected_protocol(Kitty, true, false), Kitty, "real detections are kept");
}
