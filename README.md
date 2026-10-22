# Ferry

Ferry is a tray utility for macOS and Windows that shrinks images as you copy them. It is free and GPL-3.0-or-later.

## Install

Download the latest installer from **Releases** (or, for the newest `main` build, from the
`ferry-windows` / `ferry-macos` artifact of the latest CI run).

Builds are not code-signed yet:

- **Windows** (`Ferry_x.y.z_x64-setup.exe`): SmartScreen says "Windows protected your PC".
  Choose **More info → Run anyway**. Ferry installs for the current user only and needs no admin rights.
- **macOS** (`Ferry_x.y.z_universal.dmg`, macOS 13+): drag Ferry to Applications. The first time,
  right-click Ferry.app and choose **Open**.

Ferry has no main window; look for its icon in the menu bar or the notification area.

## Develop

Requirements: Rust stable, Node 22 with pnpm 11 (`corepack enable`), and Xcode Command Line Tools on macOS or
Visual Studio Build Tools on Windows.

```bash
pnpm install
pnpm sidecar          # builds src-tauri/binaries/gifsicle-<host-triple>
pnpm tauri dev
```

Checks: `pnpm check && pnpm test && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`.

Design: `docs/superpowers/specs/`. Requirements: `docs/requirements/`.
