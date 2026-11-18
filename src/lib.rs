//! Run [`egui`] user interfaces inside a [`ratatui`] terminal app.
//!
//! egui is rendered on the CPU with [`egui_software_backend`] into an RGBA
//! buffer sized exactly to the target [`Rect`] (cells × terminal font size),
//! then shown with [`ratatui_image`] through the best graphics protocol the
//! terminal supports (Kitty, Sixel, iTerm2, or unicode halfblocks as a
//! fallback). Terminal input events from [`crossterm`](ratatui::crossterm) are translated into
//! egui input.
//!
//! The simplest way is to implement [`App`] and call [`run`], which owns
//! the terminal setup and event loop:
//!
//! ```no_run
//! use ratagui::{App, EguiTerm, EguiWidget};
//! use ratatui::Frame;
//!
//! struct Counter {
//!     clicks: u32,
//! }
//!
//! impl App for Counter {
//!     fn draw(&mut self, frame: &mut Frame, egui: &mut EguiTerm) {
//!         let widget = EguiWidget::new(|ui| {
//!             if ui.button("Click me").clicked() {
//!                 self.clicks += 1;
//!             }
//!             ui.label(format!("{} clicks", self.clicks));
//!         });
//!         frame.render_stateful_widget(widget, frame.area(), egui);
//!     }
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     ratagui::run(Counter { clicks: 0 })
//! }
//! ```
//!
//! With your own event loop, or for several egui areas, use [`EguiTerm`]
//! and [`EguiWidget`] directly: create the `EguiTerm` after
//! [`ratatui::init()`] and before [`enable_mouse`], pass events to
//! [`EguiTerm::handle_event`], wait with [`EguiTerm::poll_timeout`], and
//! send [`EguiTerm::take_copied_text`] to [`copy_to_clipboard`] after each
//! draw. The `two_panels` example shows the complete loop.
//!
//! Use the re-exported [`egui`] (0.34) rather than your own `egui`
//! dependency. Popups and egui windows are clipped to the egui area.

use std::io::{self, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use base64::Engine as _;

use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use image::{DynamicImage, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::{Rect, Size};
use ratatui::widgets::{StatefulWidget, Widget as _};
use ratatui_image::picker::Capability;
use ratatui_image::picker::cap_parser::QueryStdioOptions;
use ratatui_image::protocol::Protocol;
use ratatui_image::protocol::kitty::Kitty;
use ratatui_image::{Image, Resize};

mod app;

pub use app::{App, Headless, QuitKeys, RunOptions, render_headless, run, run_with};
/// The egui version ratagui renders (0.34). Use this re-export instead of
/// adding your own `egui` dependency, which could resolve to an
/// incompatible version.
pub use egui;
pub use ratatui_image;
pub use ratatui_image::FontSize;
pub use ratatui_image::picker::{Picker, ProtocolType};

/// Kitty image ids handed out to `EguiTerm`s ("RGU" + n, non-zero colour bytes).
static NEXT_KITTY_ID: AtomicU32 = AtomicU32::new(0x00_52_47_55);

/// How the terminal reports mouse coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseMode {
    /// Mouse positions are cell coordinates (the normal case). egui sees the
    /// pointer at the centre of the cell, so hovering is coarse but clicking
    /// buttons, dragging sliders etc. works fine.
    #[default]
    Cells,
    /// Mouse positions are pixel coordinates (xterm "SGR-Pixels", mode 1016,
    /// see [`enable_pixel_mouse`]). Gives egui precise pointer input.
    ///
    /// Note that *all* mouse events of the app then carry pixel coordinates;
    /// use [`EguiTerm::mouse_cell`] to convert them back to cells for the
    /// ratatui parts of your UI.
    Pixels,
}

/// The state of one embedded egui UI: the egui context, the CPU renderer,
/// the last rendered frame and pending input.
///
/// Create one per egui area and pass it to [`EguiWidget`] as widget state.
pub struct EguiTerm {
    ctx: egui::Context,
    renderer: EguiSoftwareRender,
    picker: Picker,
    font_size: FontSize,
    kitty_id: u32,
    pixels_per_point: f32,
    background: [u8; 4],
    /// `background` follows the theme (not set by the user or the terminal).
    background_from_theme: bool,
    margin: f32,
    lazy: bool,
    mouse_mode: MouseMode,

    start: Instant,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    /// Area (in cells) the last frame was rendered for.
    area: Rect,
    /// Whether egui last saw the pointer inside `area`.
    pointer_inside: bool,
    /// When egui last asked to be repainted; `None` = only on input.
    repaint_at: Option<Instant>,
    needs_run: bool,

    pixels: Vec<[u8; 4]>,
    frame: Option<RgbaImage>,
    /// What the last rasterised frame was made from, to skip rasterising
    /// when egui produced the same shapes again.
    last_shapes: Vec<egui::epaint::ClippedShape>,
    last_raster: Option<(usize, usize, f32, [u8; 4])>,
    protocol: Option<Protocol>,
    copied: Option<String>,
}

impl EguiTerm {
    /// Create an `EguiTerm` that draws with the given [`Picker`] (graphics
    /// protocol and font size).
    pub fn new(picker: Picker) -> Self {
        let font_size = picker.font_size();
        let ctx = egui::Context::default();
        // Blend in with the terminal if it told us its background colour.
        let terminal_bg = picker.capabilities().iter().find_map(|c| match c {
            Capability::Background(r, g, b) => Some([*r, *g, *b, 255]),
            _ => None,
        });
        let theme = match terminal_bg {
            Some(bg) if luminance(bg) > 0.5 => egui::Theme::Light,
            _ => egui::Theme::Dark,
        };
        ctx.set_theme(theme);
        let background = terminal_bg.unwrap_or_else(|| theme_background(theme));
        Self {
            ctx,
            renderer: EguiSoftwareRender::new(ColorFieldOrder::Rgba),
            picker,
            font_size,
            // One id per `EguiTerm`, kept for its lifetime, so each new frame
            // *replaces* the previous image in the terminal instead of piling
            // up new ones, while separate instances don't clobber each other.
            kitty_id: NEXT_KITTY_ID.fetch_add(1, Ordering::Relaxed),
            pixels_per_point: default_pixels_per_point(font_size),
            background,
            background_from_theme: terminal_bg.is_none(),
            margin: 8.0,
            lazy: false,
            mouse_mode: MouseMode::Cells,
            start: Instant::now(),
            events: Vec::new(),
            modifiers: egui::Modifiers::NONE,
            area: Rect::default(),
            pointer_inside: false,
            repaint_at: None,
            needs_run: true,
            pixels: Vec::new(),
            frame: None,
            last_shapes: Vec::new(),
            last_raster: None,
            protocol: None,
            copied: None,
        }
    }

    /// Detect graphics protocol, font size and background colour by querying
    /// the terminal, falling back to halfblocks if that fails. See
    /// [`query_picker`].
    ///
    /// Call this after entering the alternate screen (e.g. after
    /// [`ratatui::init()`]) and before reading any terminal events.
    pub fn from_query_stdio() -> Self {
        Self::new(query_picker())
    }

    /// Like [`EguiTerm::from_query_stdio`], but use `protocol` instead of
    /// the detected graphics protocol.
    pub fn with_protocol(protocol: ProtocolType) -> Self {
        let mut picker = query_picker();
        picker.set_protocol_type(protocol);
        Self::new(picker)
    }

    /// Draw with unicode halfblocks, without querying the terminal. Works
    /// everywhere (and in tests), but is far too coarse for readable text.
    pub fn halfblocks() -> Self {
        Self::new(Picker::halfblocks())
    }

    /// The egui context, e.g. to change styles or fonts.
    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    /// Use egui's dark or light theme. By default the theme follows the
    /// terminal's background colour if known, and is dark otherwise.
    pub fn set_theme(&mut self, theme: egui::Theme) {
        self.ctx.set_theme(theme);
        if self.background_from_theme {
            self.background = theme_background(theme);
        }
        self.needs_run = true;
    }

    /// Add a font used for glyphs egui's built-in fonts lack (they cover
    /// Latin text and a set of icons, but e.g. not `●` or CJK). `bytes` is
    /// the content of a `.ttf` / `.otf` file.
    pub fn add_fallback_font(&mut self, name: &str, bytes: Vec<u8>) {
        use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
        let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
            .into_iter()
            .map(|family| InsertFontFamily { family, priority: FontPriority::Lowest })
            .collect();
        self.ctx
            .add_font(FontInsert::new(name, egui::FontData::from_owned(bytes), families));
        self.needs_run = true;
    }

    /// The picker used to encode frames for the terminal.
    pub fn picker(&self) -> &Picker {
        &self.picker
    }

    /// Terminal font size in pixels (the size of one cell).
    pub fn font_size(&self) -> FontSize {
        self.font_size
    }

    /// Scale of the egui UI: how many pixels one egui point covers.
    ///
    /// The default is derived from the terminal's font size so that egui's
    /// body text is about as large as the terminal's text.
    pub fn set_pixels_per_point(&mut self, pixels_per_point: f32) {
        self.pixels_per_point = pixels_per_point.max(0.1);
        self.needs_run = true;
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// Colour behind the egui UI (RGBA, opaque recommended). Defaults to the
    /// terminal's background colour if known, else the egui theme's.
    pub fn set_background(&mut self, rgba: [u8; 4]) {
        self.background = rgba;
        self.background_from_theme = false;
        self.needs_run = true;
    }

    pub fn background(&self) -> [u8; 4] {
        self.background
    }

    /// Space between the edge of the area and the UI, in egui points.
    /// Defaults to `8.0`; use `0.0` to go edge to edge.
    pub fn set_margin(&mut self, margin: f32) {
        self.margin = margin.max(0.0);
        self.needs_run = true;
    }

    /// By default the UI closure runs on every draw, so the UI always shows
    /// current state. In lazy mode it only runs on input, on resize, when
    /// egui asks for a repaint, or after [`EguiTerm::request_repaint`] —
    /// cheaper if you draw often, but state changed outside egui is not
    /// shown until you call `request_repaint`.
    pub fn set_lazy(&mut self, lazy: bool) {
        self.lazy = lazy;
    }

    /// How mouse coordinates are interpreted, see [`MouseMode`].
    pub fn set_mouse_mode(&mut self, mode: MouseMode) {
        self.mouse_mode = mode;
    }

    /// Ask the terminal for pixel-precise mouse reports (see
    /// [`enable_pixel_mouse`]) and switch to [`MouseMode::Pixels`].
    /// Call after [`enable_mouse`].
    pub fn enable_pixel_mouse(&mut self) -> io::Result<()> {
        enable_pixel_mouse()?;
        self.mouse_mode = MouseMode::Pixels;
        Ok(())
    }

    pub fn mouse_mode(&self) -> MouseMode {
        self.mouse_mode
    }

    /// Area (in cells) the UI was last drawn into.
    pub fn area(&self) -> Rect {
        self.area
    }

    /// The last rendered image, exactly `area` cells × font size pixels.
    pub fn image(&self) -> Option<&RgbaImage> {
        self.frame.as_ref()
    }

    /// Does egui want keyboard input (e.g. a text field has focus)?
    /// Use this to decide whether keys go to egui or to your ratatui UI.
    pub fn wants_keyboard_input(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }

    /// Text egui put on the clipboard (Ctrl+C / Ctrl+X in a text field)
    /// since the last call. See [`copy_to_clipboard`].
    pub fn take_copied_text(&mut self) -> Option<String> {
        self.copied.take()
    }

    /// Make [`EguiTerm::poll_timeout`] return immediately so the app draws
    /// again. In [lazy mode](EguiTerm::set_lazy) this is also what makes the
    /// UI closure run again after state changed outside of egui.
    pub fn request_repaint(&mut self) {
        self.needs_run = true;
    }

    /// How long the event loop may block waiting for input before the next
    /// draw is due (for animations or egui's own repaint requests).
    pub fn poll_timeout(&self) -> Duration {
        if self.needs_run || !self.events.is_empty() {
            return Duration::ZERO;
        }
        match self.repaint_at {
            Some(at) => at.saturating_duration_since(Instant::now()),
            None => Duration::from_secs(60),
        }
    }

    /// Convert a mouse event position to a cell position, honouring
    /// [`MouseMode::Pixels`].
    pub fn mouse_cell(&self, column: u16, row: u16) -> (u16, u16) {
        match self.mouse_mode {
            MouseMode::Cells => (column, row),
            MouseMode::Pixels => (
                column / self.font_size.width.max(1),
                row / self.font_size.height.max(1),
            ),
        }
    }

    /// The same event with mouse positions in cells. A no-op in
    /// [`MouseMode::Cells`]; in [`MouseMode::Pixels`] use this for the
    /// ratatui parts of your app ([`run`] does it for you).
    pub fn to_cell_event(&self, event: &Event) -> Event {
        let mut event = event.clone();
        if let Event::Mouse(mouse) = &mut event {
            (mouse.column, mouse.row) = self.mouse_cell(mouse.column, mouse.row);
        }
        event
    }

    /// Feed a terminal event to egui.
    ///
    /// Mouse events are only forwarded while they are inside the area the UI
    /// was last drawn into (moving out sends "pointer gone"); key events are
    /// always forwarded, so filter them first if your ratatui UI handles keys
    /// too (see [`EguiTerm::wants_keyboard_input`]).
    ///
    /// Returns `true` if the event was forwarded to egui.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) => {
                // Terminals often send pasted newlines as a bare CR.
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                self.events.push(egui::Event::Paste(text));
                true
            }
            Event::FocusGained => {
                self.events.push(egui::Event::WindowFocused(true));
                true
            }
            Event::FocusLost => {
                self.events.push(egui::Event::WindowFocused(false));
                true
            }
            Event::Resize(..) => {
                self.needs_run = true;
                false
            }
        }
    }

    fn handle_key(&mut self, key: &KeyEvent) -> bool {
        // Without the kitty keyboard protocol crossterm only reports presses;
        // a press is turned into press+release, so releases are ignored.
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let modifiers = egui_modifiers(key.modifiers);
        self.modifiers = modifiers;
        let push_key = |events: &mut Vec<egui::Event>, key: egui::Key, modifiers| {
            for pressed in [true, false] {
                events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                });
            }
        };

        match key.code {
            KeyCode::Char(c) => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match (ctrl, c.to_ascii_lowercase()) {
                    (true, 'c') => self.events.push(egui::Event::Copy),
                    (true, 'x') => self.events.push(egui::Event::Cut),
                    _ => {
                        if let Some(k) = egui::Key::from_name(&c.to_string()) {
                            push_key(&mut self.events, k, modifiers);
                        }
                        if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) {
                            self.events.push(egui::Event::Text(c.to_string()));
                        }
                    }
                }
            }
            KeyCode::BackTab => {
                let shift = egui::Modifiers { shift: true, ..modifiers };
                push_key(&mut self.events, egui::Key::Tab, shift);
            }
            code => match egui_key(code) {
                Some(k) => push_key(&mut self.events, k, modifiers),
                None => return false,
            },
        }
        true
    }

    fn handle_mouse(&mut self, mouse: &MouseEvent) -> bool {
        let modifiers = egui_modifiers(mouse.modifiers);
        self.modifiers = modifiers;
        let (pos, inside) = self.mouse_pos(mouse.column, mouse.row);
        if !inside {
            // Outside: let egui know (once) that the pointer left, but keep
            // forwarding a release so drags that leave the area don't get stuck.
            let mut forwarded = false;
            if let MouseEventKind::Up(button) = mouse.kind {
                self.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui_button(button),
                    pressed: false,
                    modifiers,
                });
                forwarded = true;
            }
            if self.pointer_inside {
                self.pointer_inside = false;
                self.events.push(egui::Event::PointerGone);
                forwarded = true;
            }
            return forwarded;
        }
        self.pointer_inside = true;

        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.events.push(egui::Event::PointerMoved(pos));
            }
            MouseEventKind::Down(button) | MouseEventKind::Up(button) => {
                self.events.push(egui::Event::PointerMoved(pos));
                self.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui_button(button),
                    pressed: matches!(mouse.kind, MouseEventKind::Down(_)),
                    modifiers,
                });
            }
            MouseEventKind::ScrollUp => self.push_wheel(pos, egui::vec2(0.0, 1.0), modifiers),
            MouseEventKind::ScrollDown => self.push_wheel(pos, egui::vec2(0.0, -1.0), modifiers),
            MouseEventKind::ScrollLeft => self.push_wheel(pos, egui::vec2(1.0, 0.0), modifiers),
            MouseEventKind::ScrollRight => self.push_wheel(pos, egui::vec2(-1.0, 0.0), modifiers),
        }
        true
    }

    fn push_wheel(&mut self, pos: egui::Pos2, delta: egui::Vec2, modifiers: egui::Modifiers) {
        self.events.push(egui::Event::PointerMoved(pos));
        self.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta,
            phase: egui::TouchPhase::Move,
            modifiers,
        });
    }

    /// Mouse position in pixels relative to the terminal's top-left corner.
    fn mouse_pixels(&self, column: u16, row: u16) -> (f32, f32) {
        let (fw, fh) = (self.font_size.width as f32, self.font_size.height as f32);
        match self.mouse_mode {
            // Centre of the cell.
            MouseMode::Cells => ((column as f32 + 0.5) * fw, (row as f32 + 0.5) * fh),
            MouseMode::Pixels => (column as f32 + 0.5, row as f32 + 0.5),
        }
    }

    /// Mouse position in egui points (clamped to the area) and whether it is
    /// inside the area.
    fn mouse_pos(&self, column: u16, row: u16) -> (egui::Pos2, bool) {
        let (x, y) = self.mouse_pixels(column, row);
        let (ox, oy) = self.area_origin_pixels();
        let (w, h) = self.area_size_pixels();
        let (x, y) = (x - ox, y - oy);
        let inside = x >= 0.0 && y >= 0.0 && x < w && y < h;
        let pos = egui::pos2(x.clamp(0.0, w), y.clamp(0.0, h)) / self.effective_ppp();
        (pos, inside)
    }

    /// Pixels per egui point, including egui's own zoom (Ctrl +/-/0).
    fn effective_ppp(&self) -> f32 {
        self.pixels_per_point * self.ctx.zoom_factor()
    }

    fn area_origin_pixels(&self) -> (f32, f32) {
        (
            self.area.x as f32 * self.font_size.width as f32,
            self.area.y as f32 * self.font_size.height as f32,
        )
    }

    fn area_size_pixels(&self) -> (f32, f32) {
        (
            self.area.width as f32 * self.font_size.width as f32,
            self.area.height as f32 * self.font_size.height as f32,
        )
    }

    /// Run the egui UI for `area` and render it. Rasterising is skipped if
    /// egui produced the same shapes as last time, and the image is only
    /// re-encoded for the terminal if its pixels changed. In
    /// [lazy mode](EguiTerm::set_lazy) the UI is only run if needed.
    ///
    /// [`EguiWidget`] calls this for you.
    pub fn update(&mut self, area: Rect, mut build_ui: impl FnMut(&mut egui::Ui)) {
        let now = Instant::now();
        let due = self.repaint_at.is_some_and(|at| at <= now);
        let resized = area != self.area;
        self.area = area;
        if self.lazy && !(self.needs_run || resized || due || !self.events.is_empty()) {
            return;
        }
        self.needs_run = false;
        self.repaint_at = None;

        if area.is_empty() {
            self.protocol = None;
            self.frame = None;
            self.last_raster = None;
            self.events.clear();
            return;
        }

        let (w, h) = self.area_size_pixels();
        let (w, h) = (w as usize, h as usize);
        let ppp = self.effective_ppp();

        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                native_pixels_per_point: Some(self.pixels_per_point),
                focused: Some(true),
                ..Default::default()
            },
        );
        let input = egui::RawInput {
            viewports,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w as f32, h as f32) / ppp,
            )),
            time: Some(self.start.elapsed().as_secs_f64()),
            modifiers: self.modifiers,
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };

        // No panel fill: the background is the buffer's pre-fill colour.
        let panel_frame = egui::Frame::NONE.inner_margin(self.margin);
        let output = self.ctx.run_ui(input, |root| {
            egui::CentralPanel::default()
                .frame(panel_frame)
                .show_inside(root, |ui| build_ui(ui));
        });

        for command in &output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                self.copied = Some(text.clone());
            }
        }
        if let Some(delay) = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
        {
            self.repaint_at = now.checked_add(delay);
        }

        // Same shapes on the same canvas give the same pixels: nothing to do.
        let raster = (w, h, output.pixels_per_point, self.background);
        if self.protocol.is_some()
            && output.textures_delta.is_empty()
            && self.last_raster == Some(raster)
            && self.last_shapes == output.shapes
        {
            return;
        }
        self.last_raster = Some(raster);
        self.last_shapes.clone_from(&output.shapes);

        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        self.pixels.clear();
        self.pixels.resize(w * h, self.background);
        self.renderer.render(
            &mut BufferMutRef::new(&mut self.pixels, w, h),
            &primitives,
            &output.textures_delta,
            output.pixels_per_point,
        );

        let unchanged = self.frame.as_ref().is_some_and(|frame| {
            frame.width() as usize == w
                && frame.height() as usize == h
                && frame.as_raw().as_slice() == self.pixels.as_flattened()
        });
        if unchanged && self.protocol.is_some() {
            return;
        }
        let image = RgbaImage::from_raw(w as u32, h as u32, self.pixels.as_flattened().to_vec())
            .expect("buffer has w*h pixels");
        self.protocol = self.encode(DynamicImage::ImageRgba8(image.clone()), area.as_size());
        self.frame = Some(image);
    }

    fn encode(&self, image: DynamicImage, size: Size) -> Option<Protocol> {
        let result = match self.picker.protocol_type() {
            ProtocolType::Kitty => Kitty::new(
                image,
                size,
                self.kitty_id,
                self.picker.tmux_detected(),
                self.picker
                    .capabilities()
                    .contains(&Capability::KittyCompression),
            )
            .map(Protocol::Kitty),
            // The image is already exactly the area's pixel size, so `Fit`
            // does not rescale (except for halfblocks, which downsamples).
            _ => self.picker.new_protocol(image, size, Resize::Fit(None)),
        };
        result.ok()
    }

    /// Draw the last rendered frame into `buf`.
    pub fn draw(&self, area: Rect, buf: &mut Buffer) {
        if let Some(protocol) = &self.protocol {
            Image::new(protocol).allow_clipping(true).render(area, buf);
        }
    }
}

/// A ratatui widget that shows an egui UI, with [`EguiTerm`] as its state.
///
/// ```no_run
/// # use ratagui::{EguiTerm, EguiWidget};
/// # let mut terminal = ratatui::init();
/// # let mut egui = EguiTerm::from_query_stdio();
/// terminal.draw(|frame| {
///     let widget = EguiWidget::new(|ui| {
///         ui.heading("Hello from egui");
///     });
///     frame.render_stateful_widget(widget, frame.area(), &mut egui);
/// }).unwrap();
/// ```
pub struct EguiWidget<F> {
    ui: F,
}

impl<F: FnMut(&mut egui::Ui)> EguiWidget<F> {
    /// `ui` builds the egui UI. It is called on every draw, unless the
    /// [`EguiTerm`] is in [lazy mode](EguiTerm::set_lazy).
    pub fn new(ui: F) -> Self {
        Self { ui }
    }
}

impl<F: FnMut(&mut egui::Ui)> StatefulWidget for EguiWidget<F> {
    type State = EguiTerm;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut EguiTerm) {
        state.update(area, self.ui);
        state.draw(area, buf);
    }
}

/// Scale at which egui's body text (13 points) is about as large as the
/// terminal's text (roughly 80% of the cell height), in steps of 0.25.
fn default_pixels_per_point(font_size: FontSize) -> f32 {
    ((font_size.height as f32 / 16.0 * 4.0).round() / 4.0).clamp(1.0, 4.0)
}

fn theme_background(theme: egui::Theme) -> [u8; 4] {
    theme.default_visuals().panel_fill.to_array()
}

/// Perceived brightness of a colour, 0 (black) to 1 (white).
fn luminance([r, g, b, _]: [u8; 4]) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

fn egui_modifiers(m: KeyModifiers) -> egui::Modifiers {
    let ctrl = m.contains(KeyModifiers::CONTROL);
    egui::Modifiers {
        alt: m.contains(KeyModifiers::ALT),
        ctrl,
        shift: m.contains(KeyModifiers::SHIFT),
        mac_cmd: false,
        command: ctrl,
    }
}

fn egui_button(button: MouseButton) -> egui::PointerButton {
    match button {
        MouseButton::Left => egui::PointerButton::Primary,
        MouseButton::Right => egui::PointerButton::Secondary,
        MouseButton::Middle => egui::PointerButton::Middle,
    }
}

fn egui_key(code: KeyCode) -> Option<egui::Key> {
    use egui::Key;
    Some(match code {
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::ArrowLeft,
        KeyCode::Right => Key::ArrowRight,
        KeyCode::Up => Key::ArrowUp,
        KeyCode::Down => Key::ArrowDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Esc => Key::Escape,
        KeyCode::F(n) => Key::from_name(&format!("F{n}"))?,
        _ => return None,
    })
}

/// Detect the terminal's graphics protocol, font size and background colour
/// with ratatui-image's [`Picker`] query, falling back to halfblocks if the
/// query fails, with one correction on top of ratatui-image's detection:
///
/// ratatui-image disables Kitty and Sixel on Konsole and then falls back to
/// halfblocks, but Konsole also supports the iTerm2 image protocol, which
/// works well there. So on Konsole (outside tmux) iTerm2 is used instead.
///
/// Call this after entering the alternate screen and before reading any
/// terminal events.
pub fn query_picker() -> Picker {
    let options = QueryStdioOptions {
        terminal_background_color_osc: true,
        ..Default::default()
    };
    let mut picker =
        Picker::from_query_stdio_with_options(options).unwrap_or_else(|_| Picker::halfblocks());
    let is_konsole = std::env::var("KONSOLE_VERSION").is_ok_and(|v| !v.is_empty());
    let protocol = corrected_protocol(picker.protocol_type(), is_konsole, picker.tmux_detected());
    picker.set_protocol_type(protocol);
    picker
}

fn corrected_protocol(detected: ProtocolType, is_konsole: bool, is_tmux: bool) -> ProtocolType {
    match detected {
        ProtocolType::Halfblocks if is_konsole && !is_tmux => ProtocolType::Iterm2,
        other => other,
    }
}

/// Enable mouse capture (clicks, drags, motion, wheel) and bracketed paste.
pub fn enable_mouse() -> io::Result<()> {
    use ratatui::crossterm::{event, execute};
    execute!(io::stdout(), event::EnableMouseCapture, event::EnableBracketedPaste)
}

/// Undo [`enable_mouse`] (and [`enable_pixel_mouse`]).
pub fn disable_mouse() -> io::Result<()> {
    use ratatui::crossterm::{event, execute};
    let mut out = io::stdout();
    out.write_all(b"\x1b[?1016l")?;
    execute!(out, event::DisableMouseCapture, event::DisableBracketedPaste)
}

/// Ask the terminal to report mouse positions in pixels (xterm SGR-Pixels,
/// mode 1016). Supported by e.g. xterm, kitty, foot, WezTerm and Ghostty.
/// Call after [`enable_mouse`] and set [`MouseMode::Pixels`].
pub fn enable_pixel_mouse() -> io::Result<()> {
    let mut out = io::stdout();
    out.write_all(b"\x1b[?1016h")?;
    out.flush()
}

/// Put `text` on the system clipboard via OSC 52 (supported by most modern
/// terminals, possibly behind a setting).
pub fn copy_to_clipboard(text: &str) -> io::Result<()> {
    let mut out = io::stdout();
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    write!(out, "\x1b]52;c;{encoded}\x07")?;
    out.flush()
}

#[cfg(test)]
mod tests;
