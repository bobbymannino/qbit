//! Embed the app icon into `qbit.exe` when building for Windows; does nothing elsewhere.

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=packaging/windows/qbit.rc");
    println!("cargo:rerun-if-changed=packaging/windows/qbit.ico");

    // `manifest_required` so a missing resource compiler fails the build instead of
    // silently shipping an exe without its icon.
    embed_resource::compile("packaging/windows/qbit.rc", embed_resource::NONE)
        .manifest_required()
        .map_err(|err| format!("Couldn't embed the Windows icon: {err}"))
}
