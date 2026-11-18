// No commands for the page to call directly: the page only talks to the host's `api` command,
// which calls this plugin from Rust. The android/ folder is what adds the Kotlin to the app.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
