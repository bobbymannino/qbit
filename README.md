# qBit

[qBittorrent](https://www.qbittorrent.org/) client UI app written in Rust using
GPUI.

## Install

Download the build for your platform from the
[latest release](https://github.com/bobbymannino/qbit/releases/latest):

| Platform              | File                                 |
| --------------------- | ------------------------------------ |
| macOS (Apple Silicon) | `qBit-<version>-macos-arm64.dmg`     |
| macOS (Intel)         | `qBit-<version>-macos-x86_64.dmg`    |
| Windows (x64)         | `qBit-<version>-windows-x64.zip`     |
| Windows (ARM)         | `qBit-<version>-windows-arm64.zip`   |
| Linux (x86_64)        | `qBit-<version>-linux-x86_64.tar.gz` |
| Linux (ARM)           | `qBit-<version>-linux-arm64.tar.gz`  |

qBit connects to qBittorrent's Web UI, so turn that on in qBittorrent first
(**Tools → Options → Web UI**) and note its address, username and password.

### macOS

1. Open the `.dmg` and drag **qBit** onto the **Applications** shortcut.
2. Eject the disk image.
3. The app isn't notarised, so the first launch is blocked. Open qBit once, then
   go to **System Settings → Privacy & Security** and click **Open Anyway** next
   to the qBit message. Or, from a terminal:

   ```sh
   xattr -dr com.apple.quarantine /Applications/qBit.app
   ```

### Windows

1. Right-click the `.zip` and choose **Extract All…**.
2. Move `qbit.exe` wherever you like, e.g. `%LOCALAPPDATA%\Programs\qBit\`.
3. Run `qbit.exe`. The app isn't signed, so SmartScreen may warn about it: click
   **More info → Run anyway**.
4. Optionally, right-click `qbit.exe` → **Show more options → Pin to Start** (or
   **Send to → Desktop (create shortcut)**).

### Linux

```sh
tar -xzf qBit-<version>-linux-<arch>.tar.gz
cd qBit-<version>-linux-<arch>

# The binary, somewhere on your PATH.
install -Dm755 qbit ~/.local/bin/qbit

# Optional: an entry in your app launcher.
install -Dm644 qbit.desktop ~/.local/share/applications/qbit.desktop
```

Make sure `~/.local/bin` is on your `PATH`. qBit stores your password in the
system keyring via the Secret Service, so a keyring such as GNOME Keyring or
KWallet needs to be running.

## Building from source

Needs Rust (see `rust-version` in `Cargo.toml`). Each script writes its output
to `dist/` and builds for the machine it runs on; set `QBIT_TARGET` to a Rust
target triple to build for another architecture.

```sh
scripts/bundle-macos.sh           # macOS .app and .dmg
scripts/bundle-linux.sh           # Linux tarball
pwsh scripts/bundle-windows.ps1   # Windows zip
```

On Linux you'll need the Wayland, X11 and fontconfig development packages, and
on Windows the MSVC build tools; see `.github/workflows/release.yml` for
details.
