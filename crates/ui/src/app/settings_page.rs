//! Ajustes workspace: tab bar plus the per-engine settings pages.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::*;

use crate::app::{SettingsTab, VoxApp};
use crate::settings;
use gpui_kit::assets::IconName;

impl VoxApp {
    pub(super) fn settings_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut tabs = Vec::with_capacity(SettingsTab::ALL.len());
        for tab in SettingsTab::ALL {
            tabs.push(self.tab_button(tab, cx));
        }

        let content = match self.settings_tab {
            SettingsTab::System => self.system_page(cx).into_any_element(),
            tab => settings::view(cx, tab).into_any_element(),
        };
        let theme = cx.theme().clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_5()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .children(tabs),
            )
            .child(div().flex_1().min_h(px(0.)).child(content))
    }

    fn tab_button(&self, tab: SettingsTab, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let button = Button::new(tab.id())
            .small()
            .icon(tab_icon(tab))
            .label(tab.label());
        let button = if self.settings_tab == tab {
            button.primary()
        } else {
            button.ghost()
        };

        button.on_click(cx.listener(move |this, _, _, cx| {
            this.settings_tab = tab;
            if tab == SettingsTab::System {
                this.ensure_hardware(cx);
            }
            cx.notify();
        }))
    }
}

fn tab_icon(tab: SettingsTab) -> IconName {
    match tab {
        SettingsTab::Llm => IconName::Bot,
        SettingsTab::Audio => IconName::Mic,
        SettingsTab::Scene3d => IconName::Cuboid,
        SettingsTab::General => IconName::SlidersHorizontal,
        SettingsTab::System => IconName::CircuitBoard,
        SettingsTab::Mcp => IconName::Plug,
    }
}
