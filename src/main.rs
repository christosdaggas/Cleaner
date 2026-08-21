mod application;
mod autostart;
mod i18n;
mod storage;
mod models;
mod release_notes;
mod services;
mod theme;
mod tray;
mod ui;
pub mod version_check;

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static TOKIO_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("Fatal: Failed to create Tokio runtime: {}", e);
            std::process::exit(1);
        })
});

pub fn runtime() -> &'static Runtime {
    &TOKIO_RUNTIME
}

use application::DataCleanerApplication;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub const APP_ID: &str = "com.chrisdaggas.datacleaner";
pub const APP_NAME: &str = "Data Cleaner";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DISPLAY_VERSION: &str = VERSION;

fn main() -> glib::ExitCode {
    glib::set_prgname(Some(APP_ID));
    glib::set_application_name(APP_NAME);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting {} v{}", APP_NAME, VERSION);

    if let Err(e) = libadwaita::init() {
        tracing::error!("Failed to initialize libadwaita: {}", e);
        return glib::ExitCode::FAILURE;
    }

    if let Some(display) = gtk4::gdk::Display::default() {
        let icon_theme = gtk4::IconTheme::for_display(&display);
        let icon_search_paths = [
            std::path::PathBuf::from("data/icons"),
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/icons"),
            std::path::PathBuf::from("/usr/share/icons"),
            std::path::PathBuf::from("/usr/local/share/icons"),
        ];

        for path in &icon_search_paths {
            if path.exists() {
                icon_theme.add_search_path(path);
            }
        }
    }

    let resource_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/data-cleaner.gresource"));
    let resource_data = glib::Bytes::from_static(resource_bytes);
    if let Ok(resource) = gio::Resource::from_data(&resource_data) {
        gio::resources_register(&resource);
    }

    let mut args: Vec<String> = std::env::args().collect();
    let start_hidden = args.iter().any(|argument| argument == "--background");
    args.retain(|argument| argument != "--background");

    let app = DataCleanerApplication::new();
    app.set_start_hidden(start_hidden);
    app.run_with_args(&args)
}
