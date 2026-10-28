//! Task panes: Navigation (headings, pages, search results), Clipboard, Styles, Style Inspector,
//! Accessibility and Alt Text (see `a11y_pane`), Format Shape (see `format_shape`), Restrict Editing
//! ([`crate::dialogs_review`]), Comments, and the SmartArt Text Pane. `craft_ui::panes` draws them
//! around the document, with their title bars, menus and resize edges; the session's view flags say
//! which are open.

use std::collections::BTreeMap;

use craft_ui::panes::{Edge, PaneContent, PaneHost, PaneLabels, PaneRequest, PaneState, Place, SidePolicy};
use egui::{Stroke, Ui, vec2};
use serde_json::{Value, json};
use wordcraft_doc::numbering::{Counters, LevelSuffix};
use wordcraft_doc::section::NumFormat;
use wordcraft_doc::{Document, Path, Pos, StoryRef};
use wordcraft_engine::StylesPaneDock;

use crate::WordApp;
use crate::theme::{Tokens, regular, semibold};

/// How the Navigation pane draws its outline. Both styles share the continuous trunks, the
/// circular chevron nodes, the curved elbow and the selection pill; A leaves the inactive rows
/// without elbows (Zed-style clean rail), B connects every child (Fleet/Craft Pro look).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavTreeStyle {
    /// Opção A: Zed / Linear — clean rail, the blue elbow only on the active branch.
    #[default]
    ZedLinear,
    /// Opção B: Fleet / Craft Pro — every child connected, nested trunks ending on the last curve.
    FleetConnected,
}

impl NavTreeStyle {
    pub fn code(self) -> &'static str {
        match self {
            Self::ZedLinear => "zed_linear",
            Self::FleetConnected => "fleet_connected",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "zed_linear" | "zed" | "a" => Some(Self::ZedLinear),
            "fleet_connected" | "fleet" | "b" => Some(Self::FleetConnected),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ZedLinear => "Option A: Zed / Linear (clean rail + active curve)",
            Self::FleetConnected => "Option B: Fleet / Craft Pro (all branches connected)",
        }
    }
}

/// `navTreeStyle` from `ui.json`: a missing or unknown value is the default (A), never an error,
/// so an old or newer preferences file still loads.
pub(crate) fn lenient_nav_tree_style<'de, D: serde::Deserializer<'de>>(d: D) -> Result<NavTreeStyle, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_str().and_then(NavTreeStyle::from_code).unwrap_or_default())
}

/// The task panes, in their default order: each edge's first pane is outermost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    /// SmartArt Design › Text Pane, while a SmartArt graphic WordCraft can edit is selected.
    SmartArtText,
    Navigation,
    Clipboard,
    Styles,
    StyleInspector,
    Accessibility,
    AltText,
    FormatShape,
    RestrictEditing,
    Comments,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Self::SmartArtText => "Text Pane",
            Self::Navigation => "Navigation",
            Self::Clipboard => "Clipboard",
            Self::Styles => "Styles",
            Self::StyleInspector => "Style Inspector",
            Self::Accessibility => "Accessibility",
            Self::AltText => "Alt Text",
            Self::FormatShape => "Format Shape",
            Self::RestrictEditing => "Restrict Editing",
            Self::Comments => "Comments",
        }
    }

    fn default_state(self) -> PaneState {
        let (edge, width) = match self {
            Self::SmartArtText => (Edge::Start, 250.0),
            Self::Navigation => (Edge::Start, 260.0),
            Self::Clipboard => (Edge::Start, 250.0),
            Self::Styles => (Edge::End, 250.0),
            Self::StyleInspector => (Edge::End, 240.0),
            Self::Accessibility => (Edge::End, 280.0),
            Self::AltText => (Edge::End, 260.0),
            Self::FormatShape => (Edge::End, 280.0),
            Self::RestrictEditing => (Edge::End, 270.0),
            Self::Comments => (Edge::End, 290.0),
        };
        PaneState::docked(edge, width)
    }

    /// The pane's key in `ui.json`'s `panePlaces`.
    pub fn key(self) -> &'static str {
        match self {
            Self::SmartArtText => "smartArtText",
            Self::Navigation => "navigation",
            Self::Clipboard => "clipboard",
            Self::Styles => "styles",
            Self::StyleInspector => "styleInspector",
            Self::Accessibility => "accessibility",
            Self::AltText => "altText",
            Self::FormatShape => "formatShape",
            Self::RestrictEditing => "restrictEditing",
            Self::Comments => "comments",
        }
    }

    const ALL: [Self; 10] = [
        Self::SmartArtText,
        Self::Navigation,
        Self::Clipboard,
        Self::Styles,
        Self::StyleInspector,
        Self::Accessibility,
        Self::AltText,
        Self::FormatShape,
        Self::RestrictEditing,
        Self::Comments,
    ];

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|pane| pane.key() == key)
    }

    /// The command that closes the pane; Restrict Editing has none and closes in [`close`].
    fn close_command(self) -> Option<&'static str> {
        match self {
            Self::SmartArtText => Some("smartArt.textPane"),
            Self::Navigation => Some("view.navigationPane"),
            Self::Clipboard => Some("edit.clipboardPane"),
            Self::Styles => Some("view.stylesPane"),
            Self::StyleInspector => Some("styles.inspector"),
            Self::Accessibility => Some("view.accessibilityPane"),
            Self::AltText => Some("view.altTextPane"),
            Self::FormatShape => Some("view.formatShapePane"),
            Self::Comments => Some("view.commentsPane"),
            Self::RestrictEditing => None,
        }
    }
}

/// Every pane in the order the user left them (`paneOrder` in `ui.json`), then the panes that
/// list lacks in their default order. Each edge shows its panes in this order, the first outermost.
fn order(app: &WordApp) -> Vec<Pane> {
    let mut order: Vec<Pane> = app.ui.pane_order.iter().filter_map(|key| Pane::from_key(key)).collect();
    for pane in Pane::ALL {
        if !order.contains(&pane) {
            order.push(pane);
        }
    }
    order
}

/// Put `pane` directly before `before` in the order, outside it on their edge (`None`: after
/// every pane).
fn move_before(app: &mut WordApp, pane: Pane, before: Option<Pane>) {
    let mut order = order(app);
    order.retain(|p| *p != pane);
    let at = before.and_then(|b| order.iter().position(|p| *p == b)).unwrap_or(order.len());
    order.insert(at, pane);
    app.ui.pane_order = order.iter().map(|p| p.key().to_string()).collect();
}

/// `paneOrder` as saved: unknown and repeated keys are dropped, not the whole file.
pub(crate) fn lenient_order<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = <Value as serde::Deserialize>::deserialize(d)?;
    let mut keys: Vec<String> = Vec::new();
    for key in v.as_array().map(Vec::as_slice).unwrap_or_default().iter().take(Pane::ALL.len() * 4).filter_map(Value::as_str) {
        if Pane::from_key(key).is_some() && !keys.iter().any(|k| k == key) {
            keys.push(key.to_string());
        }
    }
    Ok(keys)
}

/// The panes open now, from the session's view flags, in the user's order.
pub fn open(app: &WordApp) -> Vec<Pane> {
    let v = &app.session.view;
    let smart_art = v.smart_art_pane && wordcraft_engine::cmd::smart_art::selected_smart_art(&app.session).is_some();
    let mut open = [
        (Pane::SmartArtText, smart_art),
        (Pane::Navigation, v.nav_pane),
        (Pane::Clipboard, v.clipboard_pane),
        (Pane::Styles, v.styles_pane),
        (Pane::StyleInspector, v.style_inspector),
        (Pane::Accessibility, v.accessibility_pane),
        (Pane::AltText, v.alt_text_pane),
        (Pane::FormatShape, v.format_shape_pane),
        (Pane::RestrictEditing, app.restrict.open),
        (Pane::Comments, v.comments_pane),
    ]
    .into_iter()
    .filter_map(|(pane, shown)| shown.then_some(pane))
    .collect::<Vec<_>>();
    let order = order(app);
    open.sort_by_key(|pane| order.iter().position(|p| p == pane).unwrap_or(usize::MAX));
    open
}

/// Where a task pane other than Styles sits, kept in `ui.json` (#1756). Styles keeps its place in
/// `view.stylesPaneDock` and `stylesPaneFloat`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedPane {
    /// `None`: the pane's default edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dock: Option<SavedDock>,
    /// `[x, y, width, height]` of the floating pane, also kept while it is docked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float: Option<[f32; 4]>,
    /// The docked width this run; like the Styles pane's, not kept between runs.
    #[serde(skip)]
    pub width: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SavedDock {
    Left,
    Right,
    Float,
}

/// `panePlaces` as saved: unknown panes, places and rects are dropped, not the whole file.
pub(crate) fn lenient_places<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, SavedPane>, D::Error> {
    let v = <Value as serde::Deserialize>::deserialize(d)?;
    let Some(map) = v.as_object() else { return Ok(BTreeMap::new()) };
    let dock = |v: &Value| match v.as_str() {
        Some("left") => Some(SavedDock::Left),
        Some("right") => Some(SavedDock::Right),
        Some("float") => Some(SavedDock::Float),
        _ => None,
    };
    Ok(Pane::ALL
        .iter()
        .filter(|pane| **pane != Pane::Styles)
        .filter_map(|pane| {
            let saved = map.get(pane.key())?;
            let place = SavedPane { dock: saved.get("dock").and_then(dock), float: saved.get("float").and_then(float_rect), width: None };
            Some((pane.key().to_string(), place))
        })
        .collect())
}

/// Four finite numbers as `[x, y, width, height]`; `craft_ui::panes` brings a saved rect back
/// inside the window.
pub(crate) fn float_rect(v: &Value) -> Option<[f32; 4]> {
    let [x, y, w, h] = v.as_array().map(Vec::as_slice)? else { return None };
    let n = |v: &Value| v.as_f64().map(|n| n as f32).filter(|n| n.is_finite());
    n(x).zip(n(y)).zip(n(w)).zip(n(h)).map(|(((x, y), w), h)| [x, y, w, h])
}

/// A pane's place and size: Styles' from `view.stylesPaneDock` and its own preferences, the
/// others' from `panePlaces`.
fn pane_state(app: &WordApp, pane: Pane) -> PaneState {
    if pane == Pane::Styles {
        return styles_state(app);
    }
    let default = pane.default_state();
    let Some(saved) = app.ui.pane_places.get(pane.key()) else { return default };
    let place = match saved.dock {
        Some(SavedDock::Left) => Place::Docked(Edge::Start),
        Some(SavedDock::Right) => Place::Docked(Edge::End),
        Some(SavedDock::Float) => Place::Floating,
        None => default.place,
    };
    PaneState { place, size: saved.width.or(default.size), float_rect: saved.float }
}

/// The Styles pane's place and size, from `view.stylesPaneDock` and the UI preferences (#1756).
fn styles_state(app: &WordApp) -> PaneState {
    let place = match app.session.view.styles_pane_dock {
        StylesPaneDock::Right => Place::Docked(Edge::End),
        StylesPaneDock::Left => Place::Docked(Edge::Start),
        StylesPaneDock::Floating => Place::Floating,
    };
    PaneState { place, size: app.ui.styles_pane_width.or(Pane::Styles.default_state().size), float_rect: app.ui.styles_pane_float }
}

fn labels() -> PaneLabels {
    PaneLabels {
        menu: tl!("Pane Options").into(),
        move_start: tl!("Dock Left").into(),
        move_end: tl!("Dock Right").into(),
        float: tl!("Float").into(),
        close: tl!("Close").into(),
        move_pane: tl!("Move Pane: {title}").into(),
        resize: tl!("Resize Pane").into(),
        ..PaneLabels::default()
    }
}

struct Content<'a> {
    app: &'a mut WordApp,
}

impl PaneContent<Pane> for Content<'_> {
    fn title(&self, pane: &Pane) -> String {
        tl!(pane.title()).into()
    }

    /// App items above Move/Float/Close: the Navigation tree's style, so it can be switched
    /// without leaving the document (§5.2).
    fn menu(&mut self, ui: &mut Ui, pane: &Pane) {
        if *pane != Pane::Navigation {
            return;
        }
        let current = self.app.ui.nav_tree_style;
        for style in [NavTreeStyle::ZedLinear, NavTreeStyle::FleetConnected] {
            let mark = if current == style { "✓" } else { "  " };
            let label = format!("{mark} {}", tl!(style.label()));
            if ui.selectable_label(current == style, label).clicked() && current != style {
                set_nav_tree_style(self.app, style);
            }
        }
        ui.separator();
    }

    fn body(&mut self, ui: &mut Ui, pane: &Pane) {
        let app = &mut *self.app;
        match pane {
            Pane::SmartArtText => crate::dialogs_smart_art::text_pane(app, ui),
            Pane::Navigation => nav(app, ui),
            Pane::Clipboard => clipboard(app, ui),
            Pane::Styles => styles(app, ui),
            Pane::StyleInspector => inspector(app, ui),
            Pane::Accessibility => crate::a11y_pane::accessibility(app, ui),
            Pane::AltText => crate::a11y_pane::alt_text(app, ui),
            Pane::FormatShape => {
                egui::ScrollArea::vertical().show(ui, |ui| crate::format_shape::pane(app, ui));
            }
            Pane::RestrictEditing => crate::dialogs_review::restrict_pane(app, ui),
            Pane::Comments => comments(app, ui),
        }
    }

    fn default_state(&self, pane: &Pane) -> PaneState {
        pane.default_state()
    }
}

/// The pane host's name. Its focus and drag state is per window ([`WordApp::window_id`]); where
/// each pane sits is the app's.
const HOST: &str = "task_panes";

/// Whether a dialog, file browser, colors dialog, content-control popup, Zotero alert, keytips
/// or egui popup is open: the keyboard belongs to it, not to the panes or the document.
pub(crate) fn overlay_open(app: &WordApp, ctx: &egui::Context) -> bool {
    app.dialog.is_some()
        || app.file_dialog_open()
        || app.colors.is_some()
        || app.controls.popup_open()
        || app.zotero.alert_open()
        || app.ui.keytips != crate::keytips::Phase::Off
        || egui::Popup::is_any_open(ctx)
}

/// F6 and Shift+F6 move the keyboard through the panes and back to the document. The key stays
/// with an open dialog, file browser, popup or keytips (Customize Keyboard records it), and an
/// F6 the user bound to a command runs that command instead.
fn route_f6(app: &WordApp, ctx: &egui::Context) {
    if overlay_open(app, ctx) {
        return;
    }
    let reg = &app.session.registry;
    let bound = |m: egui::Modifiers| app.session.keymap.resolve(reg, &crate::keys::combo(m, "F6", true)).is_some();
    let mut cycle = None;
    ctx.input_mut(|input| {
        input.events.retain(|e| match e {
            egui::Event::Key { key: egui::Key::F6, pressed: true, modifiers: m, .. } if !m.command && !m.ctrl && !m.mac_cmd && !bound(*m) => {
                cycle = Some(m.shift);
                false
            }
            _ => true,
        });
    });
    if let Some(reverse) = cycle {
        craft_ui::panes::focus_next(ctx, app.window_id(HOST), reverse);
    }
}

/// Draw the open panes around the document. Call before the canvas's `CentralPanel`.
pub fn show(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let open = open(app);
    if !open.contains(&Pane::FormatShape) {
        crate::format_shape::forget_drag(ui.ctx());
    }
    route_f6(app, ui.ctx());
    let states: Vec<(Pane, PaneState)> = open.iter().map(|pane| (*pane, pane_state(app, *pane))).collect();
    let out = PaneHost::new(app.window_id(HOST))
        .policy(SidePolicy::Columns)
        .f6(false)
        .labels(labels())
        .frame(egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)))
        .body_margin(egui::Margin::same(10))
        .show(ui, &open, |pane| states.iter().find(|(p, _)| p == pane).map(|(_, state)| *state), &mut Content { app });
    for request in out.requests {
        apply(app, request);
    }
    if out.to_document {
        app.canvas.want_focus = true;
    }
}

/// Run a request from the pane host through the pane's commands and the UI preferences.
pub(crate) fn apply(app: &mut WordApp, request: PaneRequest<Pane>) {
    match request {
        PaneRequest::Close(pane) => close(app, pane),
        PaneRequest::Place(Pane::Styles, place) => {
            let dock = match place {
                Place::Docked(Edge::Start) => "left",
                Place::Docked(Edge::End) => "right",
                Place::Floating => "float",
                Place::Docked(Edge::Bottom) => return,
            };
            let _ = app.run("view.stylesPaneDock", json!({"dock": dock}));
        }
        PaneRequest::Resize(Pane::Styles, width) => app.ui.styles_pane_width = Some(width),
        PaneRequest::FloatRect(Pane::Styles, rect) => app.ui.styles_pane_float = Some(rect),
        PaneRequest::Place(pane, place) => {
            let dock = match place {
                Place::Docked(Edge::Start) => "left",
                Place::Docked(Edge::End) => "right",
                Place::Floating => "float",
                Place::Docked(Edge::Bottom) => return,
            };
            let _ = app.run("ui.paneDock", json!({"pane": pane.key(), "dock": dock}));
        }
        PaneRequest::Resize(pane, width) => app.ui.pane_places.entry(pane.key().into()).or_default().width = Some(width),
        PaneRequest::FloatRect(pane, rect) => app.ui.pane_places.entry(pane.key().into()).or_default().float = Some(rect),
        PaneRequest::Before(pane, before) => {
            let _ = app.run("ui.paneDock", json!({"pane": pane.key(), "before": before.map(Pane::key)}));
        }
        PaneRequest::Focus(_) => {}
    }
}

/// `ui.paneDock {"pane": key, "dock"?: "left" | "right" | "float", "before"?: key | null}` (#1756):
/// docks or floats a task pane, as its menu's Dock Left, Dock Right and Float do; without `dock`
/// it reads the pane's place. `pane` is the pane's `panePlaces` key; Styles goes through
/// `view.stylesPaneDock`. `before` puts the pane directly before that pane in the order, outside
/// it on their edge, as a drop over a docked pane does; `null` puts it after every pane.
pub(crate) fn dock_command(app: &mut WordApp, p: &Value) -> Result<Value, String> {
    let key = p.get("pane").and_then(Value::as_str).ok_or("`pane` must be a task pane's key, such as \"navigation\"")?;
    let pane = Pane::from_key(key).ok_or_else(|| format!("unknown task pane `{key}`"))?;
    let dock = match p.get("dock") {
        None => None,
        Some(v) => Some(match v.as_str() {
            Some("left") => SavedDock::Left,
            Some("right") => SavedDock::Right,
            Some("float") => SavedDock::Float,
            _ => return Err("`dock` must be left, right or float".into()),
        }),
    };
    let before = match p.get("before") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => match v.as_str().and_then(Pane::from_key) {
            Some(before) if before != pane => Some(Some(before)),
            _ => return Err("`before` must be another task pane's key or null".into()),
        },
    };
    if let Some(before) = before {
        move_before(app, pane, before);
    }
    if pane == Pane::Styles {
        let params = p.get("dock").map_or_else(|| json!({}), |d| json!({"dock": d}));
        let result = app.run("view.stylesPaneDock", params)?;
        return Ok(json!({"pane": key, "dock": result.get("dock")}));
    }
    if let Some(dock) = dock {
        app.ui.pane_places.entry(key.into()).or_default().dock = Some(dock);
    }
    let name = match pane_state(app, pane).place {
        Place::Docked(Edge::Start) => "left",
        Place::Floating => "float",
        Place::Docked(_) => "right",
    };
    Ok(json!({"pane": key, "dock": name}))
}

fn close(app: &mut WordApp, pane: Pane) {
    match pane.close_command() {
        Some(id) => {
            let _ = app.run(id, json!({"value": false}));
        }
        None => app.restrict.open = false,
    }
}

/// Draws the Navigation pane: search box and tabs, page list, find results and heading outline.
fn nav(app: &mut WordApp, ui: &mut Ui) {
    let qid = app.window_id("nav_query");
    let mut q = ui.data(|d| d.get_temp::<String>(qid)).unwrap_or_default();
    // Ctrl+F with text selected searches for it, as Word does (a selection within one line).
    let focus = std::mem::take(&mut app.nav_search_focus);
    let picked = focus.then(|| app.session.selected_text()).filter(|t| !t.is_empty() && !t.contains(['\n', '\r']) && t.chars().count() <= 255);
    if let Some(t) = &picked {
        q = t.clone();
    }
    let mut out = egui::TextEdit::singleline(&mut q).hint_text(tl!("Search document")).desired_width(f32::INFINITY).show(ui);
    let r = out.response.clone();
    if focus {
        // Focused with its text selected, so typing replaces it.
        r.request_focus();
        let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(q.chars().count()));
        out.state.cursor.set_char_range(Some(all));
        out.state.store(ui.ctx(), r.id);
    }
    if r.changed() || picked.is_some() {
        app.ui.nav_tab = "results".into();
        ui.data_mut(|d| d.insert_temp(qid, q.clone()));
        if !q.is_empty() {
            let _ = app.session.run("edit.find", &json!({"text": q}));
        } else {
            app.session.find.results.clear();
        }
    }
    ui.add_space(4.0);
    nav_tabs(ui, &mut app.ui.nav_tab);
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| match app.ui.nav_tab.as_str() {
        "pages" => {
            let l = app.session.layout();
            for (i, p) in l.pages.iter().enumerate() {
                let label = crate::i18n::fmt(tl!("Page {page}"), &[("page", &p.number.to_string())]);
                if ui.selectable_label(app.session.page_hint == i, label).clicked() {
                    let _ = app.run("edit.goto", json!({"page": i + 1}));
                }
            }
        }
        "results" => {
            let results = app.session.find.results.clone();
            ui.label(egui::RichText::new(format!("{} results", results.len())).small().weak());
            for (a, b) in results.iter().take(500) {
                let Some(p) = app.session.doc.para_at(a) else { continue };
                let from = p.clamp(a.off.saturating_sub(30));
                let to = p.clamp((b.off + 40).min(p.len()));
                let pre = p.text.get(from..a.off).unwrap_or("");
                let hit = p.text.get(a.off..b.off).unwrap_or("");
                let post = p.text.get(b.off..to).unwrap_or("");
                let mut job = egui::text::LayoutJob::default();
                let fmt = |c| egui::TextFormat { font_id: regular(12.0), color: c, ..Default::default() };
                job.append(pre, 0.0, fmt(ui.visuals().text_color()));
                // The match is marked the way Find marks it on the page, per theme (#656).
                let mark = Tokens::get(ui.ctx()).find_highlight;
                job.append(hit, 0.0, egui::TextFormat { font_id: semibold(12.0), background: mark, color: ui.visuals().text_color(), ..Default::default() });
                job.append(post, 0.0, fmt(ui.visuals().text_color()));
                job.wrap.max_width = ui.available_width();
                if ui.add(egui::Button::new(job).frame(false)).clicked() {
                    let _ = app.run("select.range", json!({"anchor": a, "focus": b}));
                }
                ui.separator();
            }
        }
        _ => {
            // Modelo 2: the rows touch, so the vertical trunks have no gaps between them.
            ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let caret = app.session.sel.focus.clone();
            let items = nav_headings(&app.session.doc);
            let current = items.iter().rposition(|(path, ..)| *path <= caret.path);
            if items.is_empty() {
                ui.label(egui::RichText::new(tl!("Create an interactive outline of your document.\n\nIt's a great way to keep track of where you are or quickly move your content around.\n\nTo get started, go to the Home tab and apply Heading styles to the headings in your document.")).weak());
            }
            // A heading has children when the next heading is a deeper level (#1997).
            let has_children: Vec<bool> = items.iter().enumerate().map(|(i, (_, lv, _))| items.get(i + 1).is_some_and(|n| n.1 > *lv)).collect();
            let collapsed_flags: Vec<bool> = items.iter().enumerate().map(|(i, (p, _, _))| has_children[i] && app.ui.nav_collapsed.contains(p)).collect();
            let levels: Vec<u8> = items.iter().map(|(_, lv, _)| *lv).collect();
            let visible = visible_headings(&levels, &collapsed_flags);
            let chain = active_chain(&levels, &visible, current);
            let tree_style = app.ui.nav_tree_style;
            let current_index = current;
            // The levels this heading nests under, for the tree's connector lines.
            let mut ancestors: Vec<u8> = Vec::new();
            for (i, (path, lv, txt)) in items.iter().enumerate() {
                if !visible[i] {
                    continue;
                }
                while ancestors.last().is_some_and(|a| *a >= *lv) {
                    ancestors.pop();
                }
                // The last child of its parent closes the guide line under it at the row's middle.
                let last = items
                    .iter()
                    .enumerate()
                    .skip(i + 1)
                    .find(|(j, _)| visible[*j])
                    .is_none_or(|(_, (_, next, _))| *next <= *lv);
                let collapsed = collapsed_flags[i];
                let row_id = ui.make_persistent_id(("nav_row", path));
                // The trunks of the active branch: each ancestor level whose parent row is on the
                // chain and which lies above (or at) the selected row.
                let active_guides = branch_guide_levels(&ancestors, &chain, &levels, i, current_index);
                let on_chain = chain.contains(&i);
                let (toggle, r) = nav_heading(
                    ui,
                    row_id,
                    *lv,
                    txt,
                    current == Some(i),
                    has_children[i].then_some(collapsed),
                    &ancestors,
                    last,
                    on_chain,
                    &active_guides,
                    tree_style,
                );
                if toggle {
                    if !app.ui.nav_collapsed.remove(path) {
                        app.ui.nav_collapsed.insert(path.clone());
                    }
                } else if r.clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space))) {
                    let _ = app.run("caret.set", json!({"pos": Pos { story: StoryRef::Body, path: path.clone(), off: 0 }}));
                    // Align the chosen heading's page immediately against this pane, rather
                    // than the indeterminate minimum movement from scroll_to_rect.
                    app.canvas.nav_to_caret = true;
                    app.canvas.scroll_to_caret = false;
                    app.canvas.want_focus = true;
                }
                ancestors.push(*lv);
            }
            });
        }
    });
}

/// Which headings the pane shows: those under a collapsed heading (every following heading
/// of a deeper level) are hidden (#1997).
fn visible_headings(levels: &[u8], collapsed: &[bool]) -> Vec<bool> {
    let mut hide_below: Option<u8> = None;
    levels
        .iter()
        .zip(collapsed)
        .map(|(&lv, &c)| {
            if hide_below.is_some_and(|h| lv > h) {
                return false;
            }
            hide_below = c.then_some(lv);
            true
        })
        .collect()
}

/// The body's headings (paragraphs with an outline level) in document order, as (path, level,
/// text). A numbered heading starts with the number it shows (`2.1 Method`), as in Word.
fn nav_headings(doc: &Document) -> Vec<(Path, u8, String)> {
    // Every numbered paragraph advances the list counters, headings or not.
    let mut counters = Counters::default();
    let mut items = Vec::new();
    for path in doc.para_paths(StoryRef::Body) {
        let Some(p) = doc.para(StoryRef::Body, &path) else { continue };
        let rp = doc.styles.resolve_para(&p.props);
        let number = p
            .props
            .numbering
            .or(rp.numbering)
            .filter(|n| n.num != 0)
            .and_then(|n| counters.next_label(&doc.numbering, n.num, n.level))
            .filter(|(label, def)| !label.is_empty() && !matches!(def.format, NumFormat::Bullet | NumFormat::None));
        if let Some(lv) = rp.outline_level {
            let txt = p.plain_text().trim().to_string();
            if txt.is_empty() || rp.style.starts_with("TOC") {
                continue;
            }
            let txt = match number {
                Some((label, def)) if def.suffix == LevelSuffix::Nothing => format!("{label}{txt}"),
                Some((label, _)) => format!("{label} {txt}"),
                None => txt,
            };
            items.push((path, lv, txt));
        }
    }
    items
}

/// Modelo 2 — Curva Orgânica Fluida, options A and B, from the reference SVG's coordinates: the
/// node of level 0 sits at x=18, a 16 px step per level (level 1 at 34, level 2 at 50), the badge
/// of a parent 12 px after its node, a 14 px icon square and a 6 px gap after it. The elbow is a
/// quadratic of radius 8 (the SVG's Q 18 96 26 96 L 30 96).
const NAV_TRUNK_WIDTH: f32 = 1.3;
const NAV_TRUNK_ACTIVE_WIDTH: f32 = 1.6;
const NAV_ROW_HEIGHT: f32 = 26.0;
/// From the right of the node slot (or the indent, for a leaf) to the keycap badge/label.
const NAV_CONTENT_GAP: f32 = 6.0;
/// The mockup's keycap badge: a 14 px square.
const NAV_ICON_BOX: f32 = 14.0;
/// The colour emoji's box: the SVG's emoji placeholders are 11–13 px, and 13 keeps the following
/// text on the SVG's x=67 column (the title's own space after the emoji supplies the 6 px gap).
const NAV_EMOJI_BOX: f32 = 13.0;
/// The selection pill starts this far left of the content, where the active curve ends.
const NAV_PILL_INSET: f32 = 2.0;
/// The node column of level 0 and the step between levels (SVG: 18, 34, 50…).
const NAV_NODE_X0: f32 = 18.0;
const NAV_COLUMN_STEP: f32 = 16.0;
/// A parent's badge sits this far after its node (SVG: node 34, badge 46).
const NAV_PARENT_BADGE_DX: f32 = 12.0;
/// Radius of the curved elbow in the reference SVG (8 px).
const NAV_CURVE_RADIUS: f32 = 8.0;
/// The pane body's own left padding (frame + scroll area): the SVG's coordinates start at the
/// pane's edge, so the rows are drawn this much to the left to land on the same columns.
const NAV_PANE_TRIM: f32 = 18.5;

/// Radius of a parent's circular chevron node (7 px at level 0, 6.5 px below, as the mockup).
fn nav_node_radius(level: u8) -> f32 {
    if level == 0 { 7.0 } else { 6.5 }
}

/// Radius of the curved elbow (the reference SVG's 8 px).
fn nav_curve_radius(_level: u8) -> f32 {
    NAV_CURVE_RADIUS
}

/// The X of the trunk (and of the node) of a level: 18, 34, 50…
fn nav_node_x(level: u8) -> f32 {
    NAV_NODE_X0 + level as f32 * NAV_COLUMN_STEP
}

/// The badge/icon column of a row: a parent's badge follows its node by 12; a leaf starts at its
/// level's column.
fn nav_content_x(level: u8) -> f32 {
    nav_node_x(level) + NAV_PARENT_BADGE_DX
}

/// One trunk segment painted on a row.
#[derive(Clone, Debug, PartialEq)]
struct NavTrunk {
    x: f32,
    top: f32,
    bottom: f32,
    active: bool,
}

/// The curved elbow into the row: a quadratic Bézier from the trunk to the horizontal, then the
/// straight part to the content.
#[derive(Clone, Debug, PartialEq)]
struct NavCurve {
    start: egui::Pos2,
    control: egui::Pos2,
    end: egui::Pos2,
    horizontal_end: f32,
    active: bool,
}

/// A parent row's circular chevron node.
#[derive(Clone, Debug, PartialEq)]
struct NavNode {
    centre: egui::Pos2,
    radius: f32,
    expanded: bool,
    active: bool,
}

/// Everything one row paints, computed without a GPU so the geometry can be tested.
#[derive(Clone, Debug, Default, PartialEq)]
struct NavGeometry {
    content_x: f32,
    pill: Option<egui::Rect>,
    trunks: Vec<NavTrunk>,
    curves: Vec<NavCurve>,
    node: Option<NavNode>,
}

/// The geometry of one heading row (pure): trunks, the curved elbow, the chevron node and the
/// selection pill. `style` decides whether an inactive leaf gets its own elbow (B) or not (A).
#[allow(clippy::too_many_arguments)]
fn nav_row_geometry(
    row: egui::Rect,
    level: u8,
    guides: &[u8],
    last: bool,
    active: bool,
    active_guides: &[u8],
    style: NavTreeStyle,
    collapsed: Option<bool>,
    selected: bool,
) -> NavGeometry {
    let mid = row.center().y;
    let left = row.left() - NAV_PANE_TRIM;
    let radius = nav_node_radius(level);
    let content_x = left + nav_content_x(level);
    let mut g = NavGeometry { content_x, ..Default::default() };
    if selected {
        // SVG: pill x=48 for content x=50, height 24, right edge 13 px from the pane's.
        let h = 24.0;
        let top = mid - h * 0.5;
        g.pill = Some(egui::Rect::from_min_max(egui::pos2(content_x - NAV_PILL_INSET, top), egui::pos2(row.right() - 13.0, top + h)));
    }
    // A parent's circular chevron node sits on its own column's trunk; the trunk goes through it
    // (entering above, leaving below), never across the chevron.
    if let Some(expanded) = collapsed.map(|c| !c) {
        let centre = egui::pos2(left + nav_node_x(level), mid);
        let stroke_active = active;
        g.trunks.push(NavTrunk { x: centre.x, top: row.top(), bottom: centre.y - radius, active: stroke_active });
        g.trunks.push(NavTrunk { x: centre.x, top: centre.y + radius, bottom: row.bottom(), active: stroke_active });
        g.node = Some(NavNode { centre, radius, expanded, active });
    }
    // Trunks of every ancestor level; the innermost one ends at the curve when it is the last
    // child (no loose end below it) and the row has an elbow.
    if let Some(innermost) = guides.last().copied() {
        let curve = nav_curve_radius(innermost);
        let draw_curve = style == NavTreeStyle::FleetConnected || active;
        for &a in guides {
            let lit = active_guides.contains(&a);
            let bottom = if last && a == innermost && draw_curve { mid - curve } else { row.bottom() };
            g.trunks.push(NavTrunk { x: left + nav_node_x(a), top: row.top(), bottom, active: lit });
        }
        if draw_curve {
            let x = left + nav_node_x(innermost);
            // A parent's elbow reaches its node (SVG: curve to x=30 for the node at 34); a leaf's
            // elbow reaches the pill (SVG 48) or stops 3 px before its badge/emoji.
            let horizontal_end = match &g.node {
                Some(node) => node.centre.x - 4.0,
                None if selected => content_x - NAV_PILL_INSET,
                None => content_x - 3.0,
            };
            if horizontal_end > x {
                g.curves.push(NavCurve {
                    start: egui::pos2(x, mid - curve),
                    control: egui::pos2(x, mid),
                    end: egui::pos2(x + curve, mid),
                    horizontal_end,
                    active: active || active_guides.contains(&innermost),
                });
            }
        }
    }
    g
}

/// The child rows of a level under a node keep the trunk continuous, so the node's lower stub and
/// the first child's trunk meet without a gap.
#[cfg(test)]
fn trunk_has_no_gap(a: &NavTrunk, b: &NavTrunk) -> bool {
    (a.x - b.x).abs() < 0.01 && (a.bottom - b.top).abs() < 0.01
}

/// The row indices on the caret's branch, from the root down to the selected heading.
fn active_chain(levels: &[u8], visible: &[bool], current: Option<usize>) -> Vec<usize> {
    let mut chain = Vec::new();
    let Some(mut i) = current else { return chain };
    if !visible.get(i).copied().unwrap_or(false) {
        return chain;
    }
    chain.push(i);
    let mut level = levels[i];
    loop {
        let parent = (0..i).rev().find(|&j| visible.get(j).copied().unwrap_or(false) && levels[j] < level);
        let Some(j) = parent else { break };
        chain.push(j);
        i = j;
        level = levels[j];
    }
    chain.reverse();
    chain
}

/// The guide levels of row `i` whose trunk belongs to the active branch: an ancestor level is lit
/// when its parent row is on the chain and the row lies between that parent and the selected row,
/// which carries the light through the siblings passed on the way down.
fn branch_guide_levels(ancestors: &[u8], chain: &[usize], levels: &[u8], i: usize, current: Option<usize>) -> Vec<u8> {
    let Some(current) = current else { return Vec::new() };
    ancestors.iter().copied().filter(|a| chain.iter().any(|&k| levels.get(k) == Some(a) && k <= i) && i <= current).collect()
}

/// A heading that starts with a keycap (`1️⃣`, `#️⃣`): the digit and the text after the space
/// that follows it. The interface fonts have no keycap ligature (they would draw the digit and
/// drop the enclosing mark), so the pane draws the keycap as the badge the page shows.
fn keycap_prefix(text: &str) -> Option<(char, &str)> {
    let mut it = text.char_indices();
    let (_, digit) = it.next()?;
    if !(digit.is_ascii_digit() || digit == '#' || digit == '*') {
        return None;
    }
    for (i, c) in it {
        match c {
            // The variation selector is optional (a keycap may be digit + enclosing keycap).
            '\u{FE0E}' | '\u{FE0F}' => {}
            '\u{20E3}' => {
                let end = i + c.len_utf8();
                let rest = text.get(end..).unwrap_or("");
                return Some((digit, rest.strip_prefix(' ').unwrap_or(rest)));
            }
            _ => return None,
        }
    }
    None
}

/// Byte ranges of the emoji sequences in a heading's text, in order.
fn emoji_clusters(text: &str) -> Vec<(usize, usize)> {
    let starts = wordcraft_doc::cluster::cluster_starts(text);
    let mut out = Vec::new();
    for (i, &s) in starts.iter().enumerate() {
        let e = starts.get(i + 1).copied().unwrap_or(text.len());
        if let Some(c) = text.get(s..e) {
            // Presentation sequences (`⚖️`, keycaps, ZWJ) and characters that are emoji by
            // themselves (`📘`): the pane paints both from the colour emoji font.
            let chars: Vec<char> = c.chars().collect();
            if wordcraft_fonts::emoji_sequence(&chars) || chars.first().is_some_and(|&f| wordcraft_fonts::is_emoji(f)) {
                out.push((s, e));
            }
        }
    }
    out
}

/// The interface's colour image for an emoji, kept between frames (rasterising it again every
/// frame would burn the CPU for nothing: the pane redraws while typing).
fn emoji_texture(ui: &Ui, text: &str, px: u32) -> Option<egui::TextureHandle> {
    let id = egui::Id::unique(("nav_emoji", text, px));
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let img = wordcraft_render::emoji_image(text, px)?;
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba);
    let tex = ui.ctx().load_texture(format!("nav-emoji-{text}"), color, egui::TextureOptions::LINEAR);
    ui.ctx().data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Paint the colour images of the emoji in `text` over the glyphs the pane's (transparent) text
/// found for them. The interface fonts have emoji only in monochrome; the page draws them from the
/// colour font, and the Navigator follows the page rather than showing grey shapes.
fn paint_emoji(ui: &Ui, painter: &egui::Painter, pos: egui::Pos2, galley: &egui::Galley, text: &str) {
    let ppp = ui.ctx().pixels_per_point();
    let mut glyphs = galley.rows.iter().flat_map(|r| r.glyphs.iter());
    for (s, e) in emoji_clusters(text) {
        let Some(first) = text.get(s..e).and_then(|c| c.chars().next()) else { continue };
        // One glyph per drawn character, in order: the cluster's first character finds its box.
        let Some(g) = glyphs.find(|g| g.chr == first) else { continue };
        let baseline = pos.y + g.pos.y;
        // One icon column, as in the mockup: every emoji fits a 14 px box (height) and can never
        // be wider than its slot (advance + the explicit gap), so a wide emoji neither swells nor
        // touches the following letter; the dominant dimension is capped, so sizes match.
        let box_side = NAV_EMOJI_BOX.max(1.0);
        // The image fills only the glyph's advance; the explicit letter gap after the placeholder
        // is what separates it from the next letter, as the mockup's 6 px icon gap does.
        let slot_w = g.advance_width.max(6.0);
        let Some(tex) = emoji_texture(ui, &text[s..e], (box_side * ppp).round().max(1.0) as u32) else { continue };
        let size = tex.size_vec2();
        let aspect = (size.x / size.y.max(1.0)).clamp(0.2, 5.0);
        let mut h = box_side;
        let mut w = h * aspect;
        if w > slot_w {
            w = slot_w;
            h = w / aspect;
        }
        let top = baseline - h * 0.86;
        let rect = egui::Rect::from_min_size(egui::pos2(pos.x + g.pos.x, top), vec2(w, h));
        painter.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
}

/// One heading in the pane: its level's indent, the expand/collapse triangle, the keycap badge
/// when the heading starts with one, and the label — one line, ended with an ellipsis when it
/// doesn't fit, as Writer's Navigator does (the whole heading shows on hover). `guides` are the
/// levels of the headings this one nests under: connector lines run down their slots and across
/// to this row, and `last` closes the line under its parent at the row's middle.
///
/// The row paints itself rather than using a Button: the label mixes text with colour emoji
/// images, which a single widget's text cannot. `id` keeps its hover/selection and click stable.
/// The segmented `[ Headings | Pages | Results ]` control: one line, three equal columns, a
/// dark container with a hairline border and a filled blue active tab (the mockup's style).
fn nav_tabs(ui: &mut Ui, current: &mut String) {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().max(120.0);
    let height = 24.0;
    let (outer, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(outer, 6.0, t.nav_tab_bg);
    ui.painter().rect_stroke(outer, 6.0, Stroke::new(1.0, t.nav_tab_border), egui::StrokeKind::Inside);
    let cell = width / 3.0;
    for (i, (key, label)) in [("headings", "Headings"), ("pages", "Pages"), ("results", "Results")].into_iter().enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(outer.left() + i as f32 * cell, outer.top()), vec2(cell, height));
        let resp = ui.interact(rect, ui.scope_id().with(("nav_tab", key)), egui::Sense::click());
        let on = current.as_str() == key;
        if on {
            ui.painter().rect_filled(rect.shrink(2.0), 5.0, t.nav_tab_active);
        }
        let colour = if on { t.nav_tab_active_text } else { ui.visuals().text_color() };
        let font = if on { semibold(11.5) } else { regular(11.5) };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, tl!(label), font, colour);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, tl!(label)));
        if resp.clicked() {
            *current = key.to_string();
        }
    }
}

/// Paint the computed geometry of a row: trunks (with the active branch's glow), the quadratic
/// elbow, and the circular chevron node.
fn paint_nav_geometry(painter: &egui::Painter, g: &NavGeometry, t: &Tokens) {
    for trunk in &g.trunks {
        let stroke = if trunk.active { Stroke::new(NAV_TRUNK_ACTIVE_WIDTH, t.nav_trunk_active) } else { Stroke::new(NAV_TRUNK_WIDTH, t.nav_trunk) };
        if trunk.active {
            let glow = Stroke::new(NAV_TRUNK_ACTIVE_WIDTH + 2.0, t.nav_trunk_active.gamma_multiply(0.35));
            painter.vline(trunk.x, trunk.top..=trunk.bottom, glow);
        }
        painter.vline(trunk.x, trunk.top..=trunk.bottom, stroke);
    }
    for curve in &g.curves {
        let stroke = if curve.active { Stroke::new(NAV_TRUNK_ACTIVE_WIDTH, t.nav_curve_active) } else { Stroke::new(NAV_TRUNK_WIDTH, t.nav_curve) };
        if curve.active {
            painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
                [curve.start, curve.control, curve.end],
                false,
                egui::Color32::TRANSPARENT,
                Stroke::new(NAV_TRUNK_ACTIVE_WIDTH + 2.0, t.nav_curve_active.gamma_multiply(0.35)),
            ));
            painter.hline(
                curve.end.x..=curve.horizontal_end,
                curve.end.y,
                Stroke::new(NAV_TRUNK_ACTIVE_WIDTH + 2.0, t.nav_curve_active.gamma_multiply(0.35)),
            );
        }
        painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [curve.start, curve.control, curve.end],
            false,
            egui::Color32::TRANSPARENT,
            stroke,
        ));
        if curve.horizontal_end > curve.end.x {
            painter.hline(curve.end.x..=curve.horizontal_end, curve.end.y, stroke);
        }
    }
    if let Some(node) = &g.node {
        let (fill, border, chevron) = if node.active {
            (t.nav_node_fill_active, t.nav_node_border_active, t.nav_chevron_active)
        } else {
            (t.nav_node_fill, t.nav_node_border, t.nav_chevron)
        };
        painter.circle_filled(node.centre, node.radius, fill);
        painter.circle_stroke(node.centre, node.radius, Stroke::new(if node.active { 1.35 } else { 1.2 }, border));
        let (c, half) = (node.centre, node.radius);
        let stroke = Stroke::new(1.5, chevron);
        let pts = if node.expanded {
            [egui::pos2(c.x - half * 0.55, c.y - half * 0.25), egui::pos2(c.x, c.y + half * 0.35), egui::pos2(c.x + half * 0.55, c.y - half * 0.25)]
        } else {
            [egui::pos2(c.x - half * 0.25, c.y - half * 0.55), egui::pos2(c.x + half * 0.35, c.y), egui::pos2(c.x - half * 0.25, c.y + half * 0.55)]
        };
        painter.line_segment([pts[0], pts[1]], stroke);
        painter.line_segment([pts[1], pts[2]], stroke);
    }
}

/// The one entry point the two toggles (Options and the pane's ▾ menu) use: the command validates
/// the value, saves it to `ui.json` and marks the preferences for an immediate write.
pub(crate) fn set_nav_tree_style(app: &mut WordApp, style: NavTreeStyle) {
    if app.ui.nav_tree_style != style
        && let Err(e) = app.run("ui.navTreeStyle", json!({"value": style.code()}))
    {
        app.status(e);
    }
}

/// One heading in the pane: the trunk/elbow/node geometry, the keycap badge, the emoji and the
/// label. `active` is the caret's branch (and the selected row); `style` switches between A and B.
#[allow(clippy::too_many_arguments)]
fn nav_heading(
    ui: &mut Ui,
    id: egui::Id,
    level: u8,
    text: &str,
    selected: bool,
    collapsed: Option<bool>,
    guides: &[u8],
    last: bool,
    active: bool,
    active_guides: &[u8],
    style: NavTreeStyle,
) -> (bool, egui::Response) {
    let t = Tokens::get(ui.ctx());
    let row_h = NAV_ROW_HEIGHT;
    let keycap = keycap_prefix(text);
    let label_text = keycap.map_or(text, |(_, rest)| rest);
    let radius = nav_node_radius(level);
    let has_children = collapsed.is_some();
    let row_w = ui.available_width().max(0.0);
    let content_x = nav_content_x(level);
    let badge_w = keycap.map_or(0.0, |_| NAV_ICON_BOX + NAV_CONTENT_GAP);
    // The selection pill insets its content by 5 px (SVG: pill x=48, icon x=55).
    let shift = if selected { 5.0 } else { 0.0 };
    let label_x = content_x + badge_w + shift;
    let gap = ui.spacing().item_spacing.x;
    let label_w = (row_w - label_x - 2.0 * gap).max(24.0);

    let (row, _) = ui.allocate_exact_size(vec2(row_w, row_h), egui::Sense::hover());
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let geometry = nav_row_geometry(row, level, guides, last, active, active_guides, style, collapsed, selected);
    let painter = ui.painter().with_clip_rect(ui.clip_rect().expand2(vec2(NAV_PANE_TRIM, 0.0)));
    paint_nav_geometry(&painter, &geometry, &t);
    if let Some(pill) = geometry.pill {
        painter.rect_filled(pill, 6.0, t.nav_pill_fill);
        painter.rect_stroke(pill, 6.0, Stroke::new(1.2, t.nav_pill_border), egui::StrokeKind::Inside);
    } else if hovered {
        let hover = egui::Rect::from_min_max(
            egui::pos2(row.left() - NAV_PANE_TRIM + content_x - NAV_PILL_INSET, row.center().y - row_h * 0.5),
            egui::pos2(row.right() - 6.0, row.center().y + row_h * 0.5),
        );
        painter.rect_filled(hover, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }

    // The label: text, with the emoji laid out transparent so their colour images can sit on top.
    let font = if active || selected { semibold(12.0) } else { regular(12.0) };
    let ink = egui::TextFormat { font_id: font.clone(), color: ui.visuals().text_color(), ..Default::default() };
    // The emoji placeholder keeps the regular metrics (bold would change its advance and crowd
    // the next letter), plus an explicit gap that the image can use without reaching the text.
    let clear = egui::TextFormat { font_id: regular(12.0), color: egui::Color32::TRANSPARENT, ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    let mut prev = 0;
    for (s, e) in emoji_clusters(label_text) {
        if prev < s {
            job.append(&label_text[prev..s], 0.0, ink.clone());
        }
        job.append(&label_text[s..e], 0.0, clear.clone());
        prev = e;
    }
    if prev < label_text.len() {
        job.append(&label_text[prev..], 0.0, ink.clone());
    }
    job.wrap = egui::text::TextWrapping { max_width: label_w, max_rows: 1, break_anywhere: true, overflow_character: Some('\u{2026}') };
    let galley = ui.painter().layout_job(job);

    let mut toggled = false;
    let label = ui
        .scope_builder(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            // The SVG's columns are exact; any implicit spacing between the row's pieces would
            // shift every badge/icon column by a few points, so only the explicit gaps count.
            ui.spacing_mut().item_spacing.x = 0.0;
            // Every row reserves the same chevron gap, whether or not it has children: the button
            // must never push one sibling's title deeper than another's. A leaf leaves the slot
            // empty (the geometry only paints a node for parents).
            ui.add_space((nav_node_x(level) - radius).max(0.0));
            let (node_rect, _) = ui.allocate_exact_size(vec2(2.0 * radius, row_h), egui::Sense::hover());
            if has_children {
                let node_resp = ui.interact(node_rect.translate(vec2(-NAV_PANE_TRIM, 0.0)), id.with("node"), egui::Sense::click());
                if node_resp.clicked() {
                    toggled = true;
                }
                node_resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, collapsed == Some(false), tl!("Expand or collapse")));
            }
            ui.add_space((content_x - (nav_node_x(level) + radius)).max(0.0));
            if selected {
                ui.add_space(shift);
            }
            if let Some((digit, _)) = keycap {
                let (slot, _) = ui.allocate_exact_size(vec2(NAV_ICON_BOX, row_h), egui::Sense::hover());
                let badge = egui::Rect::from_center_size(slot.center() + vec2(-NAV_PANE_TRIM, 0.0), vec2(NAV_ICON_BOX, NAV_ICON_BOX));
                let fill = if active || selected { t.nav_badge_active } else { t.nav_badge };
                painter.rect_filled(badge, 3.0, fill);
                painter.text(badge.center(), egui::Align2::CENTER_CENTER, digit, semibold(9.5), egui::Color32::WHITE);
                ui.add_space(NAV_CONTENT_GAP);
            }
            let width = ui.available_width().max(0.0);
            let (label_rect, resp) = ui.allocate_exact_size(vec2(width, row_h), egui::Sense::click());
            let pos = label_rect.left_top() + vec2(-NAV_PANE_TRIM, ((row_h - galley.size().y) * 0.5).max(0.0));
            painter.galley(pos, galley.clone(), ui.visuals().text_color());
            paint_emoji(ui, &painter, pos, &galley, label_text);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, text));
            ui.interact(label_rect.translate(vec2(-NAV_PANE_TRIM, 0.0)), id, egui::Sense::click())
        })
        .inner;

    if ui.ctx().memory(|m| m.has_focus(id)) {
        ui.painter().rect_stroke(row.shrink(1.0), 4.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    }
    (toggled, label.on_hover_text(text))
}
/// Items collected by Copy and Cut, newest first; clicking one pastes it at the caret.
fn clipboard(app: &mut WordApp, ui: &mut Ui) {
    let items: Vec<String> = app.session.clip_history.items().iter().map(|i| i.preview()).collect();
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!items.is_empty(), egui::Button::new(tl!("Paste All"))).clicked() {
            let _ = app.run("edit.pasteAllClipboard", json!({}));
            app.canvas.want_focus = true;
        }
        if ui.add_enabled(!items.is_empty(), egui::Button::new(tl!("Clear All"))).clicked() {
            let _ = app.run("edit.clearClipboard", json!({}));
            app.canvas.want_focus = true;
        }
    });
    ui.add_space(4.0);
    let hint = if items.is_empty() { "Nothing collected yet. Items you copy or cut appear here." } else { "Click an item to paste it." };
    ui.label(egui::RichText::new(tl!(hint)).small().weak());
    ui.separator();
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, preview) in items.iter().enumerate() {
            egui::Frame::NONE.fill(t.input).stroke(Stroke::new(1.0, t.border)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let text = if preview.is_empty() { tl!("(picture or object)").to_string() } else { preview.clone() };
                let mut job =
                    egui::text::LayoutJob::single_section(text, egui::TextFormat { font_id: regular(12.5), color: t.text, ..Default::default() });
                job.wrap.max_width = ui.available_width();
                job.wrap.max_rows = 3;
                if ui.add(egui::Button::new(job).frame(false)).on_hover_text(tl!("Paste")).clicked() {
                    let _ = app.run("edit.pasteClipboardItem", json!({"index": i}));
                    app.canvas.want_focus = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.small_button(tl!("Delete")).clicked() {
                        let _ = app.run("edit.deleteClipboardItem", json!({"index": i}));
                        app.canvas.want_focus = true;
                    }
                });
            });
            ui.add_space(6.0);
        }
    });
}

fn styles(app: &mut WordApp, ui: &mut Ui) {
    let st = app.session.run("format.state", &json!({})).unwrap_or_default();
    let cur = st.get("style").and_then(Value::as_str).unwrap_or("Normal").to_string();
    let current_name = app
        .session
        .doc
        .styles
        .get(&cur)
        .map(crate::i18n::style_name)
        .unwrap_or_else(|| st.get("styleName").and_then(Value::as_str).unwrap_or("").to_string());
    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("Current style: {style}"), &[("style", &current_name)])).small());
    // Wrapped: the host gives the pane a fixed width, and the row is wider than its default.
    ui.horizontal_wrapped(|ui| {
        if ui.button(tl!("New Style…")).clicked() {
            app.dialog = crate::dialogs::Dialog::open("newStyle", app);
        }
        if ui.button(tl!("Update to Match")).clicked() {
            let _ = app.run("styles.updateToMatch", json!({}));
        }
        if ui.button(tl!("Clear")).clicked() {
            let _ = app.run("format.clear", json!({}));
        }
    });
    if ui.button(tl!("Manage Styles…")).clicked() {
        let _ = app.run("styles.manage", json!({}));
    }
    if ui.selectable_label(app.session.view.style_inspector, tl!("Style Inspector")).clicked() {
        let _ = app.run("styles.inspector", json!({}));
    }
    ui.separator();
    let mut list: Vec<(String, String, bool, bool)> = app
        .session
        .doc
        .styles
        .styles
        .iter()
        .filter(|s| !s.hidden && s.kind != wordcraft_doc::StyleKind::Table)
        .map(|s| (s.id.clone(), crate::i18n::style_name(s), s.kind == wordcraft_doc::StyleKind::Character, s.builtin))
        .collect();
    list.sort_by_key(|s| s.1.to_lowercase());
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (id, name, is_char, builtin) in list {
            ui.horizontal(|ui| {
                let r = ui.selectable_label(id == cur, &name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(if is_char { "a" } else { "¶" }).weak());
                });
                if r.clicked() {
                    let _ = app.run("para.style", json!({"style": id}));
                    app.canvas.want_focus = true;
                }
                r.context_menu(|ui| {
                    if ui.button(tl!("Modify…")).clicked() {
                        app.dialog = crate::dialogs::Dialog::modify_style(app, &id);
                        ui.close();
                    }
                    if ui.button(tl!("Update to Match Selection")).clicked() {
                        let _ = app.run("styles.updateToMatch", json!({"style": id}));
                        app.canvas.want_focus = true;
                        ui.close();
                    }
                    if ui.add_enabled(!builtin, egui::Button::new(tl!("Delete"))).clicked() {
                        let _ = app.run("styles.delete", json!({"style": id}));
                        app.canvas.want_focus = true;
                        ui.close();
                    }
                });
            });
        }
    });
}

/// Draws the Comments pane: New Comment and scrollable comment and reply cards with edit, go-to,
/// reply, resolve or reopen, and delete actions.
fn comments(app: &mut WordApp, ui: &mut Ui) {
    if crate::widgets::icon_text_button(ui, "newComment", tl!("New Comment"), regular(12.5)).clicked() {
        let _ = app.run("review.newComment", json!({"text": ""}));
    }
    ui.separator();
    let list = app.session.run("review.comments", &json!({})).unwrap_or_default();
    let t = Tokens::get(ui.ctx());
    // Comment ids are the document's own; another window's document has the same ones.
    let document = app.session.document_id();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for c in list.as_array().cloned().unwrap_or_default() {
            let id = c.get("id").and_then(Value::as_u64).unwrap_or(0);
            let reply = c.get("parent").is_some_and(|p| !p.is_null());
            let when = u32::try_from(id).ok().and_then(|i| app.session.doc.comments.get(&i)).map(crate::local_time::comment_time).unwrap_or_default();
            egui::Frame::NONE.fill(t.input).stroke(Stroke::new(1.0, t.border)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if reply {
                    ui.add_space(0.0);
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(c.get("author").and_then(Value::as_str).unwrap_or("")).font(semibold(12.0)));
                    ui.label(egui::RichText::new(when).small().weak());
                    if c.get("resolved").and_then(Value::as_bool).unwrap_or(false) {
                        ui.label(egui::RichText::new(tl!("Resolved")).small().color(t.green));
                    }
                });
                // Editable comment text (one undo step per edit, like the balloons).
                comment_editor(app, ui, id as u32, None, false);
                let open_key = egui::Id::unique(("comment_reply_open", document, id));
                let mut replying = ui.data(|d| d.get_temp::<bool>(open_key)).unwrap_or(false);
                ui.horizontal(|ui| {
                    if ui.small_button(tl!("Go to")).clicked()
                        && let Some(a) = c.get("anchor").filter(|a| !a.is_null())
                    {
                        let _ = app.run("caret.set", json!({"pos": a}));
                    }
                    if ui.small_button(tl!("Reply")).clicked() {
                        replying = true;
                        ui.data_mut(|d| d.insert_temp(open_key, true));
                        ui.data_mut(|d| d.insert_temp(egui::Id::unique(("comment_reply_focus", document, id)), true));
                    }
                    let resolved = c.get("resolved").and_then(Value::as_bool).unwrap_or(false);
                    if ui.small_button(if resolved { tl!("Reopen") } else { tl!("Resolve") }).clicked() {
                        let _ = app.run("review.resolveComment", json!({"id": id}));
                    }
                    if ui.small_button(tl!("Delete")).clicked() {
                        let _ = app.run("review.deleteComment", json!({"id": id}));
                    }
                });
                if replying {
                    let focus = ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::unique(("comment_reply_focus", document, id)))).unwrap_or(false);
                    if reply_editor(app, ui, id as u32, None, focus) {
                        ui.data_mut(|d| d.remove::<bool>(open_key));
                    }
                }
            });
            ui.add_space(6.0);
        }
    });
}

/// What has been typed in comment and reply fields but not yet written to the document (#1348).
/// Drafts belong to one document ([`Session::document_id`](wordcraft_engine::Session::document_id)):
/// when another document replaces it they are dropped, so a comment there with the same id never
/// shows them. A draft is removed only once its command succeeds; a refused edit keeps it.
#[derive(Default)]
pub(crate) struct CommentDrafts {
    document: u64,
    edits: BTreeMap<u32, String>,
    replies: BTreeMap<u32, String>,
}

impl CommentDrafts {
    /// The drafts for `document`, dropping any left from another one.
    fn of(&mut self, document: u64) -> &mut Self {
        if self.document != document {
            self.edits.clear();
            self.replies.clear();
            self.document = document;
        }
        self
    }

    /// Nothing typed and not yet written.
    pub(crate) fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.replies.is_empty()
    }
}

/// The pending draft of comment `id`'s text in the open document, if any.
#[cfg(test)]
pub(crate) fn comment_draft(app: &WordApp, id: u32) -> Option<String> {
    let d = &app.comment_drafts;
    if d.document == app.session.document_id() { d.edits.get(&id).cloned() } else { None }
}

/// Whether the open document has a comment or reply draft not yet written to it (typed, or
/// kept after the document refused it): closing the page would lose it.
pub(crate) fn has_comment_drafts(app: &WordApp) -> bool {
    let d = &app.comment_drafts;
    d.document == app.session.document_id() && !(d.edits.is_empty() && d.replies.is_empty())
}

fn drafts(app: &mut WordApp) -> &mut CommentDrafts {
    let document = app.session.document_id();
    app.comment_drafts.of(document)
}

/// A comment's text, editable in place (the Comments pane and the balloons). Typing stays in a
/// draft until the field loses focus; then [`commit_comment`] writes it back with one
/// `review.editComment`, so each editing session is one undo step.
pub(crate) fn comment_editor(app: &mut WordApp, ui: &mut Ui, id: u32, font: Option<egui::FontId>, focus: bool) {
    let Some(part) = app.session.doc.comments.get(&id).map(|c| c.part) else { return };
    let draft = drafts(app).edits.get(&id).cloned();
    let mut text = draft.unwrap_or_else(|| app.session.doc.plain_text(StoryRef::Part(part)));
    // Read-only where comments can't change (a protected document), so nothing is typed in vain.
    let editable = crate::widgets::enabled(app, "review.editComment");
    let mut edit = egui::TextEdit::multiline(&mut text)
        .id(field_id(app, ui, "comment_edit", id))
        .interactive(editable)
        .desired_rows(1)
        .desired_width(f32::INFINITY)
        .hint_text(tl!("Add a comment"));
    if let Some(f) = font {
        edit = edit.font(f);
    }
    let r = ui.add(edit);
    if focus {
        r.request_focus();
    }
    if r.changed() {
        drafts(app).edits.insert(id, text);
    }
    if r.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            // Escape throws the draft away: the field shows the comment's text again.
            drafts(app).edits.remove(&id);
        } else {
            // A refusal is on the status line, and the draft stays in the field.
            let _ = commit_comment(app, id);
        }
    }
}

/// A comment field's widget id, per document: egui's own state for the field (its cursor and
/// undo history) never carries over to another document's comment with the same id.
fn field_id(app: &WordApp, ui: &Ui, kind: &str, id: u32) -> egui::Id {
    ui.scope_id().with((kind, app.session.document_id(), id))
}

/// Write a comment's draft text back, when it has one that differs from the comment. The draft
/// is dropped once written (or when there is nothing to write); a refused edit keeps it.
pub(crate) fn commit_comment(app: &mut WordApp, id: u32) -> Result<(), String> {
    let Some(text) = drafts(app).edits.get(&id).cloned() else { return Ok(()) };
    if let Some(part) = app.session.doc.comments.get(&id).map(|c| c.part)
        && text != app.session.doc.plain_text(StoryRef::Part(part))
    {
        app.run("review.editComment", json!({"id": id, "text": text}))?;
    }
    drafts(app).edits.remove(&id);
    Ok(())
}

/// The comment a reply to `id` answers: replies to a reply join its thread.
fn thread_root(app: &WordApp, id: u32) -> u32 {
    match app.session.doc.comments.get(&id).and_then(|c| c.parent) {
        Some(p) if app.session.doc.comments.contains_key(&p) => p,
        _ => id,
    }
}

/// A field for replying to a comment. When it loses focus a non-empty reply is posted with
/// `review.reply`; returns true when the field is done (posted or left empty). A refused reply
/// keeps the field open with its text.
pub(crate) fn reply_editor(app: &mut WordApp, ui: &mut Ui, id: u32, font: Option<egui::FontId>, focus: bool) -> bool {
    let mut text = drafts(app).replies.get(&id).cloned().unwrap_or_default();
    let editable = crate::widgets::enabled(app, "review.reply");
    let mut edit = egui::TextEdit::multiline(&mut text)
        .id(field_id(app, ui, "comment_reply", id))
        .interactive(editable)
        .desired_rows(1)
        .desired_width(f32::INFINITY)
        .hint_text(tl!("Write a reply"));
    if let Some(f) = font {
        edit = edit.font(f);
    }
    let r = ui.add(edit);
    if focus {
        r.request_focus();
    }
    if r.changed() {
        drafts(app).replies.insert(id, text);
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        // Escape throws the reply away and closes the field.
        drafts(app).replies.remove(&id);
        return true;
    }
    r.lost_focus() && commit_reply(app, id).is_ok()
}

/// Post a pending reply draft to comment `id` (nothing when there is none or it is blank). The
/// draft is dropped once posted; a refused reply keeps it.
pub(crate) fn commit_reply(app: &mut WordApp, id: u32) -> Result<(), String> {
    let Some(text) = drafts(app).replies.get(&id).cloned() else { return Ok(()) };
    if !text.trim().is_empty() && app.session.doc.comments.contains_key(&id) {
        let root = thread_root(app, id);
        app.run("review.reply", json!({"id": root, "text": text.trim_end()}))?;
    }
    drafts(app).replies.remove(&id);
    Ok(())
}

/// Write every pending comment and reply draft to the document, so a save has them and a close,
/// New or Open sees them as unsaved changes (#1348). Every draft is tried; the first refusal is
/// returned (and is on the status line), and refused drafts stay.
pub(crate) fn flush_comment_drafts(app: &mut WordApp) -> Result<(), String> {
    let (edits, replies): (Vec<u32>, Vec<u32>) = {
        let d = drafts(app);
        (d.edits.keys().copied().collect(), d.replies.keys().copied().collect())
    };
    let mut first = Ok(());
    for id in edits {
        let r = commit_comment(app, id);
        if first.is_ok() {
            first = r;
        }
    }
    for id in replies {
        let r = commit_reply(app, id);
        if first.is_ok() {
            first = r;
        }
    }
    first
}

/// Style Inspector: the paragraph and character levels at the caret, each with its style and the
/// direct formatting on top, and a button to clear each level.
fn inspector(app: &mut WordApp, ui: &mut Ui) {
    // Read-only, so call it directly: running the command every frame would pay for the command
    // machinery and consume per-command state (a pending `join_next_undo`) just to draw the pane.
    let r = wordcraft_engine::cmd::inspector::inspect(&app.session);
    let t = Tokens::get(ui.ctx());
    let none = tl!("None").to_string();
    let para = &r["paragraph"];
    let chr = &r["character"];
    let pstyle = para["styleName"].as_str().unwrap_or("Normal").to_string();
    let cstyle = chr["styleName"].as_str().map(str::to_string);
    // Built-in style names show in the interface language; the document keeps the English ones.
    let shown = |id: &Value, name: &str| {
        id.as_str().and_then(|id| app.session.doc.styles.get(id)).map(crate::i18n::style_name).unwrap_or_else(|| name.to_string())
    };
    let pstyle_shown = shown(&para["style"], &pstyle);
    let cstyle_shown = cstyle.as_deref().map(|n| shown(&chr["style"], n));
    let pdirect = direct_list(&para["direct"]);
    let cdirect = direct_list(&chr["direct"]);
    let mut clear: Option<&str> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        section(ui, &t, "Paragraph", |ui| {
            if level_row(ui, &t, "Style", &pstyle_shown, false, "Reset to Normal", pstyle == "Normal" && para["style"] == "Normal") {
                clear = Some("paragraphStyle");
            }
            let (txt, empty) = if pdirect.is_empty() { (none.clone(), true) } else { (pdirect.join(", "), false) };
            if level_row(ui, &t, "Direct formatting", &txt, empty, "Clear Paragraph Formatting", empty) {
                clear = Some("paragraphFormatting");
            }
        });
        ui.add_space(8.0);
        section(ui, &t, "Characters", |ui| {
            let (txt, empty) = match &cstyle_shown {
                Some(n) => (n.clone(), false),
                None => (none.clone(), true),
            };
            if level_row(ui, &t, "Style", &txt, empty, "Clear Character Style", empty) {
                clear = Some("characterStyle");
            }
            let (txt, empty) = if cdirect.is_empty() { (none.clone(), true) } else { (cdirect.join(", "), false) };
            if level_row(ui, &t, "Direct formatting", &txt, empty, "Clear Character Formatting", empty) {
                clear = Some("characterFormatting");
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(tl!("Clear All")).on_hover_text(tl!("Clear All Formatting")).clicked() {
                let _ = app.run("format.clear", json!({}));
                app.canvas.want_focus = true;
            }
            if ui.button(tl!("Styles Pane")).clicked() {
                let _ = app.run("view.stylesPane", json!({"value": true}));
            }
        });
    });
    if let Some(level) = clear {
        let _ = app.run("styles.inspectorClear", json!({"level": level}));
        app.canvas.want_focus = true;
    }
}

/// A titled, bordered group in the inspector.
fn section(ui: &mut Ui, t: &Tokens, title: &str, body: impl FnOnce(&mut Ui)) {
    ui.label(egui::RichText::new(tl!(title)).font(semibold(12.5)).color(t.text));
    ui.add_space(2.0);
    egui::Frame::NONE.fill(t.input).stroke(Stroke::new(1.0, t.border)).corner_radius(4).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        body(ui);
    });
}

/// One level: a caption, its value, and a clear button (disabled when there's nothing to clear).
/// Returns true when the button was clicked.
fn level_row(ui: &mut Ui, t: &Tokens, caption: &str, value: &str, dim: bool, tip: &str, disabled: bool) -> bool {
    ui.label(egui::RichText::new(tl!(caption)).font(regular(11.0)).color(t.text_dim));
    let mut clicked = false;
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 26.0).max(40.0);
        ui.allocate_ui(vec2(w, 18.0), |ui| {
            ui.set_width(w);
            let text = egui::RichText::new(value).font(regular(12.5)).color(if dim { t.text_disabled } else { t.text });
            ui.add(egui::Label::new(text).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let (r, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), egui::Sense::click());
            let enabled = !disabled;
            if enabled && resp.hovered() {
                ui.painter().rect_filled(r, 3.0, t.hover);
            }
            let c = if enabled { t.icon } else { t.text_disabled };
            crate::icons::paint(
                ui.painter(),
                egui::Rect::from_center_size(r.center(), vec2(16.0, 16.0)),
                "clear",
                c,
                if enabled { t.accent } else { c },
            );
            let resp = resp.on_hover_text(tl!(tip));
            clicked = enabled && resp.clicked();
        });
    });
    ui.add_space(4.0);
    clicked
}

/// Readable descriptions of the inspector's `direct` entries (`[{"prop", "value"}]`).
fn direct_list(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().take(64).map(|d| describe(d["prop"].as_str().unwrap_or(""), &d["value"])).collect()).unwrap_or_default()
}

/// Each `{"prop", "value"}` of a list (the Style Inspector's, or a formatting change's) as text.
pub(crate) fn describe_props(list: &[Value]) -> Vec<String> {
    list.iter().take(64).map(|d| describe(d["prop"].as_str().unwrap_or(""), &d["value"])).collect()
}

/// "Bold", "Not Italic", "Font: Arial", "Size: 14 pt"…
fn describe(prop: &str, v: &Value) -> String {
    let label = match prop {
        "font" => "Font",
        "size" => "Size",
        "bold" => "Bold",
        "italic" => "Italic",
        "underline" => "Underline",
        "underlineColor" => "Underline color",
        "strike" => "Strikethrough",
        "doubleStrike" => "Double strikethrough",
        "color" => "Font color",
        "highlight" => "Highlight",
        "shading" => "Shading",
        "vertAlign" => "Position",
        "caps" => "All caps",
        "smallCaps" => "Small caps",
        "hidden" => "Hidden",
        "spacing" => "Character spacing",
        "scale" => "Scale",
        "position" => "Raised/lowered",
        "outline" => "Outline",
        "shadow" => "Shadow",
        "emboss" => "Emboss",
        "engrave" => "Engrave",
        "align" => "Alignment",
        "indentLeft" => "Left indent",
        "indentRight" => "Right indent",
        "indentFirst" => "First line",
        "spaceBefore" => "Space before",
        "spaceAfter" => "Space after",
        "lineSpacing" => "Line spacing",
        "contextualSpacing" => "Don't add space between paragraphs of the same style",
        "mirrorIndents" => "Mirror indents",
        "keepNext" => "Keep with next",
        "keepLines" => "Keep lines together",
        "pageBreakBefore" => "Page break before",
        "widowControl" => "Widow/Orphan control",
        "outlineLevel" => "Outline level",
        "tabs" => "Tabs",
        "borders" => "Borders",
        "suppressHyphens" => "Don't hyphenate",
        "suppressLineNumbers" => "Suppress line numbers",
        "bidi" => "Right-to-left",
        "dropCap" => "Drop cap",
        "dropCapMargin" => "In margin",
        "dropCapDistance" => "Distance from text",
        "dropCapFont" => "Font",
        "style" => "Style",
        "numbering" => "Numbering",
        "lang" => "Language",
        other => other,
    };
    let label = tl!(label);
    let pt = |x: f64| crate::i18n::fmt(tl!("{n} pt"), &[("n", &trim_num(x))]);
    let hex = |c: &Value| -> Option<String> {
        let a = c.as_array()?;
        let ch = |i: usize| a.get(i).and_then(Value::as_u64).unwrap_or(0).min(255);
        Some(format!("#{:02X}{:02X}{:02X}", ch(0), ch(1), ch(2)))
    };
    let value = match (prop, v) {
        (_, Value::Bool(true)) => return label.to_string(),
        (_, Value::Bool(false)) => return crate::i18n::fmt(tl!("Not {name}"), &[("name", label)]),
        ("scale", Value::Number(n)) => format!("{}%", trim_num(n.as_f64().unwrap_or(100.0))),
        ("outlineLevel", Value::Number(n)) => match n.as_u64().unwrap_or(9) {
            l @ 0..=8 => crate::i18n::fmt(tl!("Level {n}"), &[("n", &(l + 1).to_string())]),
            _ => tl!("Body Text").to_string(),
        },
        ("dropCapFont", Value::String(s)) => s.clone(),
        ("dropCap", Value::Number(n)) => crate::i18n::fmt(tl!("{n} lines"), &[("n", &n.to_string())]),
        (_, Value::Number(n)) => pt(n.as_f64().unwrap_or(0.0)),
        ("align", Value::String(s)) => tl!(match s.as_str() {
            "center" => "Centered",
            "right" => "Right",
            "justify" => "Justified",
            "distribute" => "Distributed",
            _ => "Left",
        })
        .to_string(),
        ("color", Value::String(_)) => tl!("Automatic").to_string(),
        ("color", Value::Object(o)) => o.get("Rgb").and_then(hex).unwrap_or_default(),
        (_, Value::Array(_)) if prop != "tabs" => hex(v).unwrap_or_default(),
        ("lineSpacing", Value::Object(o)) => {
            let x = o.get("value").and_then(Value::as_f64).unwrap_or(1.0);
            match o.get("rule").and_then(Value::as_str) {
                Some("atLeast") => crate::i18n::fmt(tl!("At least {n}"), &[("n", &pt(x))]),
                Some("exactly") => crate::i18n::fmt(tl!("Exactly {n}"), &[("n", &pt(x))]),
                _ => crate::i18n::fmt(tl!("{n} lines"), &[("n", &trim_num(x))]),
            }
        }
        (_, Value::String(s)) => tl!(&camel_words(s)).to_string(),
        _ => return label.to_string(),
    };
    format!("{label}: {value}")
}

/// `1.50` → `1.5`, `12.0` → `12`.
fn trim_num(x: f64) -> String {
    let s = format!("{:.2}", if x.is_finite() { x } else { 0.0 });
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `dotDash` → `Dot dash` (enum values from the document model).
fn camel_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().take(64).enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Content, Pane};
    use craft_ui::panes::PaneContent;
    use egui::vec2;

    /// #1902: Clipboard Delete returns focus to the document like paste does.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn clipboard_delete_returns_focus_to_the_document() {
        use egui_kittest::kittest::Queryable;

        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Hello")), crate::Services::default());
        app.run("select.all", serde_json::json!({})).unwrap();
        app.run("edit.copy", serde_json::json!({})).unwrap();
        assert_eq!(app.session.clip_history.items().len(), 1);
        app.run("edit.clipboardPane", serde_json::json!({"value": true})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut crate::WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        h.run_steps(4);
        // Focus the button for real: a bare accesskit click leaves egui focus empty, and an
        // unrelated chrome-click helper would hand focus to the canvas on its own.
        h.get_by_label("Delete").focus();
        h.run_steps(2);
        h.state_mut().canvas.want_focus = false;
        h.get_by_label("Delete").click();
        h.run_steps(4);
        assert!(h.state().canvas.focused, "Delete returns focus to the document");
        assert!(h.state().session.clip_history.items().is_empty(), "the item was deleted");
    }

    /// F6 past the last pane gives the document the keyboard, and typing reaches it (#1756).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn f6_past_the_last_pane_types_into_the_document() {
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        app.run("view.navigationPane", serde_json::json!({"value": true})).unwrap();
        app.run("edit.clipboardPane", serde_json::json!({"value": true})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut crate::WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        h.run_steps(4);
        assert!(h.state().canvas.focused);
        let f6 = egui::Event::Key { key: egui::Key::F6, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        for pane in ["Navigation", "Clipboard"] {
            h.input_mut().events.push(f6.clone());
            h.run_steps(2);
            assert!(!h.state().canvas.focused, "F6 moved the keyboard to the {pane} pane");
        }
        h.input_mut().events.push(f6);
        h.run_steps(2);
        assert!(h.state().canvas.focused, "F6 past the last pane returns to the document");
        h.input_mut().events.push(egui::Event::Text("typed".into()));
        h.run_steps(2);
        assert_eq!(h.state().session.doc.plain_text(wordcraft_doc::StoryRef::Body).trim(), "typed");
    }

    fn pane_harness(app: crate::WordApp) -> egui_kittest::Harness<'static, crate::WordApp> {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut crate::WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        h.run_steps(4);
        h
    }

    fn press(h: &mut egui_kittest::Harness<'static, crate::WordApp>, key: egui::Key, modifiers: egui::Modifiers) {
        h.input_mut().events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
        h.run_steps(2);
    }

    /// F6 and Shift+F6 with a dialog open leave the keyboard in the dialog: no pane title bar and
    /// no document behind it takes it.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn f6_in_an_open_dialog_keeps_focus_there() {
        for panes in [true, false] {
            let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
            app.run("view.navigationPane", serde_json::json!({"value": panes})).unwrap();
            app.run("ui.dialog", serde_json::json!({"name": "replace"})).unwrap();
            let mut h = pane_harness(app);
            let focused = h.ctx.memory(|m| m.focused());
            for shift in [false, true] {
                press(&mut h, egui::Key::F6, if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE });
                assert_eq!(h.ctx.memory(|m| m.focused()), focused, "panes open: {panes}, shift: {shift}");
                assert!(!h.state().canvas.focused && h.state().dialog.is_some(), "panes open: {panes}, shift: {shift}");
            }
        }
    }

    /// Modelo 2: every ancestor level's trunk is lit from its parent down to the selected row,
    /// including the siblings passed on the way (the CSS `.is-active-branch` path).
    #[test]
    fn branch_guides_light_the_way_from_the_root_to_the_selection() {
        let levels = [0u8, 1, 1, 2, 2, 1];
        let chain = super::active_chain(&levels, &[true; 6], Some(4));
        assert_eq!(chain, vec![0, 2, 4]);
        let g = |ancestors: &[u8], i: usize| super::branch_guide_levels(ancestors, &chain, &levels, i, Some(4));
        assert_eq!(g(&[0], 1), vec![0], "a sibling above the branch lights the root trunk");
        assert_eq!(g(&[0], 2), vec![0]);
        assert_eq!(g(&[0, 1], 3), vec![0, 1]);
        assert_eq!(g(&[0, 1], 4), vec![0, 1], "the selected row closes the path");
    }

    /// Modelo 2: the caret's branch is the chain from the root down to the selected heading, so
    /// the trunks of every ancestor level are lit through the siblings passed on the way.
    #[test]
    fn active_chain_runs_from_the_root_to_the_selected_heading() {
        let levels = [1u8, 2, 2, 3, 1];
        let visible = [true; 5];
        assert_eq!(super::active_chain(&levels, &visible, Some(3)), vec![0, 2, 3]);
        assert_eq!(super::active_chain(&levels, &visible, Some(0)), vec![0]);
        assert!(super::active_chain(&levels, &visible, None).is_empty());
        let hidden = [true, false, false, false, true];
        assert!(super::active_chain(&levels, &hidden, Some(1)).is_empty(), "a hidden heading lights nothing");
    }

    /// Ctrl and F-key shortcuts run while F6 has put the keyboard on a pane's title bar; a pane's
    /// text field keeps Ctrl+H for itself.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn shortcuts_run_while_a_pane_title_bar_has_focus() {
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        app.run("view.navigationPane", serde_json::json!({"value": true})).unwrap();
        let mut h = pane_harness(app);
        press(&mut h, egui::Key::F6, egui::Modifiers::NONE);
        assert!(!h.state().canvas.focused && h.ctx.memory(|m| m.focused()).is_some(), "the title bar has the keyboard");
        press(&mut h, egui::Key::H, egui::Modifiers::COMMAND);
        assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("replace"), "Ctrl+H opened Replace");
        press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(h.state().dialog.is_none());

        h.state_mut().nav_search_focus = true;
        h.run_steps(2);
        assert!(h.ctx.text_edit_focused());
        press(&mut h, egui::Key::H, egui::Modifiers::COMMAND);
        assert!(h.state().dialog.is_none(), "the search field keeps Ctrl+H");
    }

    /// A dialog button with the keyboard keeps Ctrl+Z away from the document behind the dialog.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn shortcuts_stay_out_of_the_document_while_a_dialog_button_has_focus() {
        use egui_kittest::kittest::Queryable;

        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        app.run("text.insert", serde_json::json!({"text": "kept"})).unwrap();
        app.run("ui.dialog", serde_json::json!({"name": "wordCount"})).unwrap();
        let mut h = pane_harness(app);
        // Two buttons are labelled Close (the dialog's title bar and its own row); either will do.
        if let Some(close) = h.query_all_by_label("Close").next() {
            close.focus();
        }
        h.run_steps(2);
        assert!(!h.state().canvas.focused, "the document gave up the keyboard");
        assert!(!h.ctx.text_edit_focused() && h.ctx.memory(|m| m.focused()).is_some(), "Close has the keyboard");
        press(&mut h, egui::Key::Z, egui::Modifiers::COMMAND);
        assert_eq!(h.state().session.doc.plain_text(wordcraft_doc::StoryRef::Body).trim(), "kept", "Ctrl+Z left the document alone");
        assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("wordCount"));
    }

    /// Escape during a pane's title-bar drag cancels the drag and leaves full screen on.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn escape_during_a_pane_drag_keeps_full_screen() {
        use egui_kittest::kittest::Queryable;

        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        app.run("view.navigationPane", serde_json::json!({"value": true})).unwrap();
        app.run("view.fullScreen", serde_json::json!({"value": true})).unwrap();
        let mut h = pane_harness(app);
        let title = h.get_by_label_contains("Move Pane: ").rect().center();
        let button =
            |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        h.input_mut().events.extend([egui::Event::PointerMoved(title), button(title, true)]);
        h.run_steps(2);
        let away = title + egui::vec2(300.0, 200.0);
        h.input_mut().events.push(egui::Event::PointerMoved(away));
        h.run_steps(2);
        assert!(h.ctx.dragged_id().is_some(), "the title bar is dragged");
        press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(h.state().session.view.full_screen);
        h.input_mut().events.push(button(away, false));
        h.run_steps(2);
        assert!(h.state().session.view.full_screen);
        let docked = craft_ui::panes::Place::Docked(craft_ui::panes::Edge::Start);
        assert_eq!(super::pane_state(h.state(), Pane::Navigation).place, docked, "the drag was cancelled");
    }

    /// With Navigation docked left, Comments dropped over Navigation's outer third docks left of
    /// it, where the preview showed, and the order survives a restart; over Navigation's inner
    /// third it goes back inside.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn comments_dropped_over_navigations_outer_third_docks_left_of_it() {
        use egui_kittest::kittest::Queryable;

        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        app.run("view.navigationPane", serde_json::json!({"value": true})).unwrap();
        app.run("view.commentsPane", serde_json::json!({"value": true})).unwrap();
        let mut h = pane_harness(app);
        // A docked pane's title bar starts at its panel's left edge.
        let panel = |h: &egui_kittest::Harness<'static, crate::WordApp>, pane: Pane| h.get_by_label(&format!("Move Pane: {}", pane.title())).rect();
        let button =
            |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        let drag = |h: &mut egui_kittest::Harness<'static, crate::WordApp>, to: egui::Pos2| {
            let from = h.get_by_label("Move Pane: Comments").rect().center();
            h.input_mut().events.extend([egui::Event::PointerMoved(from), button(from, true)]);
            h.run_steps(1);
            h.input_mut().events.push(egui::Event::PointerMoved(from + egui::vec2(-8.0, 6.0)));
            h.run_steps(1);
            h.input_mut().events.push(egui::Event::PointerMoved(to));
            h.run_steps(1);
            h.input_mut().events.push(button(to, false));
            h.run_steps(3);
        };
        let nav = panel(&h, Pane::Navigation);
        drag(&mut h, egui::pos2(nav.left() + 60.0, nav.center().y));
        let (comments, nav_after) = (panel(&h, Pane::Comments), panel(&h, Pane::Navigation));
        assert_eq!(comments.left(), nav.left(), "Comments is outermost");
        assert!(nav_after.left() > comments.right(), "Navigation sits inside it");
        assert_eq!(super::open(h.state()), vec![Pane::Comments, Pane::Navigation]);

        let saved = serde_json::to_vec(&h.state().prefs()).unwrap();
        let mut next = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        next.apply_prefs(serde_json::from_slice(&saved).unwrap());
        next.session.view.comments_pane = true;
        assert_eq!(super::open(&next), vec![Pane::Comments, Pane::Navigation], "the order is kept");

        let nav = panel(&h, Pane::Navigation);
        let width = Pane::Navigation.default_state().size.unwrap();
        drag(&mut h, egui::pos2(nav.left() + width * 0.85, nav.center().y));
        assert_eq!(super::open(h.state()), vec![Pane::Navigation, Pane::Comments]);
        assert!(panel(&h, Pane::Comments).left() > panel(&h, Pane::Navigation).right(), "back inside Navigation");

        let mut junk = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        junk.apply_prefs(serde_json::from_str(r#"{"paneOrder": ["comments", 3, "nope", "comments", "navigation"], "navPane": true}"#).unwrap());
        assert_eq!(junk.ui.pane_order, vec!["comments", "navigation"], "unknown and repeated keys are dropped");
        for bad in [serde_json::json!({"pane": "comments", "before": "comments"}), serde_json::json!({"pane": "comments", "before": 7})] {
            assert!(junk.run("ui.paneDock", bad.clone()).is_err(), "{bad}");
        }
        junk.run("ui.paneDock", serde_json::json!({"pane": "navigation", "before": null})).unwrap();
        assert_eq!(junk.ui.pane_order.last().map(String::as_str), Some("navigation"));
    }

    /// An F6 the user bound runs its command; plain F6 still moves to the panes when only
    /// Shift+F6 is bound.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn a_user_bound_f6_runs_its_command() {
        for key in ["F6", "Shift+F6"] {
            let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
            app.run("view.navigationPane", serde_json::json!({"value": true})).unwrap();
            app.session.keymap = wordcraft_engine::keymap::KeyMap::from_value(&serde_json::json!({"assigned": {key: "view.focus"}}));
            let mut h = pane_harness(app);
            assert!(h.state().canvas.focused);
            let m = if key == "F6" { egui::Modifiers::NONE } else { egui::Modifiers::SHIFT };
            press(&mut h, egui::Key::F6, m);
            assert!(h.state().session.view.focus_mode, "{key} ran its command");
            press(&mut h, egui::Key::F6, m);
            assert!(!h.state().session.view.focus_mode, "{key} ran its command again");
            if key == "Shift+F6" {
                h.state_mut().canvas.want_focus = true;
                h.run_steps(2);
                press(&mut h, egui::Key::F6, egui::Modifiers::NONE);
                assert!(!h.state().canvas.focused, "plain F6 moved the keyboard to the pane");
            }
        }
    }

    /// Every pane's body fits a pane at the 200 pt minimum width (the host clips what doesn't):
    /// button rows wrap and fields narrow. Restrict Editing shows its style list and password
    /// fields.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn pane_bodies_fit_the_narrowest_pane() {
        // 200 pt less the 10 pt body margins and the frame's 1 pt strokes.
        const BODY: f32 = 178.0;
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Hello")), crate::Services::default());
        app.run("select.all", serde_json::json!({})).unwrap();
        app.run("edit.copy", serde_json::json!({})).unwrap();
        app.restrict = crate::dialogs_review::RestrictPane {
            open: true,
            formatting: true,
            show_styles: true,
            editing: true,
            asking: true,
            ..Default::default()
        };
        let panes = [
            Pane::Navigation,
            Pane::Clipboard,
            Pane::Styles,
            Pane::StyleInspector,
            Pane::Accessibility,
            Pane::AltText,
            Pane::FormatShape,
            Pane::RestrictEditing,
            Pane::Comments,
        ];
        let widths = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = widths.clone();
        let mut fonts = false;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(400.0, 1200.0)).build_ui_state(
            move |ui, app: &mut crate::WordApp| {
                // Fonts take effect the frame after they are installed.
                if !fonts {
                    crate::theme::install_fonts(ui.ctx());
                    fonts = true;
                    return;
                }
                let mut row = Vec::new();
                for pane in panes {
                    let width = ui
                        .allocate_ui_with_layout(vec2(BODY, 1200.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_max_width(BODY);
                            Content { app: &mut *app }.body(ui, &pane);
                            ui.min_rect().width()
                        })
                        .inner;
                    row.push((pane, width));
                }
                *seen.borrow_mut() = row;
            },
            app,
        );
        h.run_steps(3);
        let widths = widths.borrow();
        assert_eq!(widths.len(), panes.len());
        let wide: Vec<_> = widths.iter().filter(|(_, width)| *width > BODY + 0.5).collect();
        assert!(wide.is_empty(), "wider than the pane: {wide:?}");
    }

    /// A pane other than Styles floated from its menu comes back floating, at the same rect, after
    /// a restart; junk in `panePlaces` reads as the defaults (#1756).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn a_floated_pane_floats_again_after_a_restart() {
        use craft_ui::panes::Place;
        use egui_kittest::kittest::Queryable;
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        app.run("view.navigationPane", serde_json::json!({"value": true})).unwrap();
        let mut h = pane_harness(app);
        let bar = h.get_by_label_contains("Move Pane: ").rect();
        let on_row = |r: egui::Rect| r.center().y > bar.top() && r.center().y < bar.bottom();
        h.get_all_by_label("Pane Options").find(|n| on_row(n.rect())).expect("the menu button beside the title bar").click();
        h.run_steps(2);
        h.get_by_label("Float").click();
        h.run_steps(3);
        assert_eq!(super::pane_state(h.state(), Pane::Navigation).place, Place::Floating);
        let rect = h.state().ui.pane_places.get("navigation").and_then(|p| p.float).expect("the float rect is kept");

        let saved = serde_json::to_vec(&h.state().prefs()).unwrap();
        let mut next = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        next.apply_prefs(serde_json::from_slice(&saved).unwrap());
        let state = super::pane_state(&next, Pane::Navigation);
        assert_eq!((state.place, state.float_rect), (Place::Floating, Some(rect)));
        assert_eq!(super::pane_state(&next, Pane::Clipboard), Pane::Clipboard.default_state(), "other panes keep their defaults");

        let mut junk = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        junk.apply_prefs(
            serde_json::from_str(
                r#"{"panePlaces": {"navigation": {"dock": "top", "float": [1, "x", 3, 4]}, "clipboard": 7, "nope": {}}, "navPane": true}"#,
            )
            .unwrap(),
        );
        assert_eq!(super::pane_state(&junk, Pane::Navigation), Pane::Navigation.default_state(), "junk reads as the default");
        assert!(junk.session.view.nav_pane, "the rest of the file still loads");
        let mut odd = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        odd.apply_prefs(serde_json::from_str(r#"{"panePlaces": [1, 2], "navPane": true}"#).unwrap());
        assert!(odd.ui.pane_places.is_empty() && odd.session.view.nav_pane);
    }

    /// `ui.paneDock` docks or floats any pane and reads its place, as the pane's menu does
    /// (#1756); Styles goes through `view.stylesPaneDock`; junk is refused and changes nothing.
    #[test]
    fn pane_dock_command_docks_floats_and_reads_each_pane() {
        use craft_ui::panes::{Edge, Place};
        use serde_json::json;
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("")), crate::Services::default());
        assert_eq!(app.run("ui.paneDock", json!({"pane": "navigation"})).unwrap(), json!({"pane": "navigation", "dock": "left"}));
        assert_eq!(app.run("ui.paneDock", json!({"pane": "comments"})).unwrap(), json!({"pane": "comments", "dock": "right"}));
        assert_eq!(app.run("ui.paneDock", json!({"pane": "navigation", "dock": "float"})).unwrap()["dock"], "float");
        assert_eq!(super::pane_state(&app, Pane::Navigation).place, Place::Floating);
        app.run("ui.paneDock", json!({"pane": "comments", "dock": "left"})).unwrap();
        assert_eq!(super::pane_state(&app, Pane::Comments).place, Place::Docked(Edge::Start));
        assert_eq!(app.run("ui.paneDock", json!({"pane": "styles", "dock": "float"})).unwrap(), json!({"pane": "styles", "dock": "float"}));
        assert_eq!(app.session.view.styles_pane_dock, wordcraft_engine::StylesPaneDock::Floating);
        assert!(!app.ui.pane_places.contains_key("styles"), "Styles keeps its place in the view state");
        for junk in [
            json!({}),
            json!({"pane": "nope", "dock": "left"}),
            json!({"pane": 3}),
            json!({"pane": "navigation", "dock": "top"}),
            json!({"pane": "navigation", "dock": 1}),
            json!({"pane": "navigation", "dock": null}),
        ] {
            assert!(app.run("ui.paneDock", junk.clone()).is_err(), "{junk}");
        }
        assert_eq!(super::pane_state(&app, Pane::Navigation).place, Place::Floating, "junk changed nothing");
    }

    /// #3032: Delete is greyed out for built-in styles in the Styles pane (the engine refuses
    /// them — `crates/engine/src/cmd/para.rs`), while custom styles keep it enabled.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn built_in_styles_grey_out_delete_in_the_styles_pane() {
        use egui_kittest::kittest::{NodeT, Queryable};

        /// Draw only the pane, so each style name labels exactly one node — the ribbon's Styles
        /// gallery shows the same names — right-click the row, and report whether its Delete is
        /// greyed out.
        fn delete_is_disabled(style: &str) -> bool {
            let mut app =
                crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Hello")), crate::Services::default());
            app.run("styles.create", serde_json::json!({"name": "Callout"})).unwrap();
            // The pane writes in the "semibold" family, which the app installs with the rest of the
            // interface fonts (`chrome::title_bar`). Fonts take effect the frame after they are set,
            // so the first frame only installs them and the pane is drawn from the second on.
            let mut fonts = false;
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(400.0, 1200.0)).build_ui_state(
                |ui, app: &mut crate::WordApp| {
                    if !fonts {
                        crate::theme::install_fonts(ui.ctx());
                        fonts = true;
                        return;
                    }
                    super::styles(app, ui);
                },
                app,
            );
            h.run_steps(4);
            h.get_by_label(style).click_secondary();
            h.run_steps(3);
            h.get_by_label("Delete").accesskit_node().is_disabled()
        }
        assert!(delete_is_disabled("Normal"), "the built-in style's Delete is greyed out");
        assert!(!delete_is_disabled("Callout"), "the custom style's Delete stays enabled");
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    /// Modelo 2: the elbow leaves the trunk going down (`mid - r`), turns through a quadratic
    /// control on the row's middle line and arrives horizontally from the left, never as `╭`.
    #[test]
    fn the_curve_starts_above_the_row_and_turns_right() {
        let row = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, NAV_ROW_HEIGHT));
        let g = nav_row_geometry(row, 2, &[0, 1], false, true, &[0, 1], NavTreeStyle::ZedLinear, None, false);
        let curve = g.curves.first().expect("an active row has its elbow");
        let mid = row.center().y;
        assert!((curve.start.y - (mid - nav_curve_radius(1))).abs() < 0.01, "starts above the middle");
        assert_eq!(curve.start.x, row.left() - NAV_PANE_TRIM + nav_node_x(1), "starts on the trunk");
        assert!(curve.control.y > curve.start.y && curve.control.y == mid, "the quadratic control is on the middle line");
        assert!(curve.end.x > curve.start.x, "turns right");
        assert_eq!(curve.end.y, mid);
        assert_eq!(curve.horizontal_end, g.content_x - 3.0, "a leaf's elbow stops before its badge");
    }

    /// The active curve ends exactly on the selection pill's left border.
    #[test]
    fn the_curve_meets_the_selection_pill() {
        let row = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, NAV_ROW_HEIGHT));
        let g = nav_row_geometry(row, 2, &[0, 1], false, true, &[0, 1], NavTreeStyle::ZedLinear, None, true);
        let pill = g.pill.expect("the selected row has a pill");
        let curve = g.curves.first().expect("the selected row has its elbow");
        assert!((pill.left() - curve.horizontal_end).abs() < 0.01, "{} vs {}", pill.left(), curve.horizontal_end);
        assert!((pill.left() - (g.content_x - NAV_PILL_INSET)).abs() < 0.01);
        assert!((22.0..=24.0).contains(&pill.height()), "pill height {}", pill.height());
    }

    /// Trunks of consecutive rows meet without a gap: the row above ends where the next begins.
    #[test]
    fn consecutive_rows_have_no_gap_in_the_trunk() {
        let a = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, NAV_ROW_HEIGHT));
        let b = a.translate(vec2(0.0, NAV_ROW_HEIGHT));
        let ga = nav_row_geometry(a, 2, &[0, 1], false, false, &[], NavTreeStyle::FleetConnected, None, false);
        let gb = nav_row_geometry(b, 2, &[0, 1], false, false, &[], NavTreeStyle::FleetConnected, None, false);
        let ta = ga.trunks.iter().find(|t| (t.x - (a.left() - NAV_PANE_TRIM + nav_node_x(0))).abs() < 0.01).expect("the outer trunk");
        let tb = gb.trunks.iter().find(|t| (t.x - (b.left() - NAV_PANE_TRIM + nav_node_x(0))).abs() < 0.01).expect("the outer trunk");
        assert!(trunk_has_no_gap(ta, tb));
    }

    /// A parent's rail goes through the node: it stops at the circle's top and resumes at its
    /// base, so the chevron is never crossed by a line.
    #[test]
    fn the_trunk_enters_and_leaves_the_node_without_crossing_it() {
        let row = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, NAV_ROW_HEIGHT));
        let g = nav_row_geometry(row, 1, &[0], true, true, &[0], NavTreeStyle::ZedLinear, Some(false), false);
        let node = g.node.expect("a parent has a node");
        assert!(g.trunks.iter().any(|t| (t.x - node.centre.x).abs() < 0.01 && t.bottom + 0.01 >= node.centre.y - node.radius));
        assert!(g.trunks.iter().any(|t| (t.x - node.centre.x).abs() < 0.01 && t.top <= node.centre.y + node.radius + 0.01));
        assert_eq!(node.radius, nav_node_radius(1));
        assert!(node.expanded);
    }

    /// Option A leaves inactive leaves without an elbow; Option B connects every child. Both keep
    /// the same trunk, node and pill geometry.
    #[test]
    fn option_a_has_no_inactive_elbows_and_option_b_connects_every_child() {
        let row = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, NAV_ROW_HEIGHT));
        let a = nav_row_geometry(row, 2, &[0, 1], false, false, &[], NavTreeStyle::ZedLinear, None, false);
        assert!(a.curves.is_empty(), "A: an inactive leaf has no comb teeth");
        let b = nav_row_geometry(row, 2, &[0, 1], false, false, &[], NavTreeStyle::FleetConnected, None, false);
        let curve = b.curves.first().expect("B: every child is connected");
        assert!(!curve.active, "an inactive child's elbow is grey");
        assert_eq!(b.pill, None);
    }

    /// B: a nested trunk ends on the last child's curve, while the outer (root) rail continues to
    /// the next root row.
    #[test]
    fn option_b_ends_the_nested_trunk_on_the_last_curve() {
        let row = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, NAV_ROW_HEIGHT));
        let g = nav_row_geometry(row, 2, &[0, 1], true, false, &[], NavTreeStyle::FleetConnected, None, false);
        let inner = g.trunks.iter().find(|t| (t.x - (row.left() - NAV_PANE_TRIM + nav_node_x(1))).abs() < 0.01).expect("inner trunk");
        let outer = g.trunks.iter().find(|t| (t.x - (row.left() - NAV_PANE_TRIM + nav_node_x(0))).abs() < 0.01).expect("root rail");
        assert!((inner.bottom - (row.center().y - nav_curve_radius(1))).abs() < 0.01, "no loose end below the last child");
        assert!((outer.bottom - row.bottom()).abs() < 0.01, "the root rail goes on to the next root");
    }

    /// The style is persisted as `navTreeStyle`; an old file (no key) or an unknown value loads as
    /// Option A without dropping the rest of the preferences.
    #[test]
    fn the_saved_style_defaults_to_a_and_tolerates_unknown_values() {
        assert_eq!(NavTreeStyle::default(), NavTreeStyle::ZedLinear);
        assert_eq!(NavTreeStyle::from_code("fleet_connected"), Some(NavTreeStyle::FleetConnected));
        assert_eq!(NavTreeStyle::from_code("sepia"), None);
        let old: crate::UiState = serde_json::from_str(r#"{"tab":"Insert"}"#).unwrap();
        assert_eq!(old.nav_tree_style, NavTreeStyle::ZedLinear);
        let odd: crate::UiState = serde_json::from_str(r#"{"navTreeStyle":"sepia","tab":"Insert"}"#).unwrap();
        assert_eq!(odd.nav_tree_style, NavTreeStyle::ZedLinear);
        assert_eq!(odd.tab, "Insert");
    }

    /// Numbered headings are listed with the number they show, and numbered body paragraphs in
    /// the same list still count (#3418).
    #[test]
    fn headings_show_their_list_numbers() {
        use wordcraft_doc::props::NumRef;
        use wordcraft_doc::{CharProps, ListKind, Paragraph, para_block};
        let mut doc = Document::new();
        let num = doc.numbering.add_list(ListKind::Legal);
        let para = |text: &str, style: &str, level: Option<u8>| {
            let mut p = Paragraph::with_text(text, CharProps::default()).styled(style);
            p.props.numbering = level.map(|level| NumRef { num, level });
            para_block(p)
        };
        doc.body = vec![
            para("Intro", "Heading1", Some(0)),
            para("Scope", "Heading2", Some(1)),
            para("A numbered body paragraph", "Normal", Some(1)),
            para("Terms", "Heading2", Some(1)),
            para("Method", "Heading1", Some(0)),
            para("Appendix", "Heading1", None),
        ];
        let texts: Vec<String> = nav_headings(&doc).into_iter().map(|(_, _, t)| t).collect();
        assert_eq!(texts, ["1. Intro", "1.1. Scope", "1.3. Terms", "2. Method", "Appendix"]);
    }

    #[test]
    fn long_headings_elide_without_stretching_navigation() {
        // A heading that runs well past the width, and a single word wider than the pane.
        let headings = ["An extremely long heading with words that must stay inside the navigation pane ".repeat(20), "x".repeat(400)];
        // Draw one heading in a pane of `width`: the pane rect, the heading row's rect, the label's
        // line count and whether it was elided.
        let render = |width: f32, level: u8, text: &str| -> (egui::Rect, egui::Rect, usize, bool) {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0));
            let mut panel_rect = egui::Rect::NOTHING;
            let mut heading_rect = egui::Rect::NOTHING;
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                let result = egui::Panel::left("navigation_test").default_size(width).resizable(true).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| {
                            nav_heading(
                                ui,
                                egui::Id::unique(("t", level)),
                                level,
                                text,
                                true,
                                None,
                                &[],
                                true,
                                false,
                                &[],
                                super::NavTreeStyle::ZedLinear,
                            )
                            .1
                        })
                        .inner
                });
                panel_rect = result.response.rect;
                heading_rect = result.inner.rect;
            });
            let (rows, elided) = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some((t.galley.rows.len(), t.galley.elided)),
                    _ => None,
                })
                .unwrap_or((0, false));
            output.drop_without_applying_deltas();
            (panel_rect, heading_rect, rows, elided)
        };
        for (width, level) in [(260.0, 0), (180.0, 5)] {
            let (_, one_line, _, _) = render(width, level, "Kilns");
            for text in &headings {
                let (panel_rect, heading_rect, rows, elided) = render(width, level, text);
                assert!(panel_rect.width() < width + 20.0, "long heading expanded the {width}px pane to {panel_rect:?}");
                assert!(heading_rect.max.x <= panel_rect.max.x, "heading escaped pane: {heading_rect:?} / {panel_rect:?}");
                // Writer's Navigator keeps the row on one line and ends it in an ellipsis (the
                // 0.10 look); the whole heading shows on hover.
                assert_eq!(rows, 1, "long heading must stay on one line at {width}px, got {rows}");
                assert!(elided, "long heading must end in an ellipsis at {width}px");
                assert!(
                    (heading_rect.height() - one_line.height()).abs() < 0.5,
                    "an elided heading keeps one row's height at {width}px: {heading_rect:?} vs {one_line:?}"
                );
            }
        }
    }

    /// Word lists headings flush left, each level indented under its parent (#1199).
    #[test]
    fn headings_align_left_and_indent_by_level() {
        let text_left = |level: u8| {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0));
            let mut panel_left = 0.0;
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                let result = egui::Panel::left("navigation_test").default_size(260.0).show(ui, |ui| {
                    nav_heading(ui, egui::Id::unique("t2"), level, "Kilns", false, None, &[], true, false, &[], super::NavTreeStyle::ZedLinear)
                });
                panel_left = result.response.rect.min.x;
            });
            let x = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == "Kilns" => Some(t.pos.x + t.galley.rect.min.x),
                    _ => None,
                })
                .unwrap();
            output.drop_without_applying_deltas();
            x - panel_left
        };
        let (top, second) = (text_left(0), text_left(1));
        // Every row reserves the chevron gap, so parents and leaves share the column; the step
        // between levels is the 16 px column step (content 30 → 46).
        assert!(top < 56.0, "a top-level heading starts at the pane's left edge, not centred: {top}px in");
        assert!((second - top - 16.0).abs() < 0.5, "a level-2 heading starts one level column in (30 → 46): {top} / {second}");
    }

    /// Collapsing a heading hides everything nested under it, and only that (#1997).
    #[test]
    fn collapsed_heading_hides_its_descendants_only() {
        // 1 / 1.1 / 1.1.1 / 1.2 / 1.2.1 / 2 / 2.1
        let levels = [0, 1, 2, 1, 2, 0, 1];
        let none = [false; 7];
        assert_eq!(visible_headings(&levels, &none), vec![true; 7]);
        let mut c = none;
        c[3] = true; // collapse 1.2
        assert_eq!(visible_headings(&levels, &c), vec![true, true, true, true, false, true, true]);
        c[3] = false;
        c[0] = true; // collapse 1
        assert_eq!(visible_headings(&levels, &c), vec![true, false, false, false, false, true, true]);
        // A collapsed child inside a collapsed parent stays hidden and does not reopen siblings.
        c[1] = true;
        assert_eq!(visible_headings(&levels, &c), vec![true, false, false, false, false, true, true]);
    }
}
