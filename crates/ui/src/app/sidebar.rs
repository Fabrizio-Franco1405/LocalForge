//! Sidebar: brand, the four workspaces and a minimal footer.
//!
//! The detailed resource monitor lives in the global footer so it is visible
//! from every workspace.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _};
use gpui_kit::prelude::{FluentBuilder as _, StatefulInteractiveElement as _};
use gpui_kit::*;

use crate::app::{VoxApp, WorkspaceMode};
use crate::config;

const WORKSPACES: [WorkspaceMode; 4] = [
    WorkspaceMode::Chat,
    WorkspaceMode::Call,
    WorkspaceMode::Studio3d,
    WorkspaceMode::Settings,
];

impl VoxApp {
    /// Fixed left navigation.
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let brand = self.brand(cx);

        let mut items = Vec::with_capacity(WORKSPACES.len());
        for workspace in WORKSPACES {
            items.push(self.nav_item(workspace, cx));
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
                            .child("WORKSPACES"),
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
                this.workspace = WorkspaceMode::Chat;
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
                    .child("entorno local · LLM · voz · 3D"),
            )
    }

    fn nav_item(
        &self,
        workspace: WorkspaceMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let active = self.workspace == workspace;
        let id = workspace.id();
        let accent = match workspace {
            WorkspaceMode::Chat => theme.primary,
            WorkspaceMode::Call => theme.success,
            WorkspaceMode::Studio3d => theme.warning,
            WorkspaceMode::Settings => theme.muted_foreground,
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
                if this.workspace != workspace {
                    this.workspace = workspace;
                    cx.notify();
                }
            }))
            .child(
                Icon::new(workspace_icon(workspace))
                    .small()
                    .text_color(if active {
                        accent
                    } else {
                        theme.muted_foreground
                    }),
            )
            .child(div().text_sm().child(workspace.label()))
    }

    fn sidebar_footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .p_3()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{} · v{}",
                        match config::current(cx).llm.mode {
                            shared::LlmMode::Managed => "local",
                            shared::LlmMode::External => "externo",
                        },
                        env!("CARGO_PKG_VERSION")
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Icon::new(IconName::ShieldCheck)
                            .small()
                            .text_color(theme.success),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("offline"),
                    ),
            )
    }
}

fn workspace_icon(workspace: WorkspaceMode) -> IconName {
    match workspace {
        WorkspaceMode::Chat => IconName::MessageSquare,
        WorkspaceMode::Call => IconName::Phone,
        WorkspaceMode::Studio3d => IconName::Cuboid,
        WorkspaceMode::Settings => IconName::Settings,
    }
}
