/// The application ID.
pub const APP_ID: &str = "qbit";

/// The application title.
pub const APP_TITLE: &str = "qBit";

/// The app's version, from the workspace manifest.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The app's source code on GitHub.
pub const REPO_URL: &str = "https://github.com/bobbymannino/qbit";

/// The app's releases on GitHub, with the changes in each.
pub const RELEASES_URL: &str = "https://github.com/bobbymannino/qbit/releases";

/// Where a new GitHub issue is opened; `?template=` picks one of `.github/ISSUE_TEMPLATE`.
pub const NEW_ISSUE_URL: &str = "https://github.com/bobbymannino/qbit/issues/new";
