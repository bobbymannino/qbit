use gpui_kit::{App, SharedString};

use crate::{
    credentials::Credentials,
    runtime::Tokio,
    session::login,
    windows::{LoginWindow, MainWindow},
};

/// Open the first window.
///
/// With saved credentials, log in before showing anything, so a logged-in user goes
/// straight to the [`MainWindow`] without the login form flashing up first. Without
/// them, or if that login fails, show the [`LoginWindow`].
pub fn start(cx: &mut App) {
    let saved = match Credentials::load() {
        Ok(Some(saved)) => saved,
        Ok(None) => return show_login(cx, None, None),
        Err(message) => return show_login(cx, None, Some(message.into())),
    };

    let Some(runtime) = Tokio::handle(cx) else {
        return show_login(cx, Some(saved), Some("The async runtime failed to start".into()));
    };

    cx.spawn(async move |cx| {
        let result = match runtime.spawn(login(saved.clone())).await {
            Ok(result) => result,
            Err(err) => Err(format!("Login task failed: {err}")),
        };

        cx.update(|cx| match result {
            Ok(client) => {
                if let Err(err) = MainWindow::open(cx, saved.clone(), client) {
                    let message = format!("Couldn't open the main window: {err}");
                    show_login(cx, Some(saved), Some(message.into()));
                }
            }
            Err(message) => show_login(cx, Some(saved), Some(message.into())),
        });
    })
    .detach();
}

/// Show the login form; quit if even that can't open, since there'd be no window at all.
fn show_login(cx: &mut App, prefill: Option<Credentials>, message: Option<SharedString>) {
    if let Err(err) = LoginWindow::open(cx, prefill, message) {
        eprintln!("Failed to open the login window: {err}");
        cx.quit();
    }
}
