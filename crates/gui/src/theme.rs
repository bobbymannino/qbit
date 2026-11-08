use gpui_kit::{
    App, Result,
    component::{Theme, ThemeRegistry},
};

/// The Catppuccin theme set (Latte and Mocha).
const CATPPUCCIN: &str = include_str!("../themes/catppuccin.json");

/// The theme used when the system appearance is light.
const LIGHT_THEME: &str = "Catppuccin Latte";

/// The theme used when the system appearance is dark.
const DARK_THEME: &str = "Catppuccin Mocha";

/// Register the Catppuccin themes and apply the one matching the system appearance.
///
/// Must be called after [`gpui_kit::component::init`].
///
/// # Errors
///
/// Returns an error if the bundled theme file cannot be parsed.
pub fn init(cx: &mut App) -> Result<()> {
    ThemeRegistry::global_mut(cx).load_themes_from_str(CATPPUCCIN)?;

    let themes = ThemeRegistry::global(cx).themes();
    let light = themes.get(LIGHT_THEME).cloned();
    let dark = themes.get(DARK_THEME).cloned();

    let theme = Theme::global_mut(cx);
    if let Some(light) = light {
        theme.light_theme = light;
    }
    if let Some(dark) = dark {
        theme.dark_theme = dark;
    }

    Theme::sync_system_appearance(None, cx);
    Ok(())
}
