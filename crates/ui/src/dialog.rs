//! Diálogos nativos de selección de rutas.
//!
//! Todo selector de archivos de la UI pasa por el prompt de la plataforma que
//! expone GPUI ([`App::prompt_for_paths`]). Está prohibido `rfd` aquí:
//! `rfd::FileDialog::pick_file()` bloquea el hilo que lo llama y, en Windows,
//! corre su propio bucle modal de mensajes. Ese bucle despacha mensajes al
//! procedimiento de ventana de GPUI mientras el `&mut App` del manejador de clic
//! sigue prestado; GPUI aborta con `RefCell already borrowed`
//! (`gpui/src/window.rs:2242`) y el proceso muere con
//! `STATUS_STACK_BUFFER_OVERRUN`. En Linux el fallo no aparece porque el backend
//! de `rfd` (GTK o portal) bloquea en otro hilo.
//!
//! `prompt_for_paths` delega el diálogo a la plataforma (hilo dedicado en
//! Windows, `xdg-desktop-portal` en Linux) y entrega el resultado por un canal
//! asíncrono. Los callbacks de este módulo se ejecutan cuando el manejador de
//! clic ya retornó y no hay ningún préstamo vivo, así que pueden mutar la
//! configuración o una entidad con total seguridad.

use std::path::PathBuf;

use gpui_kit::*;

/// Modo de selección de un diálogo nativo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Un único archivo existente.
    File,
    /// Uno o varios archivos existentes.
    Files,
    /// Una única carpeta.
    Folder,
}

impl Pick {
    /// Traduce el modo de selección a las opciones de GPUI.
    ///
    /// El prompt de GPUI no admite título propio, carpeta inicial ni filtros por
    /// extensión: la plataforma muestra sus cadenas. Es el precio de un diálogo
    /// que no puede reentrar en la app, y los filtros son poco fiables entre
    /// sistemas de todos modos (`llama-server` no lleva extensión en Linux y
    /// lleva `.exe` en Windows).
    pub fn options(self) -> PathPromptOptions {
        PathPromptOptions {
            files: !matches!(self, Self::Folder),
            directories: matches!(self, Self::Folder),
            multiple: matches!(self, Self::Files),
            prompt: None,
        }
    }

    /// Texto del botón que abre el diálogo.
    pub fn button_label(self) -> &'static str {
        match self {
            Self::File => "Elegir archivo…",
            Self::Files => "Elegir archivos…",
            Self::Folder => "Elegir carpeta…",
        }
    }
}

/// Abre el diálogo nativo y entrega las rutas elegidas a `on_pick`.
///
/// `on_pick` corre en el hilo de GPUI, después de que el clic retorne y sin
/// préstamos activos, así que puede usar [`crate::config::update`] o cualquier
/// global. Cancelar el diálogo no cambia nada; un error del portal se registra
/// en el log y la app sigue usable.
pub fn pick_paths(
    cx: &mut App,
    options: PathPromptOptions,
    on_pick: impl FnOnce(&mut App, Vec<PathBuf>) + 'static,
) {
    let receiver = cx.prompt_for_paths(options);

    cx.spawn(async move |cx| {
        let picked = match receiver.await {
            Ok(Ok(paths)) => paths,
            Ok(Err(error)) => {
                // Linux sin xdg-desktop-portal: no hay diálogo disponible.
                tracing::warn!(%error, "no se pudo abrir el selector nativo de archivos");
                None
            }
            // La plataforma descartó la petición: la ventana se está cerrando.
            Err(_) => None,
        };

        if let Some(paths) = picked.filter(|paths| !paths.is_empty()) {
            cx.update(|cx| on_pick(cx, paths));
        }
    })
    .detach();
}

/// Igual que [`pick_paths`], pero para una vista: el callback recibe la entidad.
pub fn pick_paths_for<E, F>(cx: &mut Context<E>, options: PathPromptOptions, on_pick: F)
where
    E: 'static,
    F: FnOnce(&mut E, &mut Context<E>, Vec<PathBuf>) + 'static,
{
    let this = cx.weak_entity();
    pick_paths(cx, options, move |cx, paths| {
        this.update(cx, |entity, cx| on_pick(entity, cx, paths))
            .ok();
    });
}
