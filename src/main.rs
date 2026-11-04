//! LocalForge desktop shell.
//!
//! Loads the user's configuration, starts the controller thread and opens the
//! single GPUI window. All engine work lives behind `EngineHandle`; this file
//! only composes the window and applies the saved theme.

use gpui_kit::*;
use shared::VoxConfig;
use tracing_subscriber::EnvFilter;
use ui::config;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let config_path = VoxConfig::default_path();
    let config = match VoxConfig::load_default() {
        Ok(config) => config,
        Err(error) => {
            eprintln!(
                "No se pudo leer {}: {error}. Se usarán los valores por defecto.",
                config_path.display()
            );
            VoxConfig::default()
        }
    };

    let engine = controller::spawn(config.clone());

    application().with_assets(assets::AllAssets).run(move |cx| {
        init(cx);
        config::init(cx, config.clone(), config_path.clone());
        ui::theme::apply(cx, &config.ui, None);

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("LocalForge".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(size(px(1120.), px(760.)), cx)),
            window_min_size: Some(size(px(880.), px(560.))),
            ..Default::default()
        };

        let engine = engine.clone();
        let opened = open_window(options, cx, move |window, cx| {
            cx.new(|cx| ui::VoxApp::new(window, cx, engine))
        });

        if let Err(error) = opened {
            eprintln!("No se pudo abrir la ventana de LocalForge: {error}");
            cx.quit();
        }
    });
}
