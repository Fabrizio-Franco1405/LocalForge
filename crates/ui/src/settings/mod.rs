//! Settings pages, one per sidebar section.
//!
//! Each section is a separate module with its own groups, so a change in the
//! audio page cannot accidentally affect the LLM page. `view` builds only the
//! page that is currently visible.

pub mod fields;

mod audio;
mod general;
mod llm;
mod mcp;
mod scene3d;

use gpui_kit::component::setting::Settings;
use gpui_kit::*;

use crate::app::SettingsTab;
use crate::config;

/// Builds the settings panel for one section.
pub fn view(cx: &App, tab: SettingsTab) -> impl IntoElement {
    let cfg = config::current(cx);
    let models = config::models(cx);

    let (id, pages) = match tab {
        SettingsTab::Llm => (
            "localforge-settings-llm",
            vec![llm::page(cfg.llm.mode, &cfg, &models)],
        ),
        SettingsTab::Audio => ("localforge-settings-audio", vec![audio::page()]),
        SettingsTab::Scene3d => ("localforge-settings-scene3d", vec![scene3d::page()]),
        SettingsTab::General | SettingsTab::System => {
            ("localforge-settings-general", vec![general::page()])
        }
        SettingsTab::Mcp => ("localforge-settings-mcp", vec![mcp::page()]),
    };

    div().id("settings-page").size_full().p_5().child(
        div().h_full().w_full().max_w(px(960.)).child(
            Settings::new(id)
                .sidebar_width(px(1.))
                .sidebar_size_range(px(1.)..px(1.))
                .pages(pages),
        ),
    )
}
