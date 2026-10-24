//! Stream H: handing a card's file to other apps (share, Open With, Edit with,
//! Upload, shelves) and taking files in from Finder / Explorer.

pub mod actions;
pub mod launch;
pub mod menu;

use ferry_core::integrations::launch_paths;
use tauri::AppHandle;

/// Called once from `setup`: optimises the files this process was started with.
pub fn init(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    crate::platform::services::register(app);
    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    launch::ready(app, launch_paths(&args, &cwd));
}

#[cfg(test)]
mod tests {
    use ferry_core::integrations::FILE_TYPES;

    const HOOKS: &str = include_str!("../../installer/hooks.nsh");
    const PLIST: &str = include_str!("../../Info.plist");

    #[test]
    fn the_installer_registers_every_file_type() {
        for ext in FILE_TYPES.iter().flat_map(|t| t.exts.iter()) {
            assert!(
                HOOKS.contains(&format!("!insertmacro ${{MACRO}} \"{ext}\"")),
                "{ext}"
            );
        }
        assert!(HOOKS.contains(r#"--optimise "%1"'"#));
        // Ferry must never become the default app: nothing sets `.<ext>` itself.
        assert!(!HOOKS.contains(r#"Software\Classes\.${EXT}" """#));
    }

    #[test]
    fn the_bundle_declares_every_file_type_for_open_with_and_services() {
        for t in FILE_TYPES.iter() {
            assert_eq!(
                PLIST
                    .matches(&format!("<string>{}</string>", t.uti))
                    .count(),
                2,
                "{}",
                t.uti
            );
        }
        assert!(PLIST.contains("<string>Alternate</string>"));
        assert!(PLIST.contains("<string>optimiseFiles</string>"));
    }
}
