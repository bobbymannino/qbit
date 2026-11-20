//! The dialog for adding torrents from magnet links.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui_kit::{
    App, AppContext as _, Entity, SharedString, Window,
    base::v_flex,
    component::{
        ActiveTheme as _, WindowExt as _,
        input::{Input, InputState},
    },
    div,
    prelude::*,
};

/// What every magnet link starts with.
const MAGNET_PREFIX: &str = "magnet:?";

/// The magnet links in `text`, separated by whitespace, or why there aren't any.
///
/// # Errors
///
/// Returns a message for the user if `text` is empty or has anything that isn't a
/// magnet link in it.
pub fn magnet_links(text: &str) -> Result<Vec<String>, &'static str> {
    let links: Vec<String> = text.split_whitespace().map(ToOwned::to_owned).collect();
    if links.is_empty() {
        return Err("Paste a magnet link to add.");
    }
    if !links.iter().all(|link| is_magnet_link(link)) {
        return Err("That isn't a magnet link. Magnet links start with \u{201c}magnet:?\u{201d}.");
    }
    Ok(links)
}

fn is_magnet_link(text: &str) -> bool {
    text.get(..MAGNET_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(MAGNET_PREFIX))
}

/// Ask for magnet links, then hand them to `add` once the user confirms.
///
/// The box starts filled in with the clipboard, if that holds magnet links.
pub fn open(window: &mut Window, cx: &mut App, add: impl Fn(Vec<String>, &mut App) + 'static) {
    let clipboard = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .filter(|text| magnet_links(text).is_ok());
    let input: Entity<InputState> = cx.new(|cx| {
        let input = InputState::new(window, cx).placeholder("magnet:?xt=urn:btih:\u{2026}");
        match clipboard {
            Some(text) => input.default_value(text.trim().to_owned()),
            None => input,
        }
    });
    // Why the last attempt to add was refused, shown under the box.
    let error: Rc<RefCell<Option<SharedString>>> = Rc::default();
    let add = Rc::new(add);
    let focus_once = Cell::new(true);

    window.open_alert_dialog(cx, move |alert, window, cx| {
        if focus_once.replace(false) {
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
        }

        let input_for_ok = input.clone();
        let error_for_ok = Rc::clone(&error);
        let add = Rc::clone(&add);
        let theme = cx.theme();

        alert
            .confirm()
            .title("Add magnet link")
            .child(
                // The dialog body clips what's outside it, so leave room for the input's focus ring.
                v_flex().w_full().p_1().gap_1().child(Input::new(&input)).children(
                    error
                        .borrow()
                        .clone()
                        .map(|message| div().text_xs().text_color(theme.danger).child(message)),
                ),
            )
            .ok_text("Add")
            .on_ok(
                move |_, window, cx| match magnet_links(&input_for_ok.read(cx).value()) {
                    Ok(links) => {
                        add(links, cx);
                        true
                    }
                    Err(message) => {
                        *error_for_ok.borrow_mut() = Some(message.into());
                        window.refresh();
                        false
                    }
                },
            )
    });
}

#[cfg(test)]
mod tests {
    use super::magnet_links;

    #[test]
    fn finds_magnet_links_on_their_own_or_several_at_once() {
        assert_eq!(magnet_links("  magnet:?xt=a \n"), Ok(vec!["magnet:?xt=a".to_owned()]));
        assert_eq!(
            magnet_links("magnet:?xt=a MAGNET:?xt=b"),
            Ok(vec!["magnet:?xt=a".to_owned(), "MAGNET:?xt=b".to_owned()])
        );
    }

    #[test]
    fn refuses_anything_else() {
        assert!(magnet_links("").is_err());
        assert!(magnet_links("   ").is_err());
        assert!(magnet_links("https://example.com/a.torrent").is_err());
        assert!(magnet_links("magnet:?xt=a not-a-link").is_err());
        assert!(magnet_links("magnet").is_err());
    }
}
