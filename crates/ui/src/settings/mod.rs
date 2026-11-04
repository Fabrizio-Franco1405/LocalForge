//! Settings pages, one per sidebar section.
//!
//! Each section is a separate module with its own groups, so a change in the
//! audio page cannot accidentally affect the LLM page. `view` builds only the
//! page that is currently visible.

pub mod fields;

mod audio;
mod general;
mod llm;
mod scene3d;

use gpui_kit::component::setting::Settings;
use gpui_kit::*;

use crate::app::Page;
use crate::config;

/// Builds the settings panel for one section.
pub fn view(cx: &App, page: Page) -> impl IntoElement {
    let mode = config::current(cx).llm.mode;

    let (id, pages) = match page {
        Page::Llm => ("localforge-settings-llm", vec![llm::page(mode)]),
        Page::Audio => ("localforge-settings-audio", vec![audio::page()]),
        Page::Scene3d => ("localforge-settings-scene3d", vec![scene3d::page()]),
        _ => ("localforge-settings-general", vec![general::page()]),
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
