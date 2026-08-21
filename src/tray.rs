//! StatusNotifierItem system tray integration.
//!
//! KDE supports this natively. GNOME requires the AppIndicator and
//! KStatusNotifierItem Support extension.
//!
//! The icon travels to the host as an `IconPixmap` (raw ARGB32 rasterised from
//! the symbolic SVG embedded in the binary) rather than as a bare `IconName`.
//! Name-based lookup is unreliable: hosts implement the search differently and
//! most only probe `<theme>/<size>/{apps,status,panel}/`, so an icon living in
//! `<theme>/symbolic/apps/` is never found and the tray shows an empty slot.
//! Sending the pixels makes the icon independent of the host's search rules, of
//! icon caches, and of whether the app is installed at all. `icon_name` and
//! `icon_theme_path` remain as a secondary path for hosts that prefer to
//! resolve — and recolour — a themed icon themselves.
//!
//! Because the pixmap is pre-rendered, its colour has to be chosen by us: the
//! SVG paints with `currentColor`, and [`render_tray_icons`] substitutes the
//! light or dark foreground before rasterising. `application.rs` re-renders and
//! pushes a new set whenever libadwaita reports a colour-scheme change, so the
//! tray glyph stays legible on both light and dark panels.

use ksni::menu::{MenuItem, StandardItem};
use ksni::{Tray, TrayMethods};

/// Monochrome tray artwork. The symbolic SVG is the single source of truth —
/// editing it changes what the tray draws, with no export step to forget.
const TRAY_SVG: &str =
    include_str!("../data/icons/hicolor/symbolic/apps/com.chrisdaggas.datacleaner-symbolic.svg");

/// Sizes offered to the host so it can pick the one matching its panel instead
/// of rescaling a single bitmap.
const TRAY_SIZES: [i32; 6] = [16, 22, 24, 32, 48, 64];

/// The placeholder colour written into the symbolic SVG; every occurrence is
/// swapped for the active theme's foreground before rasterising.
const SVG_PLACEHOLDER_FG: &str = "#ffffff";

/// Foreground for dark panels (the SVG ships in this state).
const DARK_PANEL_FG: &str = "#ffffff";

/// Foreground for light panels — Adwaita's dark text colour.
const LIGHT_PANEL_FG: &str = "#2e3436";

#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    Open,
    Clean,
    Close,
}

#[derive(Debug, Clone, Copy)]
pub enum TrayStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone)]
pub enum TrayCmd {
    Enable,
    Disable,
    /// Replace the rasterised artwork, e.g. after a light/dark switch.
    SetIcons(Vec<ksni::Icon>),
}

struct DataCleanerTray {
    tx: async_channel::Sender<TrayAction>,
    status_tx: async_channel::Sender<TrayStatus>,
    icon_name: String,
    /// Rasterised on the main thread; `ksni::Icon` is plain data, so it moves
    /// to the tray's thread with the struct.
    icons: Vec<ksni::Icon>,
}

impl DataCleanerTray {
    fn emit(&self, action: TrayAction) {
        let _ = self.tx.try_send(action);
    }
}

/// The symbolic SVG with its `currentColor` source recoloured for the panel.
fn themed_svg(dark: bool) -> String {
    let foreground = if dark { DARK_PANEL_FG } else { LIGHT_PANEL_FG };
    TRAY_SVG.replace(SVG_PLACEHOLDER_FG, foreground)
}

/// Rasterise the symbolic icon at `size`×`size` into the `IconPixmap` wire
/// format: ARGB32, network byte order, rows packed tightly (no rowstride
/// padding, which gdk-pixbuf may add).
///
/// Rendering goes through gdk-pixbuf's SVG loader (librsvg). Without it the
/// icon list comes back empty and hosts fall back to `icon_name`.
fn render_icon(svg: &[u8], size: i32) -> Option<ksni::Icon> {
    use gtk4::gdk_pixbuf::PixbufLoader;
    use gtk4::prelude::PixbufLoaderExt;

    let loader = PixbufLoader::with_type("svg").ok()?;
    // Must be set before the data is written: it scales the SVG while parsing
    // instead of resampling a bitmap afterwards, so every size stays crisp.
    loader.set_size(size, size);
    loader.write(svg).ok()?;
    loader.close().ok()?;

    let pixbuf = loader.pixbuf()?;
    let pixbuf = if pixbuf.has_alpha() {
        pixbuf
    } else {
        pixbuf.add_alpha(false, 0, 0, 0).ok()?
    };

    let (width, height) = (pixbuf.width() as usize, pixbuf.height() as usize);
    let channels = pixbuf.n_channels() as usize;
    let rowstride = pixbuf.rowstride() as usize;
    // SAFETY: the pixbuf is owned by this function and is neither mutated nor
    // shared while the slice is alive.
    let pixels = unsafe { pixbuf.pixels() };

    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = &pixels[y * rowstride..y * rowstride + width * channels];
        for px in row.chunks_exact(channels) {
            // RGBA -> ARGB
            data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
        }
    }

    Some(ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    })
}

/// Rasterise the tray artwork for the current colour scheme, at every size a
/// host might ask for. Call this on the GTK main thread — it uses gdk-pixbuf.
pub fn render_tray_icons(dark: bool) -> Vec<ksni::Icon> {
    let svg = themed_svg(dark);
    TRAY_SIZES
        .iter()
        .filter_map(|&size| render_icon(svg.as_bytes(), size))
        .collect()
}

impl Tray for DataCleanerTray {
    fn id(&self) -> String {
        crate::APP_ID.to_string()
    }

    fn title(&self) -> String {
        crate::APP_NAME.to_string()
    }

    fn icon_name(&self) -> String {
        self.icon_name.clone()
    }

    /// Only a hint for hosts that resolve `icon_name` themselves — the pixmap
    /// is what actually gets drawn. Returns the first theme root that really
    /// holds our icon, so we never advertise a stale or dev-only path.
    fn icon_theme_path(&self) -> String {
        // A *base* directory — the one that contains `hicolor/`, not `hicolor`
        // itself. `Gtk.IconTheme.append_search_path` and Qt's
        // `themeSearchPaths` both expect a base; given a theme directory they
        // read `symbolic/` and `scalable/` as if those were theme names, and
        // the lookup silently finds nothing.
        let bases = [
            "/usr/share/icons".to_string(),
            "/usr/local/share/icons".to_string(),
            format!("{}/icons", glib::user_data_dir().to_string_lossy()),
            format!("{}/data/icons", env!("CARGO_MANIFEST_DIR")),
        ];

        let icon = format!("{}-symbolic.svg", crate::APP_ID);
        bases
            .iter()
            .find(|base| {
                ["hicolor/symbolic/apps", "hicolor/scalable/apps"]
                    .iter()
                    .any(|dir| std::path::Path::new(base).join(dir).join(&icon).is_file())
            })
            .cloned()
            .unwrap_or_default()
    }

    /// The authoritative icon: raw pixels, so no host-side theme lookup is
    /// involved. Empty only if rasterising fails, in which case hosts fall back
    /// to `icon_name`.
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.emit(TrayAction::Open);
    }

    fn watcher_online(&self) {
        let _ = self.status_tx.try_send(TrayStatus::Available);
    }

    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        let _ = self.status_tx.try_send(TrayStatus::Unavailable);
        true
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: crate::i18n::tr("Open Data Cleaner"),
                activate: Box::new(|tray: &mut Self| tray.emit(TrayAction::Open)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: crate::i18n::tr("Clean"),
                activate: Box::new(|tray: &mut Self| tray.emit(TrayAction::Clean)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: crate::i18n::tr("Close"),
                activate: Box::new(|tray: &mut Self| tray.emit(TrayAction::Close)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn start_tray_service(
    initial_icons: Vec<ksni::Icon>,
) -> (
    async_channel::Sender<TrayCmd>,
    async_channel::Receiver<TrayAction>,
    async_channel::Receiver<TrayStatus>,
) {
    let (action_tx, action_rx) = async_channel::unbounded::<TrayAction>();
    let (status_tx, status_rx) = async_channel::unbounded::<TrayStatus>();
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<TrayCmd>();

    crate::runtime().spawn(async move {
        let mut handle: Option<ksni::Handle<DataCleanerTray>> = None;
        // Kept so a tray enabled *after* a theme change still starts with the
        // right artwork.
        let mut icons = initial_icons;

        while let Ok(command) = cmd_rx.recv().await {
            match command {
                TrayCmd::Enable if handle.is_none() => {
                    let tray = DataCleanerTray {
                        tx: action_tx.clone(),
                        status_tx: status_tx.clone(),
                        icon_name: tray_icon_name(),
                        icons: icons.clone(),
                    };
                    let sandboxed = std::path::Path::new("/.flatpak-info").exists();
                    match tray.disable_dbus_name(sandboxed).spawn().await {
                        Ok(new_handle) => {
                            tracing::info!("System tray registered");
                            let _ = status_tx.try_send(TrayStatus::Available);
                            handle = Some(new_handle);
                        }
                        Err(error) => {
                            tracing::warn!("System tray unavailable (no SNI host?): {error}");
                            let _ = status_tx.try_send(TrayStatus::Unavailable);
                        }
                    }
                }
                TrayCmd::Disable => {
                    if let Some(active_handle) = handle.take() {
                        active_handle.shutdown().await;
                        tracing::info!("System tray removed");
                    }
                    let _ = status_tx.try_send(TrayStatus::Unavailable);
                }
                TrayCmd::SetIcons(new_icons) => {
                    // An empty set means rasterising failed; keeping the old
                    // artwork beats blanking the tray slot.
                    if new_icons.is_empty() {
                        continue;
                    }
                    icons = new_icons;
                    if let Some(active_handle) = handle.as_ref() {
                        let next = icons.clone();
                        active_handle
                            .update(move |tray: &mut DataCleanerTray| tray.icons = next)
                            .await;
                    }
                }
                TrayCmd::Enable => {}
            }
        }

        if let Some(active_handle) = handle.take() {
            active_handle.shutdown().await;
        }
    });

    (cmd_tx, action_rx, status_rx)
}

fn tray_icon_name() -> String {
    // StatusNotifier hosts only apply the panel foreground color when they
    // resolve a symbolic icon through the current icon theme. Passing an
    // absolute SVG path makes GNOME AppIndicator treat it as an already
    // rendered image, leaving its source fill untouched on dark panels.
    // The icon is installed into hicolor/symbolic/apps by every package.
    format!("{}-symbolic", crate::APP_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tray(icons: Vec<ksni::Icon>) -> DataCleanerTray {
        let (tx, _rx) = async_channel::unbounded();
        let (status_tx, _status_rx) = async_channel::unbounded();
        DataCleanerTray {
            tx,
            status_tx,
            icon_name: tray_icon_name(),
            icons,
        }
    }

    #[test]
    fn symbolic_source_paints_with_current_color_only() {
        // A baked-in fill would survive both GTK's symbolic restyling and the
        // substitution in `themed_svg`, leaving an invisible icon on one of the
        // two panel colours.
        assert!(TRAY_SVG.contains("currentColor"));
        assert!(TRAY_SVG.contains("ColorScheme-Text"));
        assert!(
            !TRAY_SVG.contains("#f5c211") && !TRAY_SVG.contains("#9e054f"),
            "symbolic icon must not carry the logo's brand colours"
        );
        assert!(
            !TRAY_SVG.contains("stroke=\""),
            "GTK does not restyle stroke on plain paths, so strokes must not be used"
        );
    }

    /// Image loaders sniff the format from the head of the file. GNOME's glycin
    /// SVG loader rejects the icon as "Unknown image format: application/xml"
    /// once a long comment header pushes `<svg` out of that window, and the
    /// only visible symptom is an empty tray slot — `IconPixmap` comes back
    /// with zero entries and nothing is logged.
    #[test]
    fn svg_root_element_stays_within_the_format_sniffing_window() {
        let offset = TRAY_SVG.find("<svg").expect("symbolic icon has a root element");
        assert!(
            offset < 256,
            "`<svg` starts at byte {offset}; keep comments inside the root element"
        );
    }

    #[test]
    fn theme_substitution_swaps_every_placeholder() {
        let light = themed_svg(false);
        assert!(light.contains(LIGHT_PANEL_FG));
        assert!(
            !light.contains(SVG_PLACEHOLDER_FG),
            "a leftover white source colour would stay invisible on a light panel"
        );

        let dark = themed_svg(true);
        assert!(dark.contains(DARK_PANEL_FG));
        assert_eq!(dark, TRAY_SVG, "the SVG already ships in its dark state");
    }

    /// The colour the glyph is actually painted with: the first pixel whose
    /// alpha is fully opaque, returned as (r, g, b) from the ARGB32 buffer.
    fn first_opaque_pixel(icon: &ksni::Icon) -> Option<(u8, u8, u8)> {
        icon.data
            .chunks_exact(4)
            .find(|px| px[0] == 0xff)
            .map(|px| (px[1], px[2], px[3]))
    }

    #[test]
    fn rasterised_tray_icon_follows_the_colour_scheme() {
        let dark = render_tray_icons(true);
        let light = render_tray_icons(false);

        // No SVG loader (librsvg) on this machine: the tray falls back to
        // `icon_name` and there is nothing to assert about pixels.
        if dark.is_empty() || light.is_empty() {
            eprintln!("skipped: gdk-pixbuf has no SVG loader");
            return;
        }

        assert_eq!(dark.len(), TRAY_SIZES.len());
        assert_eq!(light.len(), TRAY_SIZES.len());
        for (icon, &size) in dark.iter().zip(TRAY_SIZES.iter()) {
            assert_eq!((icon.width, icon.height), (size, size));
            assert_eq!(icon.data.len(), (size * size * 4) as usize);
        }

        let on_dark = first_opaque_pixel(&dark[0]).expect("dark icon draws something");
        let on_light = first_opaque_pixel(&light[0]).expect("light icon draws something");

        assert_eq!(on_dark, (0xff, 0xff, 0xff), "dark panels get a white glyph");
        assert_eq!(on_light, (0x2e, 0x34, 0x36), "light panels get a dark glyph");
        assert_ne!(
            on_dark, on_light,
            "the tray pixmap must differ between colour schemes"
        );
    }

    #[test]
    fn tray_uses_symbolic_icon_and_text_only_menu_actions() {
        let tray = test_tray(Vec::new());

        assert_eq!(tray.icon_name(), "com.chrisdaggas.datacleaner-symbolic");
        assert!(!tray.icon_name().contains('/'));

        let menu = tray.menu();
        assert_eq!(menu.len(), 4);
        for (index, expected_label) in [(0, "Open Data Cleaner"), (1, "Clean"), (3, "Close")] {
            let MenuItem::Standard(item) = &menu[index] else {
                panic!("expected a standard menu item at index {index}");
            };
            assert_eq!(item.label, expected_label);
            assert!(item.icon_name.is_empty());
        }
        assert!(matches!(menu[2], MenuItem::Separator));
    }
}
