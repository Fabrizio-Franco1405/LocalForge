//! MCP connections (placeholder).
//!
//! The tab exists so the configuration slot is reserved. A real client
//! (JSON-RPC over SSE/HTTP, tool discovery and invocation) is future work; the
//! UI is explicit about that instead of pretending to connect.

use gpui_kit::component::setting::{SettingGroup, SettingItem, SettingPage};

use super::fields::*;

/// Builds the MCP section.
pub fn page() -> SettingPage {
    SettingPage::new("MCP")
        .description(
            "Servidores MCP (Model Context Protocol) que ampliarán las herramientas del agente. \
             El cliente aún no está implementado.",
        )
        .default_open(true)
        .group(
            SettingGroup::new()
                .title("Conexión")
                .description("Se conectará al arrancar cuando el cliente esté listo.")
                .item(
                    SettingItem::new(
                        "Activar MCP",
                        bool_field(|cfg| cfg.mcp.enabled, |cfg, value| cfg.mcp.enabled = value),
                    )
                    .description("Reservado: sin efecto hasta que exista el cliente.")
                    .disabled(true),
                )
                .item(
                    SettingItem::new(
                        "Endpoint MCP",
                        text_field(
                            |cfg| cfg.mcp.endpoint.clone(),
                            |cfg, value| cfg.mcp.endpoint = value.trim().to_owned(),
                        ),
                    )
                    .description("Ej: https://mcp.example.com/sse (SSE/HTTP).")
                    .disabled(true),
                ),
        )
}
