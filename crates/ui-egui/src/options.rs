//! File › Options (#485): a list of panes on the left (General, Display, Proofing, Save,
//! Language, Accessibility, Advanced, Agents), the chosen pane's settings on the right. Only
//! settings WordCraft carries out are shown. Each pane is one function below, so a new setting
//! goes into its pane's function. Settings agents can change too go through `file.options`; the
//! rest live in [`crate::UiState`] (saved in `ui.json`).

use std::cell::Cell;

use egui::{RichText, Ui, vec2};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use wordcraft_engine::cmd::options::{MAX_WORD_CHARS, PANES};
use wordcraft_geom::Unit;

use crate::WordApp;
use crate::theme::{Tokens, semibold};

thread_local! {
    // The unit dialogs and the ribbon show lengths in: File › Options › Advanced, copied from the
    // session each frame so dialog code without the app can read it.
    static UNIT: Cell<Unit> = const { Cell::new(Unit::Inches) };
    // The region's decimal separator (#838).
    static DECIMAL: Cell<char> = const { Cell::new('.') };
}

/// The unit lengths are shown in (File › Options › Advanced › Measurement units, else the
/// region's).
pub fn unit() -> Unit {
    UNIT.get()
}

/// The decimal separator lengths are written with (the region's: `,` in Poland).
pub fn decimal() -> char {
    DECIMAL.get()
}

/// `pt` in [`unit`], written the region's way (`2,54 cm`), with the unit in the interface
/// language (`2,54 см` in Russian).
pub fn length(pt: f32) -> String {
    length_in(unit(), pt)
}

/// [`length`] in a given unit.
pub fn length_in(unit: Unit, pt: f32) -> String {
    format!("{}{}", unit.number(pt, decimal()), unit_suffix(unit))
}

/// The unit's suffix as the interface shows it: ` cm`, ` mm`, ` pt` and ` pi` go through the
/// catalog (` см` in Russian, #1734); inches keep `"`.
pub fn unit_suffix(unit: Unit) -> String {
    match unit {
        Unit::Inches => unit.suffix().to_string(),
        Unit::Centimeters => format!(" {}", tl!("cm")),
        Unit::Millimeters => format!(" {}", tl!("mm")),
        Unit::Points => format!(" {}", tl!("pt")),
        Unit::Picas => format!(" {}", tl!("pi")),
    }
}

/// A length typed into a field in `unit`, as a number of that unit: either decimal separator,
/// another unit (`1 in` in a centimetre field), and the units' names in the interface language
/// as well as in English (`3 см`).
pub fn parse_length(text: &str, unit: Unit) -> Option<f64> {
    let mut text: String = text.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    for (english, translated) in [("cm", tl!("cm")), ("mm", tl!("mm")), ("pt", tl!("pt")), ("pi", tl!("pi"))] {
        let translated = translated.to_lowercase();
        if translated != english
            && let Some(number) = text.strip_suffix(translated.as_str())
        {
            text = format!("{number}{english}");
            break;
        }
    }
    wordcraft_geom::parse_measure(&text, unit).map(|pt| f64::from(pt / unit.pt_per_unit()))
}

/// Copy the session's settings the UI reads without the app.
pub(crate) fn sync(app: &WordApp) {
    UNIT.set(app.session.units());
    DECIMAL.set(app.session.region().decimal());
}

/// A length field: the unit's suffix, and numbers written and read with the region's decimal
/// separator (`2,54 cm`). Typing either separator works, and so does another unit (`1 in` in a
/// centimetre field).
pub trait MeasureExt {
    fn measure(self, unit: Unit) -> Self;
    /// The same field showing nothing, not even the unit, until a length is typed in: a value
    /// the selected paragraphs differ in.
    fn measure_blank(self, unit: Unit) -> Self;
}

impl MeasureExt for egui::DragValue<'_> {
    fn measure(self, unit: Unit) -> Self {
        let sep = decimal();
        self.suffix(unit_suffix(unit))
            .custom_formatter(move |n, decimals| {
                let s = egui::emath::format_with_decimals_in_range(n, decimals);
                if sep == '.' { s } else { s.replace('.', &sep.to_string()) }
            })
            .custom_parser(move |text| parse_length(text, unit))
    }
    fn measure_blank(self, unit: Unit) -> Self {
        self.custom_formatter(|_, _| String::new()).custom_parser(move |text| parse_length(text, unit))
    }
}

/// Regions offered in File › Options › Advanced › Region (any locale tag works through
/// `file.options`): Letter and A4 countries, decimal point and decimal comma.
const REGIONS: [&str; 16] =
    ["en-US", "en-CA", "en-GB", "en-AU", "en-IN", "es-MX", "es-ES", "pt-BR", "fr-FR", "de-DE", "it-IT", "pl-PL", "ru-RU", "ja-JP", "zh-CN", "ko-KR"];

/// The units offered, with their names.
pub const UNITS: [(Unit, &str); 5] = [
    (Unit::Inches, "Inches"),
    (Unit::Centimeters, "Centimeters"),
    (Unit::Millimeters, "Millimeters"),
    (Unit::Points, "Points"),
    (Unit::Picas, "Picas"),
];

/// The pane names, in [`PANES`] order.
fn pane_label(id: &str) -> &'static str {
    match id {
        "display" => "Display",
        "proofing" => "Proofing",
        "save" => "Save",
        "language" => "Language",
        "accessibility" => "Accessibility",
        "advanced" => "Advanced",
        "agents" => "Agents",
        "defaults" => "Defaults",
        _ => "General",
    }
}

/// The Options page.
pub fn page(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(tl!("Options")).font(semibold(26.0)));
    ui.add_space(16.0);
    let current = if PANES.contains(&app.ui.options_pane.as_str()) { app.ui.options_pane.clone() } else { "general".to_string() };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            for id in PANES {
                let label = tl!(pane_label(id));
                let active = current == id;
                let (r, resp) = ui.allocate_exact_size(vec2(170.0, 30.0), egui::Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, label));
                if active {
                    ui.painter().rect_filled(r, 4.0, t.checked);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 4.0, t.hover);
                }
                let font = if active { semibold(14.0) } else { crate::theme::regular(14.0) };
                // Long translations stay inside the row; WidgetInfo above keeps the full name.
                ui.painter().with_clip_rect(r).text(egui::pos2(r.min.x + 12.0, r.center().y), egui::Align2::LEFT_CENTER, label, font, t.text);
                if resp.clicked() {
                    app.ui.options_pane = id.to_string();
                }
            }
        });
        ui.add_space(12.0);
        let (line, _) = ui.allocate_exact_size(vec2(1.0, 420.0), egui::Sense::hover());
        ui.painter().rect_filled(line, 0.0, t.border);
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.set_max_width(640.0);
            match current.as_str() {
                "display" => display(app, ui),
                "proofing" => proofing(app, ui),
                "save" => save(app, ui),
                "language" => language(app, ui),
                "accessibility" => accessibility(app, ui),
                "advanced" => advanced(app, ui),
                "agents" => agents(ui),
                "defaults" => defaults(app, ui),
                _ => general(app, ui),
            }
        });
    });
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(tl!(title)).font(semibold(15.0)));
    ui.add_space(2.0);
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(tl!(text)).small().weak());
}

/// Change settings through `file.options` (the command agents use).
fn set(app: &mut WordApp, params: Value) {
    if let Err(e) = app.run("file.options", params) {
        app.status(e);
    }
}

/// A checkbox for a `file.options` on/off setting.
fn flag(app: &mut WordApp, ui: &mut Ui, label: &str, key: &str, value: bool) {
    let mut v = value;
    if ui.checkbox(&mut v, tl!(label)).changed() {
        set(app, json!({ key: v }));
    }
}

/// Draws General options: interface theme and size, mini toolbar, dark page, community button and
/// default apps; the Start screen; and personalization such as user name and initials.
fn general(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "Interface");
    theme_picker(app, ui);
    interface_size_picker(app, ui);
    ui.checkbox(&mut app.ui.mini_toolbar, tl!("Show the mini toolbar when text is selected"));
    let mut dark_page = app.session.view.dark_mode;
    if ui
        .checkbox(&mut dark_page, tl!("Dark page (white text on black)"))
        .on_hover_text(tl!("Show documents with their colours inverted, like View › Switch Modes. Saving, printing and PDFs are unchanged."))
        .changed()
    {
        let _ = app.run("view.darkMode", json!({"value": dark_page}));
    }
    ui.checkbox(&mut app.ui.show_discord, tl!("Show the community button in the title bar"));
    default_app_button(app, ui);
    ui.add_space(8.0);
    section(ui, "Start-up");
    let mut start = app.ui.start_screen;
    if ui
        .checkbox(&mut start, tl!("Show the Start screen when WordCraft starts"))
        .on_hover_text(tl!(
            "On: WordCraft opens on File › Home, with a blank document, the templates and your recent documents. Off: it opens a blank document."
        ))
        .changed()
    {
        let _ = app.run("ui.startScreen", json!({"value": start}));
    }
    ui.add_space(8.0);
    section(ui, "Personalize");
    egui::Grid::new("options_user").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
        let user_label = ui.label(tl!("User name:"));
        let mut n = app.session.author.clone();
        let protected = wordcraft_engine::cmd::protect::enforced(&app.session.doc);
        if ui.add_enabled(!protected, egui::TextEdit::singleline(&mut n).desired_width(260.0)).labelled_by(user_label.id).changed() && !protected {
            app.session.author = n;
        }
        ui.end_row();
        let initials_label = ui.label(tl!("Initials:"));
        let mut i = app.session.initials.clone();
        let hint = app.session.user_initials();
        if ui
            .add(
                egui::TextEdit::singleline(&mut i).desired_width(80.0).char_limit(wordcraft_engine::cmd::options::MAX_INITIALS_CHARS).hint_text(hint),
            )
            .labelled_by(initials_label.id)
            .changed()
        {
            set(app, json!({"initials": i}));
        }
        ui.end_row();
    });
    note(ui, "New comments and tracked changes are recorded under this name and these initials.");
}

fn display(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "Formatting marks");
    note(ui, "These marks show even when Show/Hide ¶ is off.");
    let m = app.session.prefs.marks;
    for (label, key, on) in [
        ("Tab characters", "tabs", m.tabs),
        ("Spaces", "spaces", m.spaces),
        ("Paragraph marks", "paragraphs", m.paragraphs),
        ("Hidden text", "hidden", m.hidden),
    ] {
        let mut v = on;
        if ui.checkbox(&mut v, tl!(label)).changed() {
            set(app, json!({"marks": { key: v }}));
        }
    }
    let mut marks = app.session.view.marks;
    if ui.checkbox(&mut marks, tl!("Show all formatting marks")).changed() {
        let _ = app.run("view.marks", json!({"value": marks}));
    }
    ui.add_space(8.0);
    section(ui, "Page display");
    let mut ruler = app.session.view.ruler;
    if ui.checkbox(&mut ruler, tl!("Show rulers")).changed() {
        let _ = app.run("view.ruler", json!({"value": ruler}));
    }
    numeral_picker(app, ui);
    ui.add_space(8.0);
    section(ui, "Navigation tree");
    ui.label(tl!("Style of the roots (Navigation › Headings):"));
    let current = app.ui.nav_tree_style;
    let t = Tokens::get(ui.ctx());
    let half = ((ui.available_width() - ui.spacing().item_spacing.x) * 0.5).clamp(120.0, 430.0);
    ui.horizontal(|ui| {
        for style in [crate::panes::NavTreeStyle::ZedLinear, crate::panes::NavTreeStyle::FleetConnected] {
            let on = current == style;
            let text = egui::RichText::new(tl!(style.label())).size(11.5).color(if on { t.nav_tab_active_text } else { ui.visuals().text_color() });
            let fill = if on { t.nav_tab_active } else { t.nav_tab_bg };
            let button = egui::Button::new(text).fill(fill).stroke(egui::Stroke::new(1.0, if on { t.nav_tab_active } else { t.nav_tab_border }));
            if ui.add_sized(vec2(half, 26.0), button).clicked() && !on {
                crate::panes::set_nav_tree_style(app, style);
            }
        }
    });
    note(
        ui,
        "Switches between the minimal look with the curve only on the active item (Option A) and the look with every branch connected (Option B).",
    );
    ui.add_space(8.0);
    section(ui, "Printing");
    ui.checkbox(&mut app.ui.print_backgrounds, tl!("Print the page color"));
    ui.checkbox(&mut app.ui.update_fields_before_print, tl!("Update fields before printing"));
}

fn proofing(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "AutoCorrect");
    if ui.button(tl!("AutoCorrect Options")).clicked() {
        let _ = app.run("tools.autocorrect", json!({}));
    }
    ui.add_space(8.0);
    section(ui, "Spelling");
    let pr = app.session.prefs.clone();
    flag(app, ui, "Ignore words in capitals", "ignoreUppercase", pr.ignore_uppercase);
    flag(app, ui, "Ignore words with numbers", "ignoreNumbers", pr.ignore_numbers);
    flag(app, ui, "Ignore web and file addresses", "ignoreInternet", pr.ignore_internet);
    ui.add_space(8.0);
    section(ui, "As you type");
    let spelling = app.session.view.proofing;
    flag(app, ui, "Check spelling as you type", "checkSpelling", spelling);
    ui.add_enabled_ui(app.session.view.proofing, |ui| flag(app, ui, "Mark grammar errors as you type", "markGrammar", pr.mark_grammar));
    ui.add_space(8.0);
    section(ui, "Custom dictionary");
    custom_dictionary(app, ui);
    ui.add_space(8.0);
    section(ui, "Dictionaries");
    dictionaries(app, ui);
}

/// Where the spelling dictionaries come from (#1063): a folder searched before the system's
/// usual places, saved with the settings, and the dictionary each language found. An app started
/// from Finder or the Dock doesn't see `WORDCRAFT_DICTIONARIES`, so this is how it finds a
/// dictionary kept in a folder of the user's own.
fn dictionaries(app: &mut WordApp, ui: &mut Ui) {
    let saved = app.session.prefs.dictionaries_folder.clone().unwrap_or_default();
    let id = egui::Id::unique("options_dictionaries_folder");
    let mut folder = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| saved.clone());
    let folder_label = ui.label(tl!("Dictionaries folder:")).id;
    ui.horizontal(|ui| {
        let r = ui
            .add(egui::TextEdit::singleline(&mut folder).desired_width(320.0).hint_text(tl!("Not set: the system's usual places")))
            .labelled_by(folder_label);
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let changed = folder.trim() != saved;
        if ui.add_enabled(changed, egui::Button::new(tl!("Apply"))).clicked() || (enter && changed) {
            let value = if folder.trim().is_empty() { "auto" } else { folder.trim() };
            match app.run("file.options", json!({"dictionariesFolder": value})) {
                // Shown as saved (`~/` expanded).
                Ok(_) => folder = app.session.prefs.dictionaries_folder.clone().unwrap_or_default(),
                Err(e) => app.status(e),
            }
        }
        if ui.add_enabled(!saved.is_empty(), egui::Button::new(tl!("Clear"))).clicked()
            && app.run("file.options", json!({"dictionariesFolder": "auto"})).is_ok()
        {
            folder.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(id, folder));
    note(ui, "Hunspell .aff and .dic files here (pl_PL, pt_BR, it_IT, hr_HR, tr_TR) are used before the system's.");
    // The dictionary each language found, looked up again after the folder changes.
    let found_id = egui::Id::unique("options_dictionaries_found");
    let generation = wordcraft_engine::proof::dictionary_generation();
    let cached = ui.data(|d| d.get_temp::<(u64, Vec<(String, Option<String>, Option<String>)>)>(found_id)).filter(|(g, _)| *g == generation);
    let found = match cached {
        Some((_, f)) => f,
        None => {
            let f: Vec<(String, Option<String>, Option<String>)> = wordcraft_engine::proof::installed_dictionaries()
                .into_iter()
                .map(|(lang, dic)| {
                    let error = wordcraft_engine::proof::dictionary_load_error(lang);
                    (language_name(lang).to_string(), dic.map(|d| d.to_string_lossy().into_owned()), error)
                })
                .collect();
            ui.data_mut(|d| d.insert_temp(found_id, (generation, f.clone())));
            f
        }
    };
    for (language, file, error) in found {
        let line = match (file, error) {
            (Some(f), Some(e)) => {
                crate::i18n::fmt(tl!("{language}: {file} (unreadable: {reason})"), &[("language", &language), ("file", &f), ("reason", &e)])
            }
            (Some(f), None) => crate::i18n::fmt(tl!("{language}: {file}"), &[("language", &language), ("file", &f)]),
            (None, _) => crate::i18n::fmt(tl!("{language}: no dictionary found"), &[("language", &language)]),
        };
        ui.add(egui::Label::new(RichText::new(line).small()).truncate());
    }
}

/// The interface name of a language with a Hunspell dictionary.
fn language_name(lang: wordcraft_engine::proof::ProofLang) -> &'static str {
    use wordcraft_engine::proof::ProofLang;
    match lang {
        ProofLang::Pl => tl!("Polish"),
        ProofLang::Pt => tl!("Portuguese (Brazil)"),
        ProofLang::It => tl!("Italian"),
        ProofLang::Hr => tl!("Croatian"),
        ProofLang::Tr => tl!("Turkish"),
        ProofLang::Ru => tl!("Russian"),
        ProofLang::Cs => tl!("Czech"),
        ProofLang::Id => tl!("Indonesian"),
        ProofLang::Es => tl!("Spanish (Spain)"),
        ProofLang::EsMx => tl!("Spanish (Mexico)"),
        ProofLang::Es419 => tl!("Spanish (Latin America)"),
        ProofLang::EsUy => tl!("Spanish (Uruguay)"),
        // English uses its bundled word list; Bulgarian and German have no dictionary yet.
        ProofLang::En | ProofLang::EnGb | ProofLang::EnCa | ProofLang::Bg | ProofLang::De | ProofLang::Pack(_) | ProofLang::Other => "",
    }
}

/// The custom dictionary: the words Add to Dictionary kept, each with a Remove button, and a
/// field to add one.
fn custom_dictionary(app: &mut WordApp, ui: &mut Ui) {
    let words = wordcraft_engine::proof::user_dictionary();
    if words.is_empty() {
        note(ui, "No words yet. Add to Dictionary on a misspelled word puts it here.");
    }
    egui::ScrollArea::vertical().id_salt("options_dictionary").max_height(180.0).show(ui, |ui| {
        for w in &words {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(vec2(220.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(220.0);
                    ui.add(egui::Label::new(w.as_str()).truncate());
                });
                if ui.small_button(tl!("Remove")).clicked() {
                    set(app, json!({"removeWords": [w]}));
                }
            });
        }
    });
    let id = egui::Id::unique("options_dictionary_new");
    let mut word = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        let r = ui
            .add(egui::TextEdit::singleline(&mut word).desired_width(220.0).char_limit(MAX_WORD_CHARS).hint_text(tl!("New word")))
            .accessible_name(tl!("New word"));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let w = word.trim().to_string();
        let ok = !w.is_empty() && !w.chars().any(char::is_whitespace);
        if (ui.add_enabled(ok, egui::Button::new(tl!("Add"))).clicked() || (enter && ok)) && app.run("file.options", json!({"addWords": [w]})).is_ok()
        {
            word.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(id, word));
}

fn save(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "Save documents");
    // The browser can't write to the user's files, so AutoSave can't be turned on there (#176).
    let browser = app.autosave_block() == Some(crate::AutoSaveBlock::Browser);
    let mut autosave = app.autosave && !browser;
    let r = ui.add_enabled(!browser, egui::Checkbox::new(&mut autosave, tl!("AutoSave documents you have saved in WordCraft")));
    if browser {
        r.on_disabled_hover_text(crate::AutoSaveBlock::Browser.reason());
    } else if r.changed() {
        app.autosave = autosave;
        app.session.autosave = autosave;
    }
    note(ui, "AutoSave writes your changes to the file a moment after you stop typing.");
    ui.add_space(8.0);
    // The open document's `file.privacyOptions` (#1062): saved with it, honoured when it is saved.
    section(ui, "Privacy for this document");
    let settings = &app.session.doc.settings;
    let rows = [
        ("Remove my name from comments, changes and file properties when saving", "removePersonalInfo", settings.remove_personal_info),
        ("Leave out the date and time of comments and tracked changes when saving", "removeDateTime", settings.remove_date_time),
    ];
    for (label, key, value) in rows {
        let mut v = value;
        if ui.checkbox(&mut v, tl!(label)).changed()
            && let Err(e) = app.run("file.privacyOptions", json!({ key: v }))
        {
            app.status(e);
        }
    }
    note(ui, "The document you are editing keeps the names and dates; only the saved file leaves them out.");
}

fn language(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "Interface language");
    language_picker(app, ui);
    ui.add_space(8.0);
    section(ui, "Proofing language");
    review_language_picker(app, ui);
    note(ui, "Automatic follows the interface language. It sets the spelling and grammar dictionary for your documents.");
    let lang = crate::ribbon::caret_language(app);
    let text = if !app.session.view.proofing {
        tl!("Spelling and grammar are not checked: Review language is Off.").to_string()
    } else if crate::ribbon::caret_no_proof(app) {
        tl!("The text at the insertion point is marked Do not check spelling or grammar.").to_string()
    } else if !crate::ribbon::caret_language_proofed(app) {
        crate::i18n::fmt(tl!("{language}: WordCraft doesn't check spelling or grammar in this language yet."), &[("language", &lang)])
    } else {
        let unreadable = crate::ribbon::caret_unreadable_dictionary(app);
        crate::chrome::dictionary_problem(&lang, &unreadable, crate::ribbon::caret_missing_dictionary(app))
            .unwrap_or_else(|| crate::i18n::fmt(tl!("Spelling and grammar are checked in {language}."), &[("language", &lang)]))
    };
    ui.label(RichText::new(text).small().weak());
    ui.add_space(8.0);
    crate::language_packs::options_section(app, ui);
}

/// Draws Accessibility options: the animation setting and Read Aloud speed and skip-citations.
fn accessibility(app: &mut WordApp, ui: &mut Ui) {
    use wordcraft_engine::speech::{MAX_RATE, MIN_RATE};
    section(ui, "Visual effects");
    animations_picker(app, ui);
    ui.add_space(8.0);
    section(ui, "Read Aloud");
    let mut rate = app.session.read_aloud.rate();
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Speed"));
        let r =
            ui.add(egui::Slider::new(&mut rate, MIN_RATE..=MAX_RATE).step_by(0.05).custom_formatter(|v, _| format!("{v:.2}×"))).labelled_by(label.id);
        if r.changed() {
            let _ = app.run("readAloud.speed", json!({"value": rate}));
        }
    });
    let mut skip = app.session.read_aloud.skip_citations;
    if ui.checkbox(&mut skip, tl!("Skip citations & bibliography")).changed() {
        let _ = app.run("readAloud.skipCitations", json!({"value": skip}));
    }
}

/// Draws Advanced options: typing and overtype behavior, alignment guides, recent-document count,
/// measurement units, region, keytips, caret animation, file locations and customization.
fn advanced(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "Editing");
    let pr = app.session.prefs.clone();
    flag(app, ui, "Typing replaces selected text", "typingReplacesSelection", pr.typing_replaces_selection);
    flag(app, ui, "Use the Insert key to switch overtype on and off", "insertKeyOvertype", pr.insert_key_overtype);
    flag(app, ui, "Overtype (typing replaces the characters after the caret)", "overtype", pr.overtype);
    let mut guides = app.ui.alignment_guides;
    if ui
        .checkbox(&mut guides, tl!("Show alignment guides when moving pictures and shapes"))
        .on_hover_text(tl!("Moved objects snap to the page's edges and centre, the margins and other objects. Hold Alt to move freely."))
        .changed()
    {
        let _ = app.run("ui.alignmentGuides", json!({"value": guides}));
    }
    ui.add_space(8.0);
    section(ui, "Display");
    egui::Grid::new("options_display").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
        let recent_label = ui.label(tl!("Recent documents to show:"));
        let mut n = app.ui.recent_shown();
        if ui.add(egui::DragValue::new(&mut n).range(0..=crate::MAX_RECENT).speed(0.2)).labelled_by(recent_label.id).changed() {
            app.ui.recent_count = n;
        }
        ui.end_row();
        let units_label = ui.label(tl!("Measurement units:"));
        // The user's choice, else the region's (#838): Automatic follows the region.
        let units = app.session.units();
        let auto = app.session.prefs.units.is_none();
        let name = UNITS.iter().find(|(u, _)| *u == units).map_or("Inches", |(_, n)| n);
        let shown = if auto { format!("{} ({})", tl!("Automatic"), tl!(name)) } else { tl!(name).to_string() };
        let combo = craft_ui::ComboBox::from_id_salt("options_units").selected_text(shown).width(200.0).show_ui(ui, |ui| {
            if ui.selectable_label(auto, tl!("Automatic")).clicked() {
                set(app, json!({"units": "auto"}));
            }
            for (u, label) in UNITS {
                if ui.selectable_label(!auto && units == u, tl!(label)).clicked() {
                    set(app, json!({"units": u}));
                }
            }
        });
        combo.response.labelled_by(units_label.id);
        ui.end_row();
        // The region new documents, units and the decimal separator follow; Automatic is the
        // system's (#838).
        let region_label = ui.label(tl!("Region:"));
        let chosen = app.session.prefs.region.clone();
        let system = &app.session.system_region;
        let system_tag = if system.territory.is_empty() { system.language.clone() } else { format!("{}-{}", system.language, system.territory) };
        let automatic = crate::i18n::fmt(tl!("Automatic ({region})"), &[("region", system_tag.as_str())]);
        let shown = chosen.clone().unwrap_or_else(|| automatic.clone());
        let combo = craft_ui::ComboBox::from_id_salt("options_region").selected_text(shown).width(200.0).show_ui(ui, |ui| {
            if ui.selectable_label(chosen.is_none(), automatic.as_str()).clicked() {
                set(app, json!({"region": "auto"}));
            }
            for tag in REGIONS {
                if ui.selectable_label(chosen.as_deref() == Some(tag), tag).clicked() {
                    set(app, json!({"region": tag}));
                }
            }
        });
        combo.response.labelled_by(region_label.id);
        ui.end_row();
    });
    let mut keytips = app.ui.keytips_enabled;
    if ui.checkbox(&mut keytips, tl!("Show KeyTips (Alt or F10)")).changed() {
        let _ = app.run("ui.keytips", json!({"value": keytips}));
    }
    ui.add_space(8.0);
    section(ui, "Animation");
    let mut caret = app.ui.caret_animation;
    if ui
        .checkbox(&mut caret, tl!("Animate the caret (it glides and fades)"))
        .on_hover_text(tl!(
            "On: the caret glides to its new place when you type or use the arrow keys, and its blink fades. Off: it jumps and blinks on and off."
        ))
        .changed()
    {
        let _ = app.run("ui.caretAnimation", json!({"value": caret}));
    }
    // The web app has no folders to name.
    if !cfg!(target_arch = "wasm32") {
        ui.add_space(8.0);
        section(ui, "File locations");
        templates_folder(app, ui);
    }
    ui.add_space(8.0);
    section(ui, "Customize");
    ui.horizontal(|ui| {
        ui.label(tl!("Keyboard shortcuts:"));
        if ui.button(tl!("Customize…")).clicked() {
            let _ = app.run("tools.customizeKeyboard", json!({}));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Ribbon and Quick Access Toolbar:"));
        if ui.button(tl!("Customize…")).clicked() {
            let _ = app.run("tools.customizeRibbon", json!({}));
        }
    });
}

/// The user templates folder (#2852): where File › New finds personal templates and Save as Word
/// Template starts. Empty is the app's own folder, shown as the hint.
fn templates_folder(app: &mut WordApp, ui: &mut Ui) {
    let saved = app.session.prefs.user_templates_folder.clone().unwrap_or_default();
    let id = egui::Id::unique("options_user_templates_folder");
    let mut folder = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| saved.clone());
    let default = app.services.templates_dir.as_ref().map(|d| d.to_string_lossy().into_owned());
    let templates_label = ui.label(tl!("User templates:")).id;
    ui.horizontal(|ui| {
        let hint = default.clone().unwrap_or_else(|| tl!("Not set").to_string());
        let r = ui.add(egui::TextEdit::singleline(&mut folder).desired_width(320.0).hint_text(hint)).labelled_by(templates_label);
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let changed = folder.trim() != saved;
        if ui.add_enabled(changed, egui::Button::new(tl!("Apply"))).clicked() || (enter && changed) {
            let value = if folder.trim().is_empty() { "auto" } else { folder.trim() };
            match app.run("file.options", json!({"userTemplatesFolder": value})) {
                Ok(_) => folder = app.session.prefs.user_templates_folder.clone().unwrap_or_default(),
                Err(e) => app.status(e),
            }
        }
        if ui.add_enabled(!saved.is_empty(), egui::Button::new(tl!("Clear"))).clicked()
            && app.run("file.options", json!({"userTemplatesFolder": "auto"})).is_ok()
        {
            folder.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(id, folder));
    note(ui, "File › New lists the .dotx and .dotm templates here under Personal; Save as Word Template starts here.");
}

fn agents(ui: &mut Ui) {
    section(ui, "Agents");
    ui.label(tl!("Every command is available to scripts and AI agents: run `wordcraft-cli mcp` for an MCP server, or start the app with `--control <port>` for the JSON control channel."));
}

/// Interface theme: Light, Dark, or follow the system's appearance (#115).
fn theme_picker(app: &mut WordApp, ui: &mut Ui) {
    use crate::theme::Appearance;
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Interface theme:"));
        let combo = craft_ui::ComboBox::from_id_salt("interface_theme").selected_text(tl!(app.ui.theme.label())).width(220.0).show_ui(ui, |ui| {
            for a in Appearance::ALL {
                if ui.selectable_label(app.ui.theme == a, tl!(a.label())).clicked() {
                    let _ = app.run("ui.theme", json!({"value": a.code()}));
                }
            }
        });
        combo.response.labelled_by(label.id);
    });
}

/// Interface size (#475, #822): Auto, which keeps the system's display scale and shows the size it
/// draws at, or an absolute size, so "100%" means one screen pixel per point.
fn interface_size_picker(app: &mut WordApp, ui: &mut Ui) {
    use crate::theme::InterfaceSize;
    let size = app.ui.interface_size;
    let native = ui.ctx().native_pixels_per_point().unwrap_or(1.0);
    let auto = crate::i18n::fmt(tl!("Auto ({percent}%)"), &[("percent", &InterfaceSize::AUTO.drawn_percent(native).to_string())]);
    let selected = size.percent().map_or_else(|| auto.clone(), |pct| format!("{pct}%"));
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Interface size:"));
        let combo = craft_ui::ComboBox::from_id_salt("interface_size").selected_text(selected).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(size.is_auto(), auto).clicked() {
                let _ = app.run("ui.interfaceSize", json!({"value": "auto"}));
            }
            for pct in InterfaceSize::PRESETS {
                if ui.selectable_label(size.percent() == Some(pct), format!("{pct}%")).clicked() {
                    let _ = app.run("ui.interfaceSize", json!({"percent": pct}));
                }
            }
        });
        combo.response.labelled_by(label.id);
    });
    ui.label(
        RichText::new(tl!(
            "Makes the ribbon, menus and dialogs larger or smaller. Auto follows your system's display scaling; 100% is one screen pixel per point. Document zoom is separate."
        ))
        .small()
        .weak(),
    );
}

/// Animation effects: on, off, or follow the system's setting (the default). Under "Use system
/// setting" a line says what the system reports, so it is plain what the choice comes to here.
fn animations_picker(app: &mut WordApp, ui: &mut Ui) {
    use crate::theme::Animations;
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Animation effects:"));
        let combo =
            craft_ui::ComboBox::from_id_salt("animation_effects").selected_text(tl!(app.ui.animations.label())).width(220.0).show_ui(ui, |ui| {
                for a in Animations::ALL {
                    if ui.selectable_label(app.ui.animations == a, tl!(a.label())).clicked() {
                        let _ = app.run("ui.animations", json!({"value": a.code()}));
                    }
                }
            });
        combo.response.labelled_by(label.id);
    });
    note(ui, "Smooth scrolling and zooming, and menus and buttons that fade in. Off shows every change at once.");
    if app.ui.animations == Animations::System {
        let reported = match app.system_animations() {
            Some(true) => "Your system has animation effects turned on.",
            Some(false) => "Your system has animation effects turned off.",
            None => "Your system doesn't report an animation setting, so animation effects stay on.",
        };
        note(ui, reported);
    }
}

/// Interface language: follow the system (the default) or pick one (#8).
fn language_picker(app: &mut WordApp, ui: &mut Ui) {
    use crate::i18n::{AUTO, Lang};
    // The language names are in their own scripts: fonts for them load from the next frame.
    app.want_system_cjk = true;
    let system = crate::i18n::system_lang();
    let auto_label = crate::i18n::fmt(tl!("Automatic ({language})"), &[("language", system.name())]);
    let current = if app.ui.language == AUTO { auto_label.clone() } else { Lang::from_pref(&app.ui.language).name().to_string() };
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Interface language:"));
        let combo = craft_ui::ComboBox::from_id_salt("interface_language").selected_text(current).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(app.ui.language == AUTO, auto_label.as_str()).clicked() {
                let _ = app.run("ui.language", json!({"value": AUTO}));
            }
            for lang in Lang::for_picker() {
                if ui.selectable_label(app.ui.language == lang.code(), lang.name()).clicked() {
                    let _ = app.run("ui.language", json!({"value": lang.code()}));
                }
            }
        });
        combo.response.labelled_by(label.id);
    });
    ui.label(RichText::new(tl!("Automatic follows your system's language. Menus and commands change; your documents don't.")).small().weak());
}

/// Deserialize `input` over `T`'s defaults one setting at a time: a value of the wrong type is
/// dropped (its default stays) instead of failing the whole read. Objects nest (`editing`).
pub fn lenient<T: Serialize + DeserializeOwned + Default>(input: &Value) -> T {
    let mut base = serde_json::to_value(T::default()).unwrap_or(Value::Null);
    if base.is_object() {
        let mut extra = Vec::new();
        overlay::<T>(&mut base, &mut Vec::new(), input, 0, &mut extra);
        // Keys with no slot in the defaults (write-only legacy settings, or unknown ones) are
        // kept only after the loop, so `base` stays the size of `T` while each key is checked.
        for (path, k, v) in extra {
            if let Some(parent) = object_at(&mut base, &path) {
                parent.insert(k, v);
            }
        }
    }
    serde_json::from_value(base).unwrap_or_default()
}

/// The object at `path` inside `root`.
fn object_at<'a>(root: &'a mut Value, path: &[String]) -> Option<&'a mut Map<String, Value>> {
    let mut v = root;
    for k in path {
        v = v.as_object_mut()?.get_mut(k)?;
    }
    v.as_object_mut()
}

fn overlay<T: DeserializeOwned>(
    root: &mut Value,
    path: &mut Vec<String>,
    input: &Value,
    depth: usize,
    extra: &mut Vec<(Vec<String>, String, Value)>,
) {
    let Some(map) = input.as_object() else { return };
    for (k, v) in map {
        let Some(parent) = object_at(root, path) else { return };
        let old = parent.get(k).cloned();
        if depth < 4 && v.is_object() && old.as_ref().is_some_and(Value::is_object) {
            path.push(k.clone());
            overlay::<T>(root, path, v, depth + 1, extra);
            path.pop();
            continue;
        }
        parent.insert(k.clone(), v.clone());
        if old.is_none() {
            let ok = serde_json::from_value::<T>(root.clone()).is_ok();
            if let Some(parent) = object_at(root, path) {
                parent.remove(k);
            }
            if ok {
                extra.push((path.clone(), k.clone(), v.clone()));
            }
            continue;
        }
        if serde_json::from_value::<T>(root.clone()).is_err()
            && let Some(parent) = object_at(root, path)
        {
            match old {
                Some(o) => parent.insert(k.clone(), o),
                None => parent.remove(k),
            };
        }
    }
}

/// File ▸ Options ▸ General: make WordCraft the default app for Word documents (#295). Only where
/// the host can (`Services::make_default_app`: Windows opens Default apps, Linux and BSD use
/// `xdg-mime`); macOS shows how to do it in Finder, the web shows nothing.
fn default_app_button(app: &mut WordApp, ui: &mut Ui) {
    if app.services.make_default_app.is_some() {
        let hint = if cfg!(target_os = "windows") {
            tl!("Opens Settings › Default apps, where you choose WordCraft for .docx and the other document types.")
        } else {
            tl!("Opens .docx, .doc, .rtf and .odt files and Word templates with WordCraft from now on.")
        };
        if ui.button(tl!("Make WordCraft the default for Word documents…")).on_hover_text(hint).clicked() {
            // Straight to the hook, not through `run`: only a click may change system defaults.
            let _ = app.make_default_app();
        }
    } else if cfg!(target_os = "macos") {
        ui.label(
            egui::RichText::new(tl!("To open Word documents with WordCraft by default: select one in Finder, choose File › Get Info, pick WordCraft under Open with, then click Change All."))
                .small()
                .weak(),
        );
    }
}

/// File ▸ Options ▸ Numerals: how digits look on screen (display only, like Word's
/// File › Options › Advanced › Numeral).
fn numeral_picker(app: &mut WordApp, ui: &mut Ui) {
    use wordcraft_layout::NumeralMode;
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Numerals:"));
        let mut sel = app.session.view.numeral;
        let combo = craft_ui::ComboBox::from_id_salt("numerals").selected_text(tl!(sel.label())).width(220.0).show_ui(ui, |ui| {
            for m in [NumeralMode::Arabic, NumeralMode::Hindi, NumeralMode::Context, NumeralMode::System] {
                ui.selectable_value(&mut sel, m, tl!(m.label()));
            }
        });
        combo.response.labelled_by(label.id);
        if sel != app.session.view.numeral {
            let _ = app.run("view.numeral", json!({ "value": sel }));
        }
    });
}

/// A "Saved" note under an action for a moment after it ran (the dialog's own feedback that the
/// defaults were written, without a new message that would have to be translated everywhere).
fn flash(ui: &mut Ui, action: &str) {
    // The time is read before the data lock: `Context::input` takes the context's own lock, and
    // calling it inside `data_mut` deadlocks.
    let now = ui.input(|i| i.time);
    ui.data_mut(|d| d.insert_temp(egui::Id::unique(("defaults_saved", action)), now));
}

fn flashed(ui: &Ui, action: &str) -> bool {
    let now = ui.input(|i| i.time);
    ui.data(|d| d.get_temp::<f64>(egui::Id::unique(("defaults_saved", action)))).is_some_and(|t| now - t < 2.0)
}

fn saved_note(ui: &mut Ui, action: &str) {
    if flashed(ui, action) {
        ui.label(RichText::new(tl!("Saved")).small().weak());
        ui.ctx().request_repaint();
    }
}

/// A Reset button that asks once (OK/Cancel) before throwing the person's defaults away: a stray
/// click on a small button shouldn't lose them.
fn confirm_reset(ui: &mut Ui, key: &str) -> bool {
    let id = egui::Id::unique(("defaults_reset", key));
    if !ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        if ui.button(tl!("Reset")).clicked() {
            ui.data_mut(|d| d.insert_temp(id, true));
        }
        return false;
    }
    let mut go = false;
    ui.label(tl!("Reset"));
    if ui.button(tl!("OK")).clicked() {
        go = true;
    }
    if ui.button(tl!("Cancel")).clicked() {
        ui.data_mut(|d| d.insert_temp(id, false));
    }
    if go {
        ui.data_mut(|d| d.insert_temp(id, false));
    }
    go
}

/// File › Options › Defaults: what a new document starts from. The view switches are the person's
/// preferences (saved with the app); the styles and the page setup are adopted from a document —
/// or reset — and only ever applied to documents created afterwards.
/// The style being edited in the Defaults pane: the resolved properties the Modify Style dialog
/// shows, kept apart from any document.
#[derive(Clone)]
pub(super) struct StyleDraft {
    /// Style id (the model's key; `basedOn` links use it).
    pub(super) id: String,
    /// The style's own name (not the localized label): what is saved.
    pub(super) name: String,
    /// The label shown in the pane (localized for built-in styles).
    pub(super) display: String,
    pub(super) font: String,
    pub(super) size: f32,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) color: String,
    pub(super) before: f32,
    pub(super) after: f32,
}

/// The page setup being edited in the Defaults pane, in the measurement unit the dialogs show.
#[derive(Clone, Debug)]
pub(super) struct PageDraft {
    pub(super) paper: (f32, f32),
    pub(super) custom: bool,
    pub(super) landscape: bool,
    pub(super) top: f32,
    pub(super) bottom: f32,
    pub(super) left: f32,
    pub(super) right: f32,
    pub(super) mirror: bool,
    /// `RRGGBB`, or empty for the theme's paper.
    pub(super) color: String,
}

pub(super) fn style_draft_id() -> egui::Id {
    egui::Id::unique("defaults_style_draft")
}

pub(super) fn page_draft_id() -> egui::Id {
    egui::Id::unique("defaults_page_draft")
}

impl PageDraft {
    /// The app's own page setup, for the editor's first show and a reset.
    fn default_page() -> Self {
        page_draft_from(&wordcraft_doc::SectionProps::default(), false, None)
    }
}

fn color_text(value: &str) -> wordcraft_doc::props::TextColor {
    match wordcraft_doc::Rgb::parse(value) {
        Some(c) => wordcraft_doc::props::TextColor::Rgb(c),
        None => wordcraft_doc::props::TextColor::Auto,
    }
}

/// A style's properties as the Modify Style dialog shows them: resolved through its `basedOn`
/// chain, so an edited Normal doesn't hide what Heading 1 inherits.
pub(super) fn draft_from_style(sheet: &wordcraft_doc::styles::StyleSheet, id: &str) -> Option<StyleDraft> {
    let st = sheet.get(id)?;
    let character = st.kind == wordcraft_doc::StyleKind::Character;
    let rc = sheet.resolve_char(
        if character { None } else { Some(id) },
        &wordcraft_doc::CharProps { style: character.then(|| id.to_string()), ..Default::default() },
    );
    let rp = sheet.resolve_para(&wordcraft_doc::ParaProps { style: Some(id.to_string()), ..Default::default() });
    Some(StyleDraft {
        id: id.to_string(),
        name: st.name.clone(),
        display: crate::i18n::style_name(st),
        font: rc.font,
        size: rc.size,
        bold: rc.bold,
        italic: rc.italic,
        color: match rc.color {
            wordcraft_doc::props::TextColor::Rgb(c) => c.hex(),
            wordcraft_doc::props::TextColor::Auto => "auto".into(),
        },
        before: rp.space_before,
        after: rp.space_after,
    })
}

/// The style the draft describes, for the sample: the default's own definition with the edited
/// properties laid over it (the preview never touches the document).
fn draft_style(sheet: &wordcraft_doc::styles::StyleSheet, draft: &StyleDraft) -> wordcraft_doc::styles::Style {
    let mut st = sheet.get(&draft.id).cloned().unwrap_or_default();
    st.id = draft.id.clone();
    st.name = draft.name.clone();
    st.chr.font = Some(draft.font.clone());
    st.chr.size = Some(draft.size);
    st.chr.bold = Some(draft.bold);
    st.chr.italic = Some(draft.italic);
    st.chr.color = Some(color_text(&draft.color));
    st.para.space_before = Some(draft.before);
    st.para.space_after = Some(draft.after);
    st
}

fn page_draft_from_defaults(p: &wordcraft_doc::defaults::PageDefaults) -> PageDraft {
    page_draft_from(
        &wordcraft_doc::SectionProps {
            page_w: p.page_w,
            page_h: p.page_h,
            landscape: p.landscape,
            margin_top: p.margin_top,
            margin_bottom: p.margin_bottom,
            margin_left: p.margin_left,
            margin_right: p.margin_right,
            ..Default::default()
        },
        p.mirror,
        p.page_color,
    )
}

fn page_draft_from(sect: &wordcraft_doc::SectionProps, mirror: bool, color: Option<wordcraft_doc::Rgb>) -> PageDraft {
    let ppu = crate::options::unit().pt_per_unit();
    let paper = (sect.page_w / ppu, sect.page_h / ppu);
    PageDraft {
        paper,
        custom: true,
        landscape: sect.landscape,
        top: sect.margin_top / ppu,
        bottom: sect.margin_bottom / ppu,
        left: sect.margin_left / ppu,
        right: sect.margin_right / ppu,
        mirror,
        color: color.map(|c| c.hex()).unwrap_or_default(),
    }
}

/// File › Options › Defaults: what a new document starts from, edited right here — the view
/// switches, the default styles (each one shown and editable like the Modify Style dialog) and
/// the default page setup. Only the adopted defaults change: the open document is never touched.
fn defaults(app: &mut WordApp, ui: &mut Ui) {
    section(ui, "Defaults");

    // A. View preferences: the same switches as View › Show, saved with the app's preferences.
    ui.label(RichText::new(tl!("View")).font(semibold(13.5)));
    let mut ruler = app.session.view.ruler;
    let mut gridlines = app.session.view.gridlines;
    let mut table_gridlines = app.session.view.table_gridlines;
    let mut marks = app.session.view.marks;
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut ruler, tl!("Ruler")).changed() {
            let _ = app.run("view.ruler", json!({"value": ruler}));
        }
        if ui.checkbox(&mut gridlines, tl!("Gridlines")).changed() {
            let _ = app.run("view.gridlines", json!({"value": gridlines}));
        }
        if ui.checkbox(&mut table_gridlines, tl!("View Gridlines")).changed() {
            let _ = app.run("table.viewGridlines", json!({"value": table_gridlines}));
        }
        if ui.checkbox(&mut marks, tl!("Show/Hide ¶")).changed() {
            let _ = app.run("view.marks", json!({"value": marks}));
        }
        if confirm_reset(ui, "view") {
            let _ = app.run("defaults.reset", json!({"what": "view"}));
        }
    });

    // B. Default styles: pick one, edit it, see it, adopt it — without a document as the source.
    ui.add_space(12.0);
    ui.label(RichText::new(tl!("Styles")).font(semibold(13.5)));
    let sheet = app.session.defaults.styles_sheet();
    let ids: Vec<String> = sheet
        .styles
        .iter()
        .filter(|s| !s.hidden && matches!(s.kind, wordcraft_doc::StyleKind::Paragraph | wordcraft_doc::StyleKind::Character))
        .map(|s| s.id.clone())
        .collect();
    if ids.is_empty() {
        return;
    }
    let Some(mut draft) = ui
        .data(|d| d.get_temp::<StyleDraft>(style_draft_id()))
        .filter(|d| ids.contains(&d.id))
        .or_else(|| draft_from_style(&sheet, "Normal").or_else(|| draft_from_style(&sheet, &ids[0])))
    else {
        return;
    };
    ui.horizontal(|ui| {
        let style_label = ui.label(tl!("Style:"));
        let combo = craft_ui::ComboBox::from_id_salt("defaults_style_pick").selected_text(&draft.display).width(240.0).show_ui(ui, |ui| {
            for id in &ids {
                let label = sheet.get(id).map(crate::i18n::style_name).unwrap_or_else(|| id.clone());
                if ui.selectable_label(draft.id == *id, label.clone()).clicked()
                    && draft.id != *id
                    && let Some(next) = draft_from_style(&sheet, id)
                {
                    draft = next;
                }
            }
        });
        combo.response.labelled_by(style_label.id);
    });
    let display = draft.display.clone();
    crate::dialogs::style_fields(
        app,
        ui,
        crate::dialogs::StyleFields {
            name: &mut draft.name,
            font: &mut draft.font,
            size: &mut draft.size,
            bold: &mut draft.bold,
            italic: &mut draft.italic,
            color: &mut draft.color,
            before: &mut draft.before,
            after: &mut draft.after,
        },
        false,
        Some(&display),
    );
    // The sample: drawn on a temporary document with the defaults' styles (never the open one).
    let sample = draft_style(&sheet, &draft);
    let ppp = ui.ctx().pixels_per_point();
    let key = format!(
        "defaults-style:{}:{}:{}:{}:{}:{}:{}:{}:{ppp}",
        draft.id, draft.font, draft.size, draft.bold, draft.italic, draft.color, draft.before, draft.after
    );
    let text = tl!("AaBbCcDd").to_string();
    let tex = app.previews.get_or(ui.ctx(), &key, || crate::previews::style_sample(&sheet, &sample, &text, 260.0, 40.0, ppp));
    match tex {
        Some(h) => {
            let t = Tokens::get(ui.ctx());
            let sz = h.size_vec2() / ppp;
            let (r, _) = ui.allocate_exact_size(vec2(268.0, 34.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 3.0, if t.dark { egui::Color32::from_gray(0xC8) } else { egui::Color32::WHITE });
            ui.painter().rect_stroke(r, 3.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
            let inner = egui::Rect::from_min_size(r.min + vec2(4.0, 2.0), sz.min(r.size() - vec2(8.0, 4.0)));
            ui.painter().image(h.id(), inner, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            ui.ctx().request_repaint();
        }
    }
    ui.horizontal(|ui| {
        if ui.button(tl!("Set as Default")).clicked() {
            let params = style_params_for_draft(&draft, false);
            if app.run("defaults.saveStyle", params).is_ok() {
                // Show what was stored (the engine validates and cleans the definition).
                if let Some(saved) = draft_from_style(&app.session.defaults.styles_sheet(), &draft.id) {
                    draft = saved;
                }
                flash(ui, "style");
            }
        }
        if ui.button(tl!("Import from Document")).clicked() {
            // Fill the editor from the open document's definition, without adopting it.
            if let Some(next) = draft_from_style(&app.session.doc.styles, &draft.id) {
                draft = next;
            }
        }
        if confirm_reset(ui, "style") {
            let _ = app.run("defaults.resetStyle", json!({"style": draft.id}));
            if let Some(next) = draft_from_style(&app.session.defaults.styles_sheet(), &draft.id) {
                draft = next;
            }
        }
        ui.label(tl!("All"));
        if confirm_reset(ui, "styles_all") {
            let _ = app.run("defaults.reset", json!({"what": "styles"}));
            if let Some(next) = draft_from_style(&app.session.defaults.styles_sheet(), &draft.id) {
                draft = next;
            }
        }
        saved_note(ui, "style");
    });
    ui.data_mut(|d| d.insert_temp(style_draft_id(), draft));

    // C. Default page setup: edited here, adopted here; nothing comes from a document unless the
    // person asks for it (Import).
    ui.add_space(12.0);
    ui.label(RichText::new(tl!("Page Setup")).font(semibold(13.5)));
    let mut page = ui.data(|d| d.get_temp::<PageDraft>(page_draft_id())).unwrap_or_else(|| match app.session.defaults.page {
        Some(p) => page_draft_from_defaults(&p),
        None => PageDraft::default_page(),
    });
    crate::dialogs::page_paper_group(ui, &mut page.paper, &mut page.landscape, &mut page.custom);
    ui.horizontal(|ui| {
        ui.radio_value(&mut page.landscape, false, tl!("Portrait"));
        ui.radio_value(&mut page.landscape, true, tl!("Landscape"));
    });
    egui::Grid::new("defaults_page").num_columns(4).show(ui, |ui| {
        let ppu = crate::options::unit().pt_per_unit();
        let unit = crate::options::unit();
        let speed = 3.6 / ppu;
        ui.label(tl!("Top:"));
        ui.add(egui::DragValue::new(&mut page.top).speed(speed).measure(unit).max_decimals(2));
        ui.label(tl!("Bottom:"));
        ui.add(egui::DragValue::new(&mut page.bottom).speed(speed).measure(unit).max_decimals(2));
        ui.end_row();
        ui.label(if page.mirror { tl!("Inside:") } else { tl!("Left:") });
        ui.add(egui::DragValue::new(&mut page.left).speed(speed).measure(unit).max_decimals(2));
        ui.label(if page.mirror { tl!("Outside:") } else { tl!("Right:") });
        ui.add(egui::DragValue::new(&mut page.right).speed(speed).measure(unit).max_decimals(2));
        ui.end_row();
    });
    ui.checkbox(&mut page.mirror, tl!("Mirror margins"));
    ui.horizontal(|ui| {
        ui.label(tl!("Page Color"));
        let theme = app.session.doc.settings.theme_colors.clone();
        crate::dialogs::color_field(ui, &theme, &mut page.color, "No Color", "");
        // Swatch of the chosen sheet colour.
        let t = Tokens::get(ui.ctx());
        let (r, _) = ui.allocate_exact_size(vec2(28.0, 18.0), egui::Sense::hover());
        match wordcraft_doc::Rgb::parse(&page.color) {
            Some(c) => {
                ui.painter().rect_filled(r, 2.0, crate::theme::c32(c));
            }
            None => {
                ui.painter().rect_filled(r, 2.0, egui::Color32::WHITE);
                ui.painter().line_segment([r.left_bottom(), r.right_top()], egui::Stroke::new(1.0, t.text_dim));
            }
        }
        ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    });
    page_preview(ui, &page);
    ui.horizontal(|ui| {
        if ui.button(tl!("Set as Default")).clicked() {
            let params = page_params(&page);
            if app.run("defaults.savePageValues", params).is_ok() {
                // The controls show what was stored: a value the page model limits (600 cm, a
                // margin that leaves no room) can't stay on screen as if it had been adopted.
                if let Some(saved) = app.session.defaults.page {
                    page = page_draft_from_defaults(&saved);
                }
                flash(ui, "page");
            }
        }
        if ui.button(tl!("Import from Document")).clicked() {
            let sect = wordcraft_engine::cmd::page::sect(&app.session);
            page = page_draft_from(&sect, app.session.doc.settings.mirror_margins, app.session.doc.settings.page_color);
        }
        if confirm_reset(ui, "page") {
            let _ = app.run("defaults.reset", json!({"what": "page"}));
            page = page_draft_from(&wordcraft_doc::SectionProps::default(), false, None);
        }
        saved_note(ui, "page");
    });
    ui.data_mut(|d| d.insert_temp(page_draft_id(), page));

    // All the defaults at once (view preferences included), after a confirmation.
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.label(tl!("All"));
        if confirm_reset(ui, "all") {
            let _ = app.run("defaults.reset", json!({"what": "all"}));
            ui.data_mut(|d| d.remove::<StyleDraft>(style_draft_id()));
            ui.data_mut(|d| d.remove::<PageDraft>(page_draft_id()));
        }
        saved_note(ui, "all");
    });
}

/// The `defaults.saveStyle` parameters for the pane's draft (`apply` false: the open document is
/// not changed by editing the defaults here).
fn style_params_for_draft(draft: &StyleDraft, apply: bool) -> serde_json::Value {
    let mut chr = json!({"font": draft.font, "size": draft.size, "bold": draft.bold, "italic": draft.italic});
    if let Some(c) = wordcraft_doc::Rgb::parse(&draft.color) {
        chr["color"] = json!({"Rgb": [c.0, c.1, c.2]});
    }
    json!({
        "style": draft.id,
        "name": draft.name,
        "chr": chr,
        "para": {"spaceBefore": draft.before, "spaceAfter": draft.after},
        "apply": apply,
    })
}

/// The `defaults.savePageValues` parameters for the pane's draft (points, as the engine takes).
fn page_params(page: &PageDraft) -> serde_json::Value {
    let ppu = crate::options::unit().pt_per_unit();
    json!({
        "pageW": page.paper.0 * ppu,
        "pageH": page.paper.1 * ppu,
        "landscape": page.landscape,
        "marginTop": page.top * ppu,
        "marginBottom": page.bottom * ppu,
        "marginLeft": page.left * ppu,
        "marginRight": page.right * ppu,
        "mirror": page.mirror,
        "color": wordcraft_doc::Rgb::parse(&page.color).map(|c| c.hex()),
    })
}

/// A small, proportional view of the sheet: orientation, margins and colour, scaled to fit.
fn page_preview(ui: &mut Ui, page: &PageDraft) {
    let t = Tokens::get(ui.ctx());
    let ppu = crate::options::unit().pt_per_unit();
    let (w, h) = (page.paper.0 * ppu, page.paper.1 * ppu);
    let box_size = vec2(200.0, 150.0);
    let k = (box_size.x / w.max(1.0)).min(box_size.y / h.max(1.0)).min(1.0);
    let size = vec2((w * k).max(8.0), (h * k).max(8.0));
    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let paper = wordcraft_doc::Rgb::parse(&page.color).map(crate::theme::c32).unwrap_or(egui::Color32::WHITE);
    ui.painter().rect_filled(r, 1.0, paper);
    ui.painter().rect_stroke(r, 1.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    let m = |v: f32| (v * ppu * k).max(0.0);
    let inner = egui::Rect::from_min_max(
        egui::pos2(r.min.x + m(page.left), r.min.y + m(page.top)),
        egui::pos2((r.max.x - m(page.right)).max(r.min.x + 1.0), (r.max.y - m(page.bottom)).max(r.min.y + 1.0)),
    );
    ui.painter().rect_stroke(inner, 0.0, egui::Stroke::new(1.0, t.text_disabled), egui::StrokeKind::Inside);
}

/// Review language: the app's proofing default for text that names no language. The same shapes
/// as the interface picker above (Dropdown + helper line); Automatic follows the interface, Off
/// turns the live check off, and a language is used as it is. The choice is persisted in
/// `ui.json` and applies to the open document's default language at once.
fn review_language_picker(app: &mut WordApp, ui: &mut Ui) {
    use crate::ribbon::{PROOFING_LANGUAGES, language_name};
    let checking = app.session.view.proofing;
    let saved = app.session.prefs.default_language.clone();
    let resolved = saved.clone().or_else(|| app.session.editing_lang.clone()).unwrap_or_else(|| "en-US".to_string());
    let resolved_name = tl!(&language_name(Some(&resolved))).to_string();
    let automatic = crate::i18n::fmt(tl!("Automatic ({language})"), &[("language", &resolved_name)]);
    let shown = if !checking {
        tl!("Do not check spelling or grammar").to_string()
    } else if saved.is_none() {
        automatic.clone()
    } else {
        tl!(&language_name(Some(&resolved))).to_string()
    };
    let mut chosen: Option<&str> = None;
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Review language:"));
        let combo = craft_ui::ComboBox::from_id_salt("review_language").selected_text(shown).width(260.0).show_ui(ui, |ui| {
            if ui.selectable_label(checking && saved.is_none(), automatic.as_str()).clicked() {
                chosen = Some("auto");
            }
            for (tag, _) in PROOFING_LANGUAGES.iter().filter(|(t, _)| ["pt-BR", "pt-PT", "en-US", "es-ES"].contains(t)) {
                let name = tl!(&language_name(Some(tag))).to_string();
                let on = checking && saved.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(tag));
                if ui.selectable_label(on, name).clicked() {
                    chosen = Some(tag);
                }
            }
            if ui.selectable_label(!checking, tl!("Do not check spelling or grammar")).clicked() {
                chosen = Some("off");
            }
        });
        combo.response.labelled_by(label.id);
    });
    if let Some(value) = chosen
        && let Err(e) = app.run("review.defaultLanguage", json!({"value": value}))
    {
        app.status(e);
    }
}

#[cfg(test)]
mod tests;
