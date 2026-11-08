use gpui_kit::{App, Global};
use tokio::runtime::{Builder, Handle, Runtime};

/// The Tokio runtime that network requests (`reqwest`) run on.
///
/// gpui's executor isn't Tokio, so futures from the `qbittorrent` crate are
/// spawned here and their handles awaited from gpui tasks.
pub struct Tokio(Runtime);

impl Global for Tokio {}

impl Tokio {
    /// Start the runtime and store it as a global.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime could not be started.
    pub fn init(cx: &mut App) -> std::io::Result<()> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;
        cx.set_global(Self(runtime));
        Ok(())
    }

    /// A handle to the runtime, if [`Tokio::init`] succeeded.
    pub fn handle(cx: &App) -> Option<Handle> {
        cx.try_global::<Self>().map(|tokio| tokio.0.handle().clone())
    }
}
