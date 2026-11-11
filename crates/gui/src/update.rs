//! Checking GitHub for a newer release of the app, and installing it in place.
//!
//! On macOS the whole `qBit.app` bundle is swapped for the release's `.app.tar.gz`; on
//! Linux and Windows the running executable is replaced with the one in the release's
//! tarball or zip. Either way the app then restarts itself.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use gpui_kit::{App, Global, SharedString};
use reqwest::{StatusCode, header};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::{consts::APP_VERSION, runtime::Tokio};

/// The newest published (non-prerelease) release.
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/bobbymannino/qbit/releases/latest";

/// The executable inside a Linux or Windows release.
#[cfg(not(target_os = "macos"))]
const BIN_NAME: &str = if cfg!(windows) { "qbit.exe" } else { "qbit" };

/// Where checking for and installing an update is at.
#[derive(Clone, Default)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// Downloading and installing this version.
    Installing(SharedString),
    Failed(SharedString),
}

/// A release newer than this app.
#[derive(Clone)]
pub struct Release {
    /// Its version, without the tag's `v`.
    pub version: SharedString,
    /// Its page on GitHub, with what changed.
    pub url: SharedString,
    /// The file to install on this platform, if the release has one.
    asset: Option<Asset>,
}

/// A release file to download.
#[derive(Clone)]
struct Asset {
    url: String,
    /// The SHA-256 digest GitHub reports for the file, in lowercase hex.
    sha256: Option<String>,
}

/// The update status, shared by every window that shows it.
#[derive(Default)]
pub struct Updater {
    status: Status,
    /// Whether to ask the user about an update the startup check finds; cleared once
    /// they've been asked, or the check found nothing.
    prompt_pending: bool,
}

impl Global for Updater {}

impl Updater {
    /// Where checking for and installing an update is at.
    pub fn status(cx: &App) -> Status {
        cx.try_global::<Self>()
            .map(|updater| updater.status.clone())
            .unwrap_or_default()
    }

    fn set(cx: &mut App, status: Status) {
        // Only an update found by the startup check is prompted for.
        let prompt_pending = matches!(status, Status::Checking | Status::Available(_))
            && cx.try_global::<Self>().is_some_and(|updater| updater.prompt_pending);
        cx.set_global(Self { status, prompt_pending });
    }

    /// Check for an update when the app starts, so the main window can offer to install it.
    ///
    /// Development builds can't install updates, so they don't check.
    pub fn check_on_startup(cx: &mut App) {
        if cfg!(debug_assertions) {
            return;
        }
        cx.set_global(Self {
            status: Self::status(cx),
            prompt_pending: true,
        });
        Self::check(cx);
    }

    /// The update the startup check found, if the user hasn't been asked about it yet.
    ///
    /// Taking it means they won't be asked again.
    pub fn take_startup_prompt(cx: &mut App) -> Option<Release> {
        let updater = cx.try_global::<Self>()?;
        let Status::Available(release) = &updater.status else { return None };
        if !updater.prompt_pending {
            return None;
        }
        let release = release.clone();
        cx.set_global(Self {
            status: Status::Available(release.clone()),
            prompt_pending: false,
        });
        Some(release)
    }

    /// Ask GitHub for the latest release, unless a check or install is already running.
    pub fn check(cx: &mut App) {
        if matches!(Self::status(cx), Status::Checking | Status::Installing(_)) {
            return;
        }
        let Some(runtime) = Tokio::handle(cx) else {
            return Self::set(cx, Status::Failed("The async runtime failed to start".into()));
        };
        Self::set(cx, Status::Checking);

        cx.spawn(async move |cx| {
            let result = match runtime.spawn(fetch_latest()).await {
                Ok(result) => result,
                Err(err) => Err(format!("The update check failed: {err}")),
            };
            cx.update(|cx| {
                let status = match result {
                    Ok(release) if is_newer(&release.version, APP_VERSION) => Status::Available(release),
                    Ok(_) => Status::UpToDate,
                    Err(message) => Status::Failed(message.into()),
                };
                Self::set(cx, status);
            });
        })
        .detach();
    }

    /// Download and install the available update, then restart into it.
    pub fn install(cx: &mut App) {
        let Status::Available(release) = Self::status(cx) else { return };
        if cfg!(debug_assertions) {
            return Self::set(cx, Status::Failed("Development builds can't install updates".into()));
        }
        let Some(asset) = release.asset else {
            let message = format!("v{} has no download for this platform", release.version);
            return Self::set(cx, Status::Failed(message.into()));
        };
        let Some(runtime) = Tokio::handle(cx) else {
            return Self::set(cx, Status::Failed("The async runtime failed to start".into()));
        };
        Self::set(cx, Status::Installing(release.version));

        // The task is detached, and the install itself runs on the Tokio runtime, so closing
        // the settings window can't interrupt it halfway.
        cx.spawn(async move |cx| {
            let result = match runtime.spawn(download_and_install(asset)).await {
                Ok(result) => result,
                Err(err) => Err(format!("The update failed: {err}")),
            };
            cx.update(|cx| match result.and_then(|installed| relaunch(&installed)) {
                Ok(()) => cx.quit(),
                Err(message) => Self::set(cx, Status::Failed(message.into())),
            });
        })
        .detach();
    }
}

/// The parts of GitHub's release JSON that are used.
#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    /// e.g. `sha256:…`; GitHub computes it for every uploaded file.
    digest: Option<String>,
}

/// GitHub's API turns away requests without a user agent.
fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("qBit/{APP_VERSION}"))
        .build()
        .map_err(|err| format!("Couldn't set up the update check: {err}"))
}

async fn fetch_latest() -> Result<Release, String> {
    let response = http_client()?
        .get(LATEST_RELEASE_URL)
        .header(header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| format!("Couldn't reach GitHub: {err}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("No releases have been published yet".into());
    }
    let release: GitHubRelease = response
        .error_for_status()
        .map_err(|err| format!("GitHub didn't return the latest release: {err}"))?
        .json()
        .await
        .map_err(|err| format!("Couldn't read the latest release: {err}"))?;

    let asset = asset_suffix()
        .and_then(|suffix| release.assets.into_iter().find(|asset| asset.name.ends_with(suffix)))
        .map(|asset| Asset {
            url: asset.browser_download_url,
            sha256: asset
                .digest
                .and_then(|digest| digest.strip_prefix("sha256:").map(str::to_ascii_lowercase)),
        });

    Ok(Release {
        version: release.tag_name.trim_start_matches('v').to_owned().into(),
        url: release.html_url.into(),
        asset,
    })
}

/// How this platform's release file name ends; it starts with `qBit-<version>`.
const fn asset_suffix() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("-macos-arm64.app.tar.gz")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("-macos-x86_64.app.tar.gz")
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Some("-windows-x64.zip")
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        Some("-windows-arm64.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("-linux-x86_64.tar.gz")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("-linux-arm64.tar.gz")
    } else {
        None
    }
}

/// Whether `latest` is a higher `major.minor.patch` version than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |version: &str| -> Option<Vec<u64>> { version.split('.').map(|part| part.parse().ok()).collect() };
    match (parse(latest), parse(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// Download `asset`, check it, and put it in place of this app.
///
/// # Returns
///
/// What to launch to run the new version.
async fn download_and_install(asset: Asset) -> Result<PathBuf, String> {
    // Work out where the app lives before downloading, so an unsupported install fails fast.
    let target = install_target()?;

    let archive = http_client()?
        .get(&asset.url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|err| format!("Couldn't download the update: {err}"))?
        .bytes()
        .await
        .map_err(|err| format!("Couldn't download the update: {err}"))?;

    if let Some(expected) = &asset.sha256
        && sha256_hex(&archive) != *expected
    {
        return Err("The downloaded update is corrupt (its checksum doesn't match)".into());
    }

    tokio::task::spawn_blocking(move || replace(&target, &archive))
        .await
        .map_err(|err| format!("The update failed: {err}"))?
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// The `qBit.app` bundle this app is running from.
#[cfg(target_os = "macos")]
fn install_target() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|err| format!("Couldn't find where qBit is installed: {err}"))?;
    // …/qBit.app/Contents/MacOS/qbit
    let bundle = exe
        .ancestors()
        .nth(3)
        .filter(|bundle| bundle.extension().is_some_and(|ext| ext == "app"))
        .ok_or("qBit isn't running from an app bundle, so it can't update itself")?;
    // Gatekeeper runs quarantined apps from a temporary read-only copy until they're moved.
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return Err("Move qBit to your Applications folder, then update".into());
    }
    Ok(bundle.to_path_buf())
}

/// The executable this app is running from.
#[cfg(not(target_os = "macos"))]
fn install_target() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|err| format!("Couldn't find where qBit is installed: {err}"))
}

/// Swap the app bundle for the one in `archive`.
#[cfg(target_os = "macos")]
fn replace(bundle: &Path, archive: &[u8]) -> Result<PathBuf, String> {
    let parent = bundle.parent().ok_or("Couldn't find the folder qBit is in")?;
    // Unpacked beside the app, so swapping them is a rename on the same volume.
    let staging = parent.join(format!(".qBit-update-{}", std::process::id()));
    let result = unpack_and_swap_bundle(archive, &staging, bundle);
    remove_quietly(&staging);
    result.map(|()| bundle.to_path_buf())
}

#[cfg(target_os = "macos")]
fn unpack_and_swap_bundle(archive: &[u8], staging: &Path, bundle: &Path) -> Result<(), String> {
    unpack_tar_gz(archive, staging)?;
    let new_bundle = staging.join("qBit.app");
    if !new_bundle.is_dir() {
        return Err("The update doesn't contain qBit.app".into());
    }

    // A running app can be moved: it keeps running from the files it already has open.
    let old_bundle = staging.join("qBit.app.old");
    fs::rename(bundle, &old_bundle).map_err(|err| format!("Couldn't move the current app aside: {err}"))?;
    if let Err(err) = fs::rename(&new_bundle, bundle) {
        // Put the old app back, so a failed update leaves a working one.
        if let Err(restore) = fs::rename(&old_bundle, bundle) {
            return Err(format!(
                "Couldn't install the update ({err}) or restore the old app ({restore}); it's at {}",
                old_bundle.display()
            ));
        }
        return Err(format!("Couldn't install the update: {err}"));
    }
    Ok(())
}

/// Replace the running executable with the one in `archive`.
#[cfg(not(target_os = "macos"))]
fn replace(exe: &Path, archive: &[u8]) -> Result<PathBuf, String> {
    let staging = std::env::temp_dir().join(format!("qbit-update-{}", std::process::id()));
    remove_quietly(&staging);
    let result = unpack_and_replace_exe(archive, &staging);
    remove_quietly(&staging);
    result.map(|()| exe.to_path_buf())
}

#[cfg(not(target_os = "macos"))]
fn unpack_and_replace_exe(archive: &[u8], staging: &Path) -> Result<(), String> {
    #[cfg(windows)]
    unpack_zip(archive, staging)?;
    #[cfg(not(windows))]
    unpack_tar_gz(archive, staging)?;

    let new_exe = find_exe(staging)?;
    // Handles Windows, where a running executable can't be overwritten, only moved.
    self_replace::self_replace(&new_exe).map_err(|err| format!("Couldn't replace qBit with the update: {err}"))
}

/// The executable in an unpacked release, which keeps it in a `qBit-<version>-…` folder.
#[cfg(not(target_os = "macos"))]
fn find_exe(dir: &Path) -> Result<PathBuf, String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("Couldn't read the unpacked update: {err}"))?;
    std::iter::once(dir.join(BIN_NAME))
        .chain(entries.filter_map(Result::ok).map(|entry| entry.path().join(BIN_NAME)))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("The update doesn't contain {BIN_NAME}"))
}

#[cfg(not(windows))]
fn unpack_tar_gz(archive: &[u8], dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|err| format!("Couldn't unpack the update: {err}"))?;
    tar::Archive::new(flate2::read::GzDecoder::new(archive))
        .unpack(dir)
        .map_err(|err| format!("Couldn't unpack the update: {err}"))
}

#[cfg(windows)]
fn unpack_zip(archive: &[u8], dir: &Path) -> Result<(), String> {
    zip::ZipArchive::new(std::io::Cursor::new(archive))
        .and_then(|mut zip| zip.extract(dir))
        .map_err(|err| format!("Couldn't unpack the update: {err}"))
}

fn remove_quietly(path: &Path) {
    if path.exists()
        && let Err(err) = fs::remove_dir_all(path)
    {
        eprintln!("Couldn't clean up {}: {err}", path.display());
    }
}

/// Start the newly installed version; the caller then quits this one.
fn relaunch(installed: &Path) -> Result<(), String> {
    // `-n` starts a new instance even though this one (with the same bundle ID) is running.
    #[cfg(target_os = "macos")]
    let child = Command::new("open").arg("-n").arg(installed).spawn();
    #[cfg(not(target_os = "macos"))]
    let child = Command::new(installed).spawn();

    child
        .map(drop)
        .map_err(|err| format!("The update is installed, but qBit couldn't restart ({err}); reopen it to finish"))
}

#[cfg(test)]
mod tests {
    use super::{is_newer, sha256_hex};

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }

    #[test]
    fn ignores_versions_it_cant_parse() {
        assert!(!is_newer("nightly", "0.2.0"));
        assert!(!is_newer("0.3.0-beta", "0.2.0"));
    }

    #[test]
    fn hashes_to_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
