use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString, assets::Assets};

gpui_kit::assets::icon_assets!(ExtraIcons, [LogOut, Trash]);

/// gpui-kit's component icons plus the [`ExtraIcons`] this app uses.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
