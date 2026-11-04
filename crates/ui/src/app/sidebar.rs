//! Sidebar: brand, the four sections and the engine status footer.
//!
//! The conversation is the home view: clicking the brand returns to it. The
//! four sections are the isolated configuration areas (LLM, audio, 3D,
//! general); diagnostics is a utility, not a section.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _};
use gpui_kit::prelude::{FluentBuilder as _, StatefulInteractiveElement as _};
use gpui_kit::*;
use shared::text::truncate_chars;
use shared::{LlmMode, LlmState};

use crate::app::{Page, VoxApp};
use crate::config;

const SECTIONS: [Page; 4] = [Page::Llm, Page::Audio, Page::Scene3d, Page::General];

impl VoxApp {
    /// Fixed left navigation.
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let brand = self.brand(cx);

        let mut items = Vec::with_capacity(SECTIONS.len());
        for page in SECTIONS {
            items.push(self.nav_item(page, cx));
        }

        let footer = self.sidebar_footer(cx);
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(brand)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_3()
                    .pb_3()
                    .child(
                        div()
                            .px_2()
                            .pb_1()
                            .text_xs()
                            .font_semibold()
                            .text_color(theme.muted_foreground)
                            .child("MOTORES"),
                    )
                    .children(items),
            )
            .child(div().flex_1())
            .child(footer)
    }

    fn brand(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();

        div()
            .id("brand-home")
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_4()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.page = Page::Chat;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(26.))
                            .h(px(26.))
                            .rounded_md()
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .text_xs()
                            .font_semibold()
                            .child(Icon::new(IconName::Sparkles).small()),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("LocalForge"),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("harness local · LLM · voz · 3D"),
            )
    }

    fn nav_item(&self, page: Page, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let active = self.page == page;
        let id = page.id();
        let accent = match page {
            Page::Audio => theme.success,
            Page::Scene3d => theme.warning,
            Page::Llm => theme.primary,
            _ => theme.muted_foreground,
        };

        div()
            .id(format!("nav-{id}"))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .when(active, |this| this.bg(accent.opacity(0.14)))
            .text_color(if active { accent } else { theme.foreground })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.page != page {
                    this.page = page;
                    cx.notify();
                }
            }))
            .child(Icon::new(section_icon(page)).small().text_color(if active {
                accent
            } else {
                theme.muted_foreground
            }))
            .child(div().text_sm().child(page.label()))
    }

    fn sidebar_footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let color = match self.llm {
            LlmState::Stopped => theme.muted_foreground,
            LlmState::Starting => theme.warning,
            LlmState::Ready => theme.success,
            LlmState::Error => theme.danger,
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .mt(px(4.))
                            .w(px(8.))
                            .h(px(8.))
                            .rounded_full()
                            .bg(color),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(format!("LLM · {}", self.llm.label())),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(truncate_chars(&self.llm_detail, 40)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(resource_line(
                        IconName::Cpu,
                        match self.cpu {
                            Some(value) => format!("CPU {value:.0}%"),
                            None => "CPU —".to_owned(),
                        },
                        cx,
                    ))
                    .child(resource_line(
                        IconName::MemoryStick,
                        match (self.ram_used_mb, self.ram_total_mb) {
                            (Some(used), Some(total)) => format!(
                                "RAM {:.1}/{:.1} GB",
                                used as f32 / 1024.0,
                                total as f32 / 1024.0
                            ),
                            _ => "RAM —".to_owned(),
                        },
                        cx,
                    ))
                    .child(resource_line(
                        IconName::Gauge,
                        match self.app_mb {
                            Some(value) => format!("App {value} MB"),
                            None => "App —".to_owned(),
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        Button::new("sidebar-diagnostics")
                            .small()
                            .ghost()
                            .label("Diagnóstico")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page = Page::Tools;
                                this.probe(cx);
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} · v{}",
                                match config::current(cx).llm.mode {
                                    LlmMode::Managed => "local",
                                    LlmMode::External => "externo",
                                },
                                env!("CARGO_PKG_VERSION")
                            )),
                    ),
            )
    }
}

fn section_icon(page: Page) -> IconName {
    match page {
        Page::Chat => IconName::MessageSquare,
        Page::Llm => IconName::Bot,
        Page::Audio => IconName::Mic,
        Page::Scene3d => IconName::Cuboid,
        Page::General => IconName::Settings,
        Page::Tools => IconName::Activity,
    }
}

fn resource_line(icon: IconName, text: String, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(Icon::new(icon).small().text_color(theme.muted_foreground))
        .child(div().text_xs().text_color(theme.foreground).child(text))
}
