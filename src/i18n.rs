use crate::models::AppLanguage;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

static GREEK: Lazy<HashMap<String, String>> = Lazy::new(|| parse_po(include_str!("../po/el.po")));
static ITALIAN: Lazy<HashMap<String, String>> = Lazy::new(|| parse_po(include_str!("../po/it.po")));
static SPANISH: Lazy<HashMap<String, String>> = Lazy::new(|| parse_po(include_str!("../po/es.po")));
static GERMAN: Lazy<HashMap<String, String>> = Lazy::new(|| parse_po(include_str!("../po/de.po")));
static FRENCH: Lazy<HashMap<String, String>> = Lazy::new(|| parse_po(include_str!("../po/fr.po")));

// Stores the resolved language, so `Automatic` never needs to inspect the
// environment while widgets are being built.
static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(AppLanguage::English as u8);

pub fn set_language(language: AppLanguage) {
    let resolved = match language {
        AppLanguage::Automatic => automatic_language(),
        language => language,
    };
    CURRENT_LANGUAGE.store(resolved as u8, Ordering::Relaxed);
}

pub fn current_language() -> AppLanguage {
    match CURRENT_LANGUAGE.load(Ordering::Relaxed) {
        value if value == AppLanguage::Greek as u8 => AppLanguage::Greek,
        value if value == AppLanguage::Italian as u8 => AppLanguage::Italian,
        value if value == AppLanguage::Spanish as u8 => AppLanguage::Spanish,
        value if value == AppLanguage::German as u8 => AppLanguage::German,
        value if value == AppLanguage::French as u8 => AppLanguage::French,
        _ => AppLanguage::English,
    }
}

pub fn tr(source: &str) -> String {
    translation(source).unwrap_or_else(|| source.to_string())
}

fn translation(source: &str) -> Option<String> {
    let catalog = match current_language() {
        AppLanguage::Greek => Some(&*GREEK),
        AppLanguage::Italian => Some(&*ITALIAN),
        AppLanguage::Spanish => Some(&*SPANISH),
        AppLanguage::German => Some(&*GERMAN),
        AppLanguage::French => Some(&*FRENCH),
        AppLanguage::Automatic | AppLanguage::English => None,
    };

    catalog
        .and_then(|translations| translations.get(source))
        .filter(|translation| !translation.is_empty())
        .cloned()
}

pub fn tr_args(source: &str, replacements: &[(&str, &str)]) -> String {
    replacements
        .iter()
        .fold(tr(source), |text, (placeholder, value)| {
            text.replace(placeholder, value)
        })
}

/// Translate the text-bearing widgets in an already constructed widget tree.
///
/// Keeping English strings in the Rust UI code makes English the reliable
/// fallback, while embedded PO catalogs provide the selected translation.
pub fn translate_widget_tree(root: &impl IsA<gtk4::Widget>) {
    translate_widget(root.as_ref());
}

fn translate_widget(widget: &gtk4::Widget) {
    if let Some(group) = widget.downcast_ref::<adw::PreferencesGroup>() {
        if let Some(title) = translation(group.title().as_str()) {
            group.set_title(&title);
        }
        if let Some(description) = group.description() {
            if let Some(description) = translation(description.as_str()) {
                group.set_description(Some(&description));
            }
        }
    }

    if let Some(row) = widget.downcast_ref::<adw::PreferencesRow>() {
        if let Some(title) = translation(row.title().as_str()) {
            row.set_title(&title);
        }
    }

    if let Some(row) = widget.downcast_ref::<adw::ActionRow>() {
        if let Some(subtitle) = row.subtitle() {
            if let Some(subtitle) = translation(subtitle.as_str()) {
                row.set_subtitle(&subtitle);
            }
        }
    }

    if let Some(title) = widget.downcast_ref::<adw::WindowTitle>() {
        if let Some(translated) = translation(title.title().as_str()) {
            title.set_title(&translated);
        }
        if let Some(translated) = translation(title.subtitle().as_str()) {
            title.set_subtitle(&translated);
        }
    }

    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        if let Some(translated) = translation(label.text().as_str()) {
            if label.uses_markup() {
                label.set_markup(&translated);
            } else {
                label.set_text(&translated);
            }
        }
    }

    if let Some(tooltip) = widget.tooltip_text() {
        if let Some(tooltip) = translation(tooltip.as_str()) {
            widget.set_tooltip_text(Some(&tooltip));
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        translate_widget(&current);
    }
}

fn automatic_language() -> AppLanguage {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find_map(|locale| language_from_locale(&locale))
        .unwrap_or(AppLanguage::English)
}

fn language_from_locale(locale: &str) -> Option<AppLanguage> {
    let code = locale
        .split(':')
        .next()
        .unwrap_or(locale)
        .split(['_', '.', '-'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();

    match code.as_str() {
        "en" | "c" | "posix" => Some(AppLanguage::English),
        "el" => Some(AppLanguage::Greek),
        "it" => Some(AppLanguage::Italian),
        "es" => Some(AppLanguage::Spanish),
        "de" => Some(AppLanguage::German),
        "fr" => Some(AppLanguage::French),
        _ => None,
    }
}

fn parse_po(source: &str) -> HashMap<String, String> {
    #[derive(Clone, Copy)]
    enum Field {
        None,
        Id,
        Translation,
    }

    fn quoted(value: &str) -> String {
        serde_json::from_str::<String>(value.trim()).unwrap_or_default()
    }

    fn save(catalog: &mut HashMap<String, String>, id: &mut String, translation: &mut String) {
        if !id.is_empty() && !translation.is_empty() {
            catalog.insert(std::mem::take(id), std::mem::take(translation));
        } else {
            id.clear();
            translation.clear();
        }
    }

    let mut catalog = HashMap::new();
    let mut id = String::new();
    let mut translation = String::new();
    let mut field = Field::None;

    for line in source.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("msgid ") {
            save(&mut catalog, &mut id, &mut translation);
            id = quoted(value);
            field = Field::Id;
        } else if let Some(value) = line.strip_prefix("msgstr ") {
            translation = quoted(value);
            field = Field::Translation;
        } else if line.starts_with('"') {
            match field {
                Field::Id => id.push_str(&quoted(line)),
                Field::Translation => translation.push_str(&quoted(line)),
                Field::None => {}
            }
        }
    }
    save(&mut catalog, &mut id, &mut translation);
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection_accepts_common_linux_formats() {
        assert_eq!(
            language_from_locale("el_GR.UTF-8"),
            Some(AppLanguage::Greek)
        );
        assert_eq!(language_from_locale("de-DE"), Some(AppLanguage::German));
        assert_eq!(language_from_locale("fr:en"), Some(AppLanguage::French));
        assert_eq!(language_from_locale("ja_JP.UTF-8"), None);
    }

    #[test]
    fn po_parser_reads_single_and_multiline_values() {
        let catalog = parse_po("msgid \"Hello\"\nmsgstr \"Bonjour\"\n\nmsgid \"Long \"\n\"text\"\nmsgstr \"Texte \"\n\"long\"\n");
        assert_eq!(catalog.get("Hello").map(String::as_str), Some("Bonjour"));
        assert_eq!(
            catalog.get("Long text").map(String::as_str),
            Some("Texte long")
        );
    }
}
