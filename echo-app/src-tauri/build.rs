fn main() {
    // The app icon is compiled *into* the binary — Windows reads the taskbar
    // and Alt-Tab icon from the executable's own resources, and the tray takes
    // `default_window_icon()` from the same place. `tauri_build` emits its own
    // `rerun-if-changed` list, which opts this script out of cargo's default
    // "rebuild when any file in the package changed" and does not include the
    // icons. So replacing an icon changed nothing until something else forced a
    // rebuild: the binary kept serving the icon it was born with.
    println!("cargo:rerun-if-changed=icons");

    tauri_build::build()
}
