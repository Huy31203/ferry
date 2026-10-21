# Ferry M1 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Read `AGENTS.md` and `CONSTRAINTS.md` at the repo root if present, and follow both.

**Goal:** A Tauri 2 + Svelte 5 workspace whose CI builds an installable, unsigned Windows NSIS `.exe` and a macOS universal `.dmg`. The app shows a tray icon, a Settings window, and a non-activating results-stack window with a test card.

**Architecture:**
- **Cargo workspace:** a pure `crates/ferry-core` library (format sniffing, window geometry), tested on all three OSes, and the `src-tauri` app crate (tray, windows, per-OS native glue).
- **Frontend:** a Svelte 5 multi-page frontend in `src/` (`stack.html`, `settings.html`) built by Vite 8 into `dist/`.
- **Sidecar:** gifsicle is built from pinned source by one script and bundled as a sidecar.
- **CI:** GitHub Actions builds both installers on every push, uploads them on `main`, and publishes a Release on `v*` tags.

**Tech Stack:** Rust stable (edition 2024), Tauri 2.12, tauri-nspanel 2.1 (macOS), windows-sys 0.61 (Windows), Svelte 5.57, Vite 8.3, TypeScript 5.9, Vitest 5, pnpm 11.1.2, Node 22, gifsicle 1.98, GitHub Actions (`tauri-apps/tauri-action@v1`).

**Spec:** [docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md). This plan covers milestone **M1** (spec §11). Requirements: [docs/requirements/ferry-optimize-requirements.md](../../requirements/ferry-optimize-requirements.md).

## Global Constraints

- Licence `GPL-3.0-or-later` (spec §2), in `LICENSE` and every manifest.
- `productName` `Ferry`, `mainBinaryName` `Ferry`, identifier `io.ferry.desktop`, version `0.1.0`.
- Targets: Windows `x86_64-pc-windows-msvc` (NSIS, `installMode: currentUser`, WebView2 `downloadBootstrapper`); macOS `universal-apple-darwin` (DMG, `minimumSystemVersion: 13.0`).
- Pin `typescript` to `5.9.3`: `svelte-check` 4.7 only accepts TS 5–6, so do not use TS 7.
- Node 22 (pnpm 11.1.2 does not run on Node 20).
- gifsicle `1.98` from `https://www.lcdf.org/gifsicle/gifsicle-1.98.tar.gz`, SHA-256 `e9d4cc0816db95c64d32e79f778836db0f1f7eb15868490bd330104e5146b66e`.
- Every UI string lives in `src/lib/i18n/en.json`, a flat `{ "key": "string" }` map read by both Svelte and Rust (spec §2, R§3.7).
- Result card 300×200, radius 12 on macOS / 8 on Windows, 16 px visible margin from the work-area edge (spec §7, R§4.4).
- The results stack window must never take keyboard focus (spec §1, criterion 2).
- No `var` in JavaScript or TypeScript (user rule).
- Builds are unsigned (spec §2). Do not add signing.
- Commit message trailer on every commit: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- No dependencies beyond those named in this plan without asking the user.

## Review Focus

1. **Card shown while the user types in another app.** Keystrokes must keep going to that app, and clicking or hovering the card must not activate Ferry. Task 5 Step 6 pins this on macOS; on Windows it is unverified (spec §1).
2. **Unusual monitor layouts.** A monitor left of the primary (negative x), a work area shortened by a taskbar, or a stack taller than the screen must keep the stack fully inside the visible work area. Task 1 geometry tests.
3. **Launching Ferry a second time.** There must be no second tray icon, and Settings opens. Task 3 registers single-instance; Task 7 Step 6 checks it on the built app.
4. **Corrupted or tampered gifsicle download.** The build must stop with "checksum mismatch" and never bundle an unverified binary. Task 4 Step 3.
5. **Closing the Settings window.** Ferry must keep running in the tray; only "Quit Ferry" exits. Task 3 Step 9.

---

## File map

| Path | Responsibility | Task |
| --- | --- | --- |
| `Cargo.toml`, `rust-toolchain.toml`, `.gitignore` | Workspace, toolchain pin, ignores | 1 (members extended in 3) |
| `crates/ferry-core/src/lib.rs` | Core crate root | 1 |
| `crates/ferry-core/src/sniff.rs` | `Format`, `sniff()`: format by magic bytes | 1 |
| `crates/ferry-core/src/geometry.rs` | `Corner`, `Rect`, `place()`: stack position | 1 |
| `package.json`, `pnpm-lock.yaml`, `tsconfig.json`, `vite.config.ts` | Frontend toolchain | 2 |
| `src/stack.html`, `src/settings.html` | Page entries | 2 |
| `src/lib/format.ts` (+ test) | `formatBytes`, `savingPercent` | 2 |
| `src/lib/i18n.ts`, `src/lib/i18n/en.json` (+ test) | `t()`, the string table | 2 (keys added in 5) |
| `src/lib/theme.css` | Design tokens (spec §7) | 2 |
| `src/lib/ipc.ts` | `CardData`, `isMac`, `applyOsClass` | 2 |
| `src/settings/main.ts`, `src/settings/Settings.svelte` | Settings placeholder | 2 |
| `src/stack/main.ts`, `src/stack/Stack.svelte` | Stack root (placeholder in 2, real in 5) | 2, 5 |
| `src/stack/Card.svelte` | Result card, Done state | 5 |
| `LICENSE` | GPL-3.0 text | 3 |
| `src-tauri/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json` | App crate and config | 3 (+4, +5) |
| `src-tauri/icons/app-icon.svg`, `src-tauri/icons/tray.svg` (+ generated PNG/ICNS/ICO) | Icons | 3 |
| `src-tauri/src/main.rs`, `lib.rs` | Entry, builder | 3, 5 |
| `src-tauri/src/i18n.rs` | Rust `t()` over the shared `en.json` | 3 |
| `src-tauri/src/tray.rs` | Tray icon and menu | 3, 5 |
| `src-tauri/src/windows.rs` | Settings window, stack window, `stack_resize`, test card | 3, 5 |
| `src-tauri/src/platform/{mod,macos,windows}.rs` | Non-activating stack, show without focus | 5 |
| `scripts/build-gifsicle.sh` | Pinned gifsicle sidecar build | 4 |
| `.github/workflows/ci.yml`, `release.yml`, `README.md` | CI/CD, install notes | 6 |

---

### Task 1: Toolchain, workspace and `ferry-core` (sniff + geometry)

**Files:**
- Create: `rust-toolchain.toml`, `Cargo.toml`, `.gitignore`, `crates/ferry-core/Cargo.toml`, `crates/ferry-core/src/lib.rs`, `crates/ferry-core/src/sniff.rs`, `crates/ferry-core/src/geometry.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `ferry_core::sniff::{Format, sniff}`, where `Format { Png, Jpeg, Gif, WebP }`, `Format::extension(self) -> &'static str` and `sniff(bytes: &[u8]) -> Option<Format>`.
  - `ferry_core::geometry::{Corner, Rect, place}`, where `Corner { TopLeft, TopRight, BottomLeft, BottomRight }`, `Rect { x: i32, y: i32, width: u32, height: u32 }` and `place(area: Rect, size: (u32, u32), corner: Corner, margin: u32) -> (i32, i32)`.

- [ ] **Step 1: Install Rust and select Node 22**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal -c rustfmt,clippy
source "$HOME/.cargo/env"
rustup target add x86_64-apple-darwin aarch64-apple-darwin
nvm alias default 22 && nvm use 22
corepack enable
rustc --version && cargo --version && node --version
```

Expected: `rustc 1.9x.x`, `cargo 1.9x.x`, `v22.x`. (`pnpm` is checked in Task 2 once `package.json` pins it.)

- [ ] **Step 2: Create the workspace files**

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/ferry-core"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-or-later"
```

`.gitignore`:

```gitignore
/target
/dist
node_modules/
src-tauri/gen/
src-tauri/binaries/
.DS_Store
```

`crates/ferry-core/Cargo.toml`:

```toml
[package]
name = "ferry-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
```

`crates/ferry-core/src/lib.rs`:

```rust
//! Ferry's platform-independent core. No Tauri or OS UI dependencies, so every
//! module is unit-tested on macOS, Windows and Linux.

pub mod geometry;
pub mod sniff;
```

- [ ] **Step 3: Write the failing sniff tests**

`crates/ferry-core/src/sniff.rs`:

```rust
//! Detect an image format from its content, never from the file extension (R§2.4).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl Format {
    /// File extension used when Ferry writes this format.
    pub fn extension(self) -> &'static str {
        unimplemented!()
    }
}

/// The format of `bytes`, or `None` when it is not a supported image.
pub fn sniff(_bytes: &[u8]) -> Option<Format> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_png() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Some(Format::Png));
    }

    #[test]
    fn detects_jpeg_even_when_it_would_be_named_png() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43]), Some(Format::Jpeg));
    }

    #[test]
    fn detects_both_gif_versions() {
        assert_eq!(sniff(b"GIF87a\x01\0\x01\0"), Some(Format::Gif));
        assert_eq!(sniff(b"GIF89a\x01\0\x01\0"), Some(Format::Gif));
    }

    #[test]
    fn detects_webp() {
        assert_eq!(sniff(b"RIFF\x24\0\0\0WEBPVP8 "), Some(Format::WebP));
    }

    #[test]
    fn rejects_other_riff_files() {
        assert_eq!(sniff(b"RIFF\x24\0\0\0WAVEfmt "), None);
    }

    #[test]
    fn rejects_short_empty_and_text_input() {
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"\x89PN"), None);
        assert_eq!(sniff(b"RIFF\0\0\0\0WEB"), None);
        assert_eq!(sniff(b"hello world"), None);
    }

    #[test]
    fn extensions() {
        assert_eq!(Format::Png.extension(), "png");
        assert_eq!(Format::Jpeg.extension(), "jpg");
        assert_eq!(Format::Gif.extension(), "gif");
        assert_eq!(Format::WebP.extension(), "webp");
    }
}
```

Create `crates/ferry-core/src/geometry.rs` with only a comment for now, so the crate compiles:

```rust
//! Where the results stack window goes on screen.
```

- [ ] **Step 4: Run the tests and watch them fail**

Run: `cargo test -p ferry-core sniff`
Expected: FAIL. The `detects_*` tests fail with `left: None, right: Some(..)`, and `extensions` panics with `not implemented`.

- [ ] **Step 5: Implement sniff**

Replace the `extension` and `sniff` bodies in `crates/ferry-core/src/sniff.rs`:

```rust
impl Format {
    /// File extension used when Ferry writes this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::WebP => "webp",
        }
    }
}

/// The format of `bytes`, or `None` when it is not a supported image.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Format::WebP)
    } else {
        None
    }
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p ferry-core sniff`
Expected: `test result: ok. 7 passed`

- [ ] **Step 7: Write the failing geometry tests**

Replace `crates/ferry-core/src/geometry.rs` with:

```rust
//! Where the results stack window goes on screen.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A screen rectangle in physical pixels (a monitor's work area).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Top-left position for a window of `size` placed in `corner` of `area`,
/// `margin` pixels in from both edges. A window larger than the area is
/// pinned to the area's top/left edge so it never starts off-screen.
pub fn place(_area: Rect, _size: (u32, u32), _corner: Corner, _margin: u32) -> (i32, i32) {
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };

    #[test]
    fn bottom_right_sits_margin_from_both_edges() {
        assert_eq!(place(FULL_HD, (300, 200), Corner::BottomRight, 16), (1604, 864));
    }

    #[test]
    fn work_area_above_a_taskbar_is_respected() {
        let above_taskbar = Rect { height: 1032, ..FULL_HD };
        assert_eq!(place(above_taskbar, (300, 200), Corner::BottomRight, 16), (1604, 816));
    }

    #[test]
    fn monitor_left_of_primary_has_negative_origin() {
        let left = Rect { x: -1440, y: 0, width: 1440, height: 900 };
        assert_eq!(place(left, (300, 200), Corner::BottomRight, 16), (-316, 684));
    }

    #[test]
    fn every_corner() {
        assert_eq!(place(FULL_HD, (300, 200), Corner::TopLeft, 16), (16, 16));
        assert_eq!(place(FULL_HD, (300, 200), Corner::TopRight, 16), (1604, 16));
        assert_eq!(place(FULL_HD, (300, 200), Corner::BottomLeft, 16), (16, 864));
    }

    #[test]
    fn window_larger_than_area_is_pinned_inside_top_left() {
        assert_eq!(place(FULL_HD, (2000, 1200), Corner::BottomRight, 16), (0, 0));
    }
}
```

- [ ] **Step 8: Run the tests and watch them fail**

Run: `cargo test -p ferry-core geometry`
Expected: FAIL in `bottom_right_sits_margin_from_both_edges` (`left: (0, 0), right: (1604, 864)`).

- [ ] **Step 9: Implement place**

Replace the `place` function body:

```rust
pub fn place(area: Rect, size: (u32, u32), corner: Corner, margin: u32) -> (i32, i32) {
    let (x0, y0) = (i64::from(area.x), i64::from(area.y));
    let (w, h, m) = (i64::from(size.0), i64::from(size.1), i64::from(margin));
    let right = (x0 + i64::from(area.width) - m - w).max(x0);
    let bottom = (y0 + i64::from(area.height) - m - h).max(y0);
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => x0 + m,
        Corner::TopRight | Corner::BottomRight => right,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => y0 + m,
        Corner::BottomLeft | Corner::BottomRight => bottom,
    };
    (x as i32, y as i32)
}
```

- [ ] **Step 10: Run all core checks**

Run: `cargo fmt --all && cargo test -p ferry-core && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: `test result: ok. 12 passed`. Clippy prints nothing. (CI runs `cargo fmt --all --check`, so always format before committing.)

- [ ] **Step 11: Commit**

```bash
git add rust-toolchain.toml Cargo.toml Cargo.lock .gitignore crates/
git commit -m "feat(core): workspace with format sniffing and stack geometry" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Frontend scaffold (Svelte 5 + Vite 8, format and i18n helpers)

**Files:**
- Create: `package.json`, `tsconfig.json`, `vite.config.ts`, `src/stack.html`, `src/settings.html`, `src/lib/format.ts`, `src/lib/format.test.ts`, `src/lib/i18n.ts`, `src/lib/i18n.test.ts`, `src/lib/i18n/en.json`, `src/lib/theme.css`, `src/lib/ipc.ts`, `src/settings/main.ts`, `src/settings/Settings.svelte`, `src/stack/main.ts`, `src/stack/Stack.svelte`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `formatBytes(bytes: number, locale?: string): string` and `savingPercent(before: number, after: number): string` from `src/lib/format.ts`.
  - `t(key: StringKey, vars?: Record<string, string | number>): string` from `src/lib/i18n.ts`.
  - `interface CardData { id; name; ext; before; after; detail; thumb }`, `isMac: boolean` and `applyOsClass(): void` from `src/lib/ipc.ts`.
  - The pages `stack.html` and `settings.html`, built to `dist/`.
  - The pnpm scripts `dev`, `build`, `check`, `test`, `tauri` and `sidecar`.

- [ ] **Step 1: Create the toolchain files**

`package.json`:

```json
{
  "name": "ferry",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "license": "GPL-3.0-or-later",
  "packageManager": "pnpm@11.1.2",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "check": "svelte-check --tsconfig ./tsconfig.json",
    "test": "vitest run",
    "tauri": "tauri",
    "sidecar": "bash scripts/build-gifsicle.sh"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.12.4"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^7.3.1",
    "@tauri-apps/cli": "^2.12.3",
    "@tsconfig/svelte": "^5.0.8",
    "svelte": "^5.57.2",
    "svelte-check": "^4.7.7",
    "typescript": "5.9.3",
    "vite": "^8.3.5",
    "vitest": "^5.0.5"
  }
}
```

`tsconfig.json`:

```json
{
  "extends": "@tsconfig/svelte/tsconfig.json",
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`vite.config.ts`:

```ts
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

const page = (name: string) => fileURLToPath(new URL(`./src/${name}`, import.meta.url));

// Tauri loads http://localhost:1420 in dev and ../dist in release (src-tauri/tauri.conf.json).
export default defineConfig({
  root: 'src',
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'es2022',
    rolldownOptions: {
      input: { stack: page('stack.html'), settings: page('settings.html') },
    },
  },
  test: { include: ['**/*.test.ts'] },
});
```

- [ ] **Step 2: Install and confirm pnpm runs on Node 22**

Run: `pnpm --version && pnpm install`
Expected: `11.1.2`, then `pnpm-lock.yaml` is created with no errors. If pnpm stops with an "ignored build scripts" error, run `pnpm approve-builds`, approve only the packages it lists, and rerun `pnpm install`.

- [ ] **Step 3: Write the failing helper tests**

`src/lib/format.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { formatBytes, savingPercent } from './format';

describe('formatBytes', () => {
  it.each([
    [0, '0 B'],
    [999, '999 B'],
    [1_000, '1 KB'],
    [1_500, '1.5 KB'],
    [380_000, '380 KB'],
    [2_400_000, '2.4 MB'],
    [12_345_678, '12.3 MB'],
    [999_999, '1 MB'],
    [1_234_567_890, '1.2 GB'],
    [-5, '0 B'],
  ])('%d bytes → %s', (bytes, expected) => {
    expect(formatBytes(bytes, 'en-US')).toBe(expected);
  });
});

describe('savingPercent', () => {
  it('shows savings with a real minus sign', () => {
    expect(savingPercent(2_400_000, 380_000)).toBe('−84%');
  });
  it('shows growth with a plus sign', () => {
    expect(savingPercent(100, 112)).toBe('+12%');
  });
  it('shows 0% for no change or an empty original', () => {
    expect(savingPercent(500, 500)).toBe('0%');
    expect(savingPercent(0, 10)).toBe('0%');
  });
  it('shows −100% when the result is empty', () => {
    expect(savingPercent(10, 0)).toBe('−100%');
  });
});
```

`src/lib/i18n/en.json`:

```json
{
  "app.name": "Ferry",
  "tray.tooltip": "Ferry",
  "tray.settings": "Settings…",
  "tray.about": "About Ferry",
  "tray.quit": "Quit Ferry",
  "settings.window_title": "Ferry Settings",
  "settings.placeholder_title": "Settings",
  "settings.placeholder_body": "Settings panes arrive in the next milestone.",
  "card.aria": "{name}, {state}"
}
```

`src/lib/i18n.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import en from './i18n/en.json';
import { t } from './i18n';

describe('t', () => {
  it('returns the English string', () => {
    expect(t('tray.quit')).toBe('Quit Ferry');
  });
  it('fills placeholders', () => {
    expect(t('card.aria', { name: 'shot.png', state: 'optimised' })).toBe('shot.png, optimised');
  });
  it('leaves unknown placeholders visible', () => {
    expect(t('card.aria', { name: 'shot.png' })).toBe('shot.png, {state}');
  });
});

describe('en.json', () => {
  it('is a flat map of strings, because Rust parses it too', () => {
    for (const value of Object.values(en)) {
      expect(typeof value).toBe('string');
    }
  });
});
```

- [ ] **Step 4: Run the tests and watch them fail**

Run: `pnpm test`
Expected: FAIL with `Failed to resolve import "./format"` and `"./i18n"`.

- [ ] **Step 5: Implement the helpers**

`src/lib/format.ts`:

```ts
const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

/** Decimal (1000-based) size: 2_400_000 → "2.4 MB"; one decimal below 100 units. */
export function formatBytes(bytes: number, locale?: string): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  let digits = unit > 0 && value < 100 ? 1 : 0;
  let rounded = Number(value.toFixed(digits));
  if (rounded >= 1000 && unit < UNITS.length - 1) {
    rounded = 1;
    digits = 0;
    unit += 1;
  }
  const text = new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(rounded);
  return `${text} ${UNITS[unit]}`;
}

/** Change from `before` to `after` as "−84%" / "+12%" / "0%" (U+2212 minus). */
export function savingPercent(before: number, after: number): string {
  if (before <= 0) return '0%';
  const change = Math.round((after / before - 1) * 100);
  if (change === 0) return '0%';
  return change < 0 ? `−${-change}%` : `+${change}%`;
}
```

`src/lib/i18n.ts`:

```ts
import en from './i18n/en.json';

export type StringKey = keyof typeof en;

/** UI string for `key`, with `{name}` placeholders filled from `vars`. */
export function t(key: StringKey, vars: Record<string, string | number> = {}): string {
  return en[key].replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `pnpm test`
Expected: `Tests  18 passed` (14 format, 4 i18n).

- [ ] **Step 7: Create the shared UI files**

`src/lib/theme.css` (tokens from spec §7):

```css
:root {
  color-scheme: light dark;
  --font-ui: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif;
  --card-radius: 12px;
  --bg-content: #f5f5f7;
  --bg-group: #ffffff;
  --bg-sidebar: #e9e9ed;
  --fg: #1d1d1f;
  --fg-2: #5e5e63;
  --accent: #0a64d8;
  --accent-text: #0a64d8;
  --track-off: #8e8e93;
  --error: #c42b1c;
  --rail-bg: rgba(250, 250, 250, 0.92);
  --rail-fg: #1d1d1f;
  --rail-line: rgba(0, 0, 0, 0.2);
  --motion-in: 180ms;
  --motion-out: 150ms;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg-content: #1c1c1e;
    --bg-group: #2c2c2e;
    --bg-sidebar: #232325;
    --fg: #f5f5f7;
    --fg-2: #a1a1a6;
    --accent-text: #6cb4ff;
    --track-off: #7c7c80;
    --error: #ff8a80;
    --rail-bg: rgba(36, 36, 38, 0.9);
    --rail-fg: #f5f5f7;
    --rail-line: rgba(255, 255, 255, 0.18);
  }
}

:root.os-windows {
  --font-ui: 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif;
  --card-radius: 8px;
}

@media (prefers-reduced-motion: reduce) {
  :root {
    --motion-in: 0ms;
    --motion-out: 0ms;
  }
}

html,
body {
  margin: 0;
  font-family: var(--font-ui);
  font-size: 13px;
}

body.window {
  background: var(--bg-content);
  color: var(--fg);
}
```

`src/lib/ipc.ts`:

```ts
/** A result card as sent by Rust. Sizes are bytes; `thumb` is an image URL or null. */
export interface CardData {
  id: string;
  name: string;
  ext: string;
  before: number;
  after: number;
  detail: string;
  thumb: string | null;
}

export const isMac = navigator.userAgent.includes('Mac');

/** Tag <html> with os-mac / os-windows so theme.css can switch fonts and radii. */
export function applyOsClass(): void {
  document.documentElement.classList.add(isMac ? 'os-mac' : 'os-windows');
}
```

`src/settings.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Ferry Settings</title>
  </head>
  <body class="window">
    <div id="app"></div>
    <script type="module" src="/settings/main.ts"></script>
  </body>
</html>
```

`src/settings/main.ts`:

```ts
import { mount } from 'svelte';
import '../lib/theme.css';
import { applyOsClass } from '../lib/ipc';
import Settings from './Settings.svelte';

applyOsClass();
mount(Settings, { target: document.getElementById('app')! });
```

`src/settings/Settings.svelte`:

```svelte
<script lang="ts">
  import { t } from '../lib/i18n';
</script>

<main>
  <h1>{t('settings.placeholder_title')}</h1>
  <p>{t('settings.placeholder_body')}</p>
</main>

<style>
  main {
    padding: 24px 28px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 20px;
    font-weight: 600;
  }
  p {
    margin: 0;
    color: var(--fg-2);
  }
</style>
```

`src/stack.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Ferry results</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/stack/main.ts"></script>
  </body>
</html>
```

`src/stack/main.ts`:

```ts
import { mount } from 'svelte';
import '../lib/theme.css';
import { applyOsClass } from '../lib/ipc';
import Stack from './Stack.svelte';

applyOsClass();
mount(Stack, { target: document.getElementById('app')! });
```

`src/stack/Stack.svelte` (placeholder; Task 5 replaces it):

```svelte
<div class="stack"></div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
  }
</style>
```

- [ ] **Step 8: Type-check, test and build**

Run: `pnpm check && pnpm test && pnpm build`
Expected: `svelte-check found 0 errors and 0 warnings`; tests pass; `dist/stack.html` and `dist/settings.html` exist (`ls dist`).

- [ ] **Step 9: Commit**

If `pnpm approve-builds` created `pnpm-workspace.yaml` in Step 2, add it as well.

```bash
git add package.json pnpm-lock.yaml tsconfig.json vite.config.ts src/
git commit -m "feat(ui): Svelte 5 multi-page scaffold with format and i18n helpers" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Tauri app shell (tray, Settings window, single instance)

**Files:**
- Create: `LICENSE`, `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/app-icon.svg`, `src-tauri/icons/tray.svg` (and the generated icons), `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/i18n.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/windows.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `src/lib/i18n/en.json` (Task 2), `dist/` (Task 2 `pnpm build`).
- Produces:
  - `ferry_lib::run()`.
  - `crate::i18n::t(key: &str) -> String`.
  - `crate::windows::open_settings(app: &AppHandle)`.
  - `crate::tray::build(app: &AppHandle) -> tauri::Result<()>`.
  - Tray menu ids `"settings"` and `"quit"`.

- [ ] **Step 1: Add the licence and register the crate**

```bash
curl -fsSL https://www.gnu.org/licenses/gpl-3.0.txt -o LICENSE
```

In `Cargo.toml`, change the members line to:

```toml
members = ["crates/ferry-core", "src-tauri"]
```

- [ ] **Step 2: Create the crate manifest and build script**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "ferry"
description = "Shrinks images as you copy them."
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
name = "ferry_lib"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
ferry-core = { path = "../crates/ferry-core" }
serde_json = "1"
tauri = { version = "2.12", features = ["tray-icon", "image-png", "macos-private-api"] }
tauri-plugin-single-instance = "2.5"
```

`src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 3: Create the Tauri config and capabilities**

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Ferry",
  "mainBinaryName": "Ferry",
  "version": "0.1.0",
  "identifier": "io.ferry.desktop",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "macOSPrivateApi": true,
    "windows": [],
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost",
      "devCsp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "category": "Utility",
    "shortDescription": "Shrinks images as you copy them.",
    "license": "GPL-3.0-or-later",
    "licenseFile": "../LICENSE",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "windows": {
      "webviewInstallMode": { "type": "downloadBootstrapper", "silent": true },
      "nsis": { "installMode": "currentUser" }
    },
    "macOS": { "minimumSystemVersion": "13.0" }
  }
}
```

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Permissions for Ferry's own windows; no remote content is ever loaded.",
  "windows": ["stack", "settings"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 4: Create and generate the icons**

`src-tauri/icons/app-icon.svg` (the ferry glyph from the approved design, on the accent colour):

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect x="64" y="64" width="896" height="896" rx="200" fill="#0A64D8"/>
  <g transform="translate(212 212) scale(30)" fill="none" stroke="#FFFFFF" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
    <path d="M2.5 11h15l-2 4.2a1 1 0 0 1-.9.6H5.4a1 1 0 0 1-.9-.6z"/>
    <path d="M6 11V7.5h6l2 3.5"/>
    <path d="M2 18.5c1.3 0 1.3-.8 2.7-.8s1.3.8 2.6.8 1.3-.8 2.7-.8 1.3.8 2.6.8 1.3-.8 2.7-.8 1.3.8 2.7.8"/>
  </g>
</svg>
```

`src-tauri/icons/tray.svg` (a black template glyph that macOS tints for light or dark menu bars):

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 20 20" fill="none" stroke="#000000" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
  <path d="M2.5 11h15l-2 4.2a1 1 0 0 1-.9.6H5.4a1 1 0 0 1-.9-.6z"/>
  <path d="M6 11V7.5h6l2 3.5"/>
  <path d="M2 18.5c1.3 0 1.3-.8 2.7-.8s1.3.8 2.6.8 1.3-.8 2.7-.8 1.3.8 2.6.8 1.3-.8 2.7-.8 1.3.8 2.7.8"/>
</svg>
```

Run:

```bash
pnpm tauri icon src-tauri/icons/app-icon.svg -o src-tauri/icons
pnpm tauri icon src-tauri/icons/tray.svg -o src-tauri/icons/tray --png 64
ls src-tauri/icons src-tauri/icons/tray
```

Expected: `src-tauri/icons` contains `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns` and `icon.ico`; `src-tauri/icons/tray` contains `64x64.png`. Delete any `android/` or `ios/` folders the command creates, because mobile is out of scope.

- [ ] **Step 5: Write the failing i18n test (Rust side)**

`src-tauri/src/i18n.rs`:

```rust
//! UI strings, shared with the Svelte frontend through src/lib/i18n/en.json.

use std::{collections::HashMap, sync::LazyLock};

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

/// The English string for `key`; a missing key returns the key itself so the
/// gap is visible in the UI instead of crashing.
pub fn t(key: &str) -> String {
    EN.get(key).cloned().unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::t;

    #[test]
    fn known_key() {
        assert_eq!(t("tray.quit"), "Quit Ferry");
    }

    #[test]
    fn missing_key_falls_back_to_the_key() {
        assert_eq!(t("nope.missing"), "nope.missing");
    }
}
```

`src-tauri/src/main.rs`:

```rust
// Release builds on Windows must not open a console window next to the tray app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ferry_lib::run();
}
```

`src-tauri/src/windows.rs`:

```rust
//! Ferry's windows: Settings (normal, on demand).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::i18n::t;

pub const SETTINGS: &str = "settings";

/// Show the Settings window, creating it the first time.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title(t("settings.window_title"))
        .inner_size(820.0, 600.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        eprintln!("ferry: could not open settings: {e}");
    }
}
```

`src-tauri/src/tray.rs`:

```rust
//! Tray icon and menu (R§4.1–4.2, M1 subset).

use tauri::{
    AppHandle, Result,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

use crate::{i18n::t, windows};

pub fn build(app: &AppHandle) -> Result<()> {
    let settings = MenuItem::with_id(app, "settings", t("tray.settings"), true, Some("CmdOrCtrl+,"))?;
    let about = PredefinedMenuItem::about(app, Some(&t("tray.about")), None)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&settings, &PredefinedMenuItem::separator(app)?, &about, &quit],
    )?;

    let mut tray = TrayIconBuilder::with_id("ferry")
        .tooltip(t("tray.tooltip"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => windows::open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        });

    // macOS tints a black template image for light and dark menu bars;
    // Windows shows the full-colour app icon in the notification area.
    #[cfg(target_os = "macos")]
    {
        tray = tray
            .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray/64x64.png"))?)
            .icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}
```

`src-tauri/src/lib.rs`:

```rust
mod i18n;
mod tray;
mod windows;

pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch focuses Settings instead of adding a tray icon.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::open_settings(app);
        }))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::build(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Ferry")
        .run(|_app, event| {
            // Closing Settings must not quit a tray app; only "Quit Ferry" (an explicit exit code) does.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
```

- [ ] **Step 6: Run the i18n test and watch it fail**

Run: `pnpm build && cargo test -p ferry i18n`
Expected: FAIL in `known_key` (`left: "tray.quit", right: "Quit Ferry"`). `missing_key_falls_back_to_the_key` passes.

- [ ] **Step 7: Load the shared string table**

In `src-tauri/src/i18n.rs`, replace the `EN` static with:

```rust
static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/lib/i18n/en.json"))
        .expect("src/lib/i18n/en.json must be a flat map of strings")
});
```

- [ ] **Step 8: Run tests and lints**

Run: `cargo fmt --all && cargo test -p ferry && cargo clippy --workspace --all-targets -- -D warnings`
Expected: `test result: ok. 2 passed`; clippy is silent.

- [ ] **Step 9: Run the app and check the shell by hand**

Run: `pnpm tauri dev`
Check each and note the result:
1. A ferry glyph appears in the macOS menu bar, and no Dock icon appears.
2. Clicking it shows **Settings…**, a separator, **About Ferry** and **Quit Ferry**.
3. **Settings…** opens an 820×600 window titled "Ferry Settings" showing "Settings" and the placeholder line; light and dark mode both read correctly (toggle in System Settings → Appearance).
4. Closing the Settings window leaves the tray icon in place, and **Settings…** opens it again.
5. **Quit Ferry** removes the tray icon and the `pnpm tauri dev` process exits.

- [ ] **Step 10: Commit**

```bash
git add LICENSE Cargo.toml Cargo.lock src-tauri/
git commit -m "feat(app): Tauri shell with tray menu, settings window and single instance" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: gifsicle sidecar (pinned source build, bundled)

**Files:**
- Create: `scripts/build-gifsicle.sh`
- Modify: `src-tauri/tauri.conf.json` (`bundle.externalBin`)

**Interfaces:**
- Consumes: `rustc -vV` (host triple).
- Produces:
  - `scripts/build-gifsicle.sh [target-triple]` writes `src-tauri/binaries/gifsicle-<triple>[.exe]`. Supported triples are `x86_64-apple-darwin`, `aarch64-apple-darwin`, `universal-apple-darwin` and `x86_64-pc-windows-msvc`.
  - The env var `GIFSICLE_SHA256` overrides the expected checksum; it exists only for the negative test.
  - Bundled sidecar name `binaries/gifsicle`; M2 resolves it at runtime.

- [ ] **Step 1: Write the script**

`scripts/build-gifsicle.sh`:

```bash
#!/usr/bin/env bash
# Build the gifsicle sidecar from pinned, checksum-verified source.
# Usage: scripts/build-gifsicle.sh [target-triple]   (default: the host triple)
# Output: src-tauri/binaries/gifsicle-<triple>[.exe]
set -euo pipefail

VERSION=1.98
SHA256=e9d4cc0816db95c64d32e79f778836db0f1f7eb15868490bd330104e5146b66e
URL="https://www.lcdf.org/gifsicle/gifsicle-${VERSION}.tar.gz"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
CACHE="$ROOT/target/gifsicle-build"
TARBALL="$CACHE/gifsicle-${VERSION}.tar.gz"
WORK="$CACHE/$TRIPLE"
OUT_DIR="$ROOT/src-tauri/binaries"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$CACHE" "$OUT_DIR"
[ -f "$TARBALL" ] || curl -fsSL "$URL" -o "$TARBALL"
ACTUAL="$(sha256 "$TARBALL")"
if [ "$ACTUAL" != "${GIFSICLE_SHA256:-$SHA256}" ]; then
  echo "gifsicle tarball checksum mismatch: got $ACTUAL" >&2
  exit 1
fi

rm -rf "$WORK" && mkdir -p "$WORK"
tar xzf "$TARBALL" -C "$WORK" --strip-components=1
cd "$WORK"

case "$TRIPLE" in
  universal-apple-darwin) ARCHS="-arch x86_64 -arch arm64" ;;
  x86_64-apple-darwin) ARCHS="-arch x86_64" ;;
  aarch64-apple-darwin) ARCHS="-arch arm64" ;;
  x86_64-pc-windows-msvc) ARCHS="" ;;
  *) echo "unsupported target: $TRIPLE" >&2; exit 1 ;;
esac

case "$TRIPLE" in
  *-apple-darwin)
    ./configure --disable-gifview --disable-gifdiff \
      CFLAGS="-O2 $ARCHS -mmacosx-version-min=13.0" >/dev/null
    make -j4 >/dev/null
    BIN="src/gifsicle"
    EXT=""
    ;;
  x86_64-pc-windows-msvc)
    # Needs the MSVC environment (ilammy/msvc-dev-cmd in CI). Put MSVC's own
    # directory first so its link.exe wins over Git Bash's /usr/bin/link.
    export PATH="$(dirname "$(command -v cl)"):$PATH"
    cd src
    # /MT links the C runtime statically, so no VC++ redistributable is needed.
    MSYS_NO_PATHCONV=1 nmake -nologo -f Makefile.w32 gifsicle.exe \
      CFLAGS="-I.. -I..\\include -DHAVE_CONFIG_H -D_CONSOLE /W2 /O2 /MT"
    cd ..
    BIN="src/gifsicle.exe"
    EXT=".exe"
    ;;
esac

DEST="$OUT_DIR/gifsicle-${TRIPLE}${EXT}"
cp "$BIN" "$DEST"
case "$TRIPLE" in *-apple-darwin) strip "$DEST" ;; esac
# A universal app is built as two per-arch cargo builds, and tauri-build checks
# the sidecar for each arch, so the fat binary is copied under both names too.
if [ "$TRIPLE" = universal-apple-darwin ]; then
  cp "$DEST" "$OUT_DIR/gifsicle-aarch64-apple-darwin"
  cp "$DEST" "$OUT_DIR/gifsicle-x86_64-apple-darwin"
fi
"$DEST" --version | head -1
echo "built $DEST"
```

Run: `chmod +x scripts/build-gifsicle.sh`

- [ ] **Step 2: Build for this Mac and the universal target**

Run:

```bash
pnpm sidecar
scripts/build-gifsicle.sh universal-apple-darwin
lipo -archs src-tauri/binaries/gifsicle-universal-apple-darwin
```

Expected: both builds print `LCDF Gifsicle 1.98` and `built …/gifsicle-x86_64-apple-darwin` / `…universal-apple-darwin`; `lipo` prints `x86_64 arm64`. `ls src-tauri/binaries` shows the universal file plus its `aarch64-apple-darwin` and `x86_64-apple-darwin` copies.

- [ ] **Step 3: Check that a bad checksum stops the build**

Run: `GIFSICLE_SHA256=0000 scripts/build-gifsicle.sh; echo "exit=$?"`
Expected: `gifsicle tarball checksum mismatch: got e9d4cc08…` and `exit=1`. Nothing new is written to `src-tauri/binaries/`.

- [ ] **Step 4: Bundle the sidecar**

In `src-tauri/tauri.conf.json`, add `externalBin` to the `bundle` object, right after `"active": true,`:

```json
    "externalBin": ["binaries/gifsicle"],
```

From now on, `cargo build`/`clippy`/`test -p ferry` need `src-tauri/binaries/gifsicle-<host-triple>` to exist (`pnpm sidecar` creates it).

- [ ] **Step 5: Confirm the app bundle contains gifsicle**

Run:

```bash
pnpm tauri build --bundles app
target/release/bundle/macos/Ferry.app/Contents/MacOS/gifsicle --version | head -1
```

Expected: the build succeeds and prints `LCDF Gifsicle 1.98`.

- [ ] **Step 6: Commit**

```bash
git add scripts/build-gifsicle.sh src-tauri/tauri.conf.json
git commit -m "build: pinned gifsicle 1.98 sidecar for macOS and Windows" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Non-activating results stack with a test card

**Files:**
- Create: `src-tauri/src/platform/mod.rs`, `src-tauri/src/platform/macos.rs`, `src-tauri/src/platform/windows.rs`, `src/stack/Card.svelte`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/windows.rs`, `src-tauri/src/tray.rs`, `src/stack/Stack.svelte`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes:
  - `ferry_core::geometry::{Corner, Rect, place}` (Task 1);
  - `formatBytes`, `savingPercent`, `t`, `CardData` and `isMac` (Task 2);
  - `windows::open_settings` and `tray::build` (Task 3).
- Produces:
  - `windows::STACK = "stack"`;
  - `windows::create_stack(app: &AppHandle) -> tauri::Result<()>`;
  - the Tauri command `stack_resize(width: f64, height: f64) -> Result<(), String>` (JS: `invoke('stack_resize', { width, height })`; `0, 0` hides the stack);
  - `windows::show_test_card(app: &AppHandle)`, which emits the event `stack:test-card` with a `CardData` payload;
  - `platform::make_non_activating(&WebviewWindow) -> tauri::Result<()>` and `platform::show_without_focus(&WebviewWindow) -> tauri::Result<()>`;
  - `Card.svelte` props `{ card: CardData; onclose: () => void }`.

- [ ] **Step 1: Add the per-OS dependencies**

Append to `src-tauri/Cargo.toml`:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
tauri-nspanel = "2.1"

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_UI_WindowsAndMessaging"] }
```

- [ ] **Step 2: Write the platform modules**

`src-tauri/src/platform/mod.rs`:

```rust
//! Per-OS native glue. Each OS module exposes the same functions.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{make_non_activating, show_without_focus};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{make_non_activating, show_without_focus};
```

`src-tauri/src/platform/macos.rs`:

```rust
//! macOS: the results stack is a non-activating NSPanel, so showing, hovering
//! or clicking a card never takes focus from the user's app (R§5).

use tauri::{Manager, WebviewWindow};
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelLevel, WebviewWindowExt,
    objc2_app_kit::NSWindowStyleMask, tauri_panel,
};

tauri_panel! {
    panel!(StackPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

pub fn make_non_activating(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel::<StackPanel>()?;
    panel.set_level(PanelLevel::Floating.value());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .stationary()
            .ignores_cycle()
            .value(),
    );
    panel.set_hides_on_deactivate(false);
    panel.set_accepts_mouse_moved_events(true);
    if let Err(e) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
        eprintln!("ferry: could not make the stack non-activating: {e:?}");
    }
    Ok(())
}

pub fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    match window.app_handle().get_webview_panel(window.label()) {
        Ok(panel) => {
            panel.show(); // orderFrontRegardless: visible, not key
            Ok(())
        }
        Err(_) => window.show(),
    }
}
```

`src-tauri/src/platform/windows.rs`:

```rust
//! Windows: the stack is a tool window (no taskbar button, not in Alt+Tab)
//! that never activates. tao already shows unfocusable windows with
//! SW_SHOWNOACTIVATE, so a plain show() leaves focus where it is.

use tauri::WebviewWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

pub fn make_non_activating(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
    Ok(())
}

pub fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()
}
```

- [ ] **Step 3: Create the stack window, the resize command and the test card**

Replace `src-tauri/src/windows.rs` with:

```rust
//! Ferry's windows: the results stack (transparent, never focused) and
//! Settings (normal, on demand).

use std::time::{SystemTime, UNIX_EPOCH};

use ferry_core::geometry::{Corner, Rect, place};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{i18n::t, platform};

pub const STACK: &str = "stack";
pub const SETTINGS: &str = "settings";

/// stack.html pads the cards by 12 logical px (room for shadows); adding this
/// margin keeps cards 16 px from the work-area edge (R§4.4).
const STACK_MARGIN: f64 = 4.0;

/// Create the hidden results stack at startup so its page is loaded and
/// listening before the first card arrives.
pub fn create_stack(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, STACK, WebviewUrl::App("stack.html".into()))
        .title(t("stack.window_title"))
        .inner_size(1.0, 1.0)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .content_protected(true)
        .visible(false)
        .build()?;
    platform::make_non_activating(&window)
}

/// Fit the stack window to its content and pin it to the bottom-right of the
/// work area under the cursor. A zero size hides it.
#[tauri::command]
pub fn stack_resize(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let Some(stack) = app.get_webview_window(STACK) else {
        return Ok(());
    };
    if width < 1.0 || height < 1.0 {
        return stack.hide().map_err(|e| e.to_string());
    }
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or("no monitor found")?;
    let scale = monitor.scale_factor();
    let size = ((width * scale).ceil() as u32, (height * scale).ceil() as u32);
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let (x, y) = place(area, size, Corner::BottomRight, (STACK_MARGIN * scale).round() as u32);

    // macOS positions windows in points; converting with the *target* monitor's
    // scale keeps mixed-DPI setups right. Windows uses physical pixels directly.
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, LogicalSize};
        stack.set_size(LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
        stack
            .set_position(LogicalPosition::new(f64::from(x) / scale, f64::from(y) / scale))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        stack.set_size(PhysicalSize::new(size.0, size.1)).map_err(|e| e.to_string())?;
        stack.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    }
    platform::show_without_focus(&stack).map_err(|e| e.to_string())
}

/// M1 only: put a sample Done card on the stack to check focus and hover.
pub fn show_test_card(app: &AppHandle) {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let card = serde_json::json!({
        "id": format!("test-{id}"),
        "name": "Screenshot 2026-10-21 at 14.02.11",
        "ext": "png",
        "before": 2_400_000,
        "after": 380_000,
        "detail": "2880×1800 → 256 colours",
        "thumb": null
    });
    if let Err(e) = app.emit_to(STACK, "stack:test-card", card) {
        eprintln!("ferry: could not send the test card: {e}");
    }
}

/// Show the Settings window, creating it the first time.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title(t("settings.window_title"))
        .inner_size(820.0, 600.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        eprintln!("ferry: could not open settings: {e}");
    }
}
```

- [ ] **Step 4: Wire the builder and the tray item**

Replace `src-tauri/src/lib.rs` with:

```rust
mod i18n;
mod platform;
mod tray;
mod windows;

pub fn run() {
    // Must be the first plugin: a second launch focuses Settings instead of adding a tray icon.
    let builder = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(
        |app, _args, _cwd| windows::open_settings(app),
    ));
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            windows::create_stack(app.handle())?;
            tray::build(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![windows::stack_resize])
        .build(tauri::generate_context!())
        .expect("error while building Ferry")
        .run(|_app, event| {
            // Closing Settings must not quit a tray app; only "Quit Ferry" (an explicit exit code) does.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
```

Replace `src-tauri/src/tray.rs` with (adds the **Show test card** item at the top):

```rust
//! Tray icon and menu (R§4.1–4.2, M1 subset plus a debug test-card item).

use tauri::{
    AppHandle, Result,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

use crate::{i18n::t, windows};

pub fn build(app: &AppHandle) -> Result<()> {
    let test_card = MenuItem::with_id(app, "test_card", t("tray.test_card"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", t("tray.settings"), true, Some("CmdOrCtrl+,"))?;
    let about = PredefinedMenuItem::about(app, Some(&t("tray.about")), None)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &test_card,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &about,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("ferry")
        .tooltip(t("tray.tooltip"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "test_card" => windows::show_test_card(app),
            "settings" => windows::open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        });

    // macOS tints a black template image for light and dark menu bars;
    // Windows shows the full-colour app icon in the notification area.
    #[cfg(target_os = "macos")]
    {
        tray = tray
            .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray/64x64.png"))?)
            .icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}
```

- [ ] **Step 5: Build the stack UI**

In `src/lib/i18n/en.json`, add these keys (keep the existing ones):

```json
  "tray.test_card": "Show test card",
  "stack.window_title": "Ferry results",
  "card.state.done": "optimised",
  "card.copy": "Copy optimised file",
  "card.reveal.mac": "Show in Finder",
  "card.reveal.windows": "Show in Explorer",
  "card.aggressive": "Optimise aggressively",
  "card.close": "Close"
```

Replace `src/stack/Stack.svelte` with:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import type { CardData } from '../lib/ipc';
  import Card from './Card.svelte';

  let cards = $state<CardData[]>([]);
  let root = $state<HTMLDivElement>();

  // Ask Rust to fit the window to the stack; 0×0 hides it. getBoundingClientRect
  // forces layout, so this works even while the window is still hidden.
  function report() {
    if (!root) return;
    const rect = root.getBoundingClientRect();
    const empty = cards.length === 0;
    invoke('stack_resize', {
      width: empty ? 0 : Math.ceil(rect.width),
      height: empty ? 0 : Math.ceil(rect.height),
    });
  }

  $effect(() => {
    void cards.length;
    report();
  });

  onMount(() => {
    const unlisten = listen<CardData>('stack:test-card', (event) => {
      cards = [...cards, event.payload];
    });
    const observer = new ResizeObserver(report);
    if (root) observer.observe(root);
    return () => {
      observer.disconnect();
      unlisten.then((stop) => stop());
    };
  });

  function close(id: string) {
    cards = cards.filter((card) => card.id !== id);
  }
</script>

<div class="stack" bind:this={root}>
  {#each cards as card (card.id)}
    <Card {card} onclose={() => close(card.id)} />
  {/each}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    width: max-content;
  }
</style>
```

Create `src/stack/Card.svelte` (Done state from the approved design; rail actions are wired in M2, so they are disabled here):

```svelte
<script lang="ts">
  import { formatBytes, savingPercent } from '../lib/format';
  import { t } from '../lib/i18n';
  import { isMac, type CardData } from '../lib/ipc';

  let { card, onclose }: { card: CardData; onclose: () => void } = $props();

  const label = $derived(
    t('card.aria', { name: `${card.name}.${card.ext}`, state: t('card.state.done') }),
  );
</script>

<article class="card" aria-label={label}>
  <div class="thumb" aria-hidden="true">
    {#if card.thumb}
      <img src={card.thumb} alt="" />
    {:else}
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" /></svg>
    {/if}
  </div>
  <div class="scrim" aria-hidden="true"></div>

  <div class="rail">
    <button type="button" aria-label={t('card.copy')} disabled>
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
    </button>
    <button type="button" aria-label={t(isMac ? 'card.reveal.mac' : 'card.reveal.windows')} disabled>
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
    </button>
    <button type="button" aria-label={t('card.aggressive')} disabled>
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 3 14h9l-1 8 10-12h-9l1-8z" /></svg>
    </button>
  </div>

  <button class="close" type="button" aria-label={t('card.close')} onclick={onclose}>
    <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
  </button>

  <div class="info">
    <div class="chips">
      <span class="name">{card.name}</span>
      <span class="ext">{card.ext}</span>
    </div>
    <div class="sizes">
      <span>{formatBytes(card.before)} → {formatBytes(card.after)}</span>
      <span class="pill ok">
        <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
        {savingPercent(card.before, card.after)}
      </span>
    </div>
    <span class="detail">{card.detail}</span>
  </div>
</article>

<style>
  .card {
    position: relative;
    width: 300px;
    height: 200px;
    border-radius: var(--card-radius);
    overflow: hidden;
    background: #2c2c2e;
    color: #fff;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22), 0 0 0 0.5px rgba(0, 0, 0, 0.18);
    font-variant-numeric: tabular-nums;
    animation: slide-in var(--motion-in) ease-out;
  }
  .thumb {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-bottom: 36px;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.55);
  }
  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .scrim {
    position: absolute;
    inset: auto 0 0 0;
    height: 64%;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.8), rgba(0, 0, 0, 0.5) 45%, rgba(0, 0, 0, 0));
  }
  .rail {
    position: absolute;
    left: 8px;
    top: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 0 0 0.5px var(--rail-line);
    opacity: 0;
    transition: opacity 120ms;
  }
  .rail button {
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
  }
  .rail button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .close {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 0.5px solid rgba(255, 255, 255, 0.35);
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
    opacity: 0;
    transition: opacity 120ms;
  }
  .card:hover .rail,
  .card:focus-within .rail,
  .card:hover .close,
  .card:focus-within .close {
    opacity: 1;
  }
  .info {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    transition: left 120ms;
  }
  .card:hover .info,
  .card:focus-within .info {
    left: 52px;
  }
  .chips {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .name {
    min-width: 0;
    flex-shrink: 1;
    background: rgba(255, 255, 255, 0.16);
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ext {
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.45);
    font-size: 11px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
  }
  .sizes {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    font-weight: 600;
    padding: 1px 8px 1px 6px;
    border-radius: 999px;
  }
  .pill.ok {
    background: rgba(48, 209, 88, 0.26);
    color: #8cf5ae;
  }
  .detail {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.82);
  }
  @keyframes slide-in {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
</style>
```

- [ ] **Step 6: Run all checks, then check the focus gate and the hover spike by hand**

Run: `cargo fmt --all && pnpm check && pnpm test && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: everything passes, with no warnings.

Run `pnpm tauri dev`, then check each item and record the result:
1. Open TextEdit and start typing. While typing, use the tray menu's **Show test card**. The card slides in 16 px from the bottom-right corner of the work area, and **typing keeps going into TextEdit** (Review Focus 1). Press ⌘⇥: Ferry is not in the switcher.
2. Show a second test card. It appears below the first with an 8 px gap; the window grows upward, so the stack stays pinned to the bottom-right.
3. **Hover spike:** move the pointer onto a card without clicking. The rail (three greyed buttons) and the close button fade in, and the info strip shifts right. Record `HOVER: PASS` or `HOVER: FAIL`.
4. Click a card's close button. That card goes; closing the last card hides the window. TextEdit stays the frontmost app throughout.
5. Press ⌘⇧4 and then space over the card. The screenshot does not show the card (content protection).
6. Put Safari in full screen and show a test card. It appears over the full-screen Safari.
7. If a second monitor is connected, move the pointer there and show a card; it appears on that monitor's bottom-right.

If item 3 is `HOVER: FAIL`, do not change approach in M1. Record it, because M2's plan then switches the card to `tauri-nspanel` tracking-area events (spec §12).

- [ ] **Step 7: Commit**

Put the results of manual items 1–7 in the commit message body:

```bash
git add src-tauri/ src/
git commit -m "feat(app): non-activating results stack with test card" -m "Manual: focus kept=<yes/no>, HOVER=<PASS/FAIL>, screenshot hidden=<yes/no>, fullscreen=<yes/no>, multi-monitor=<yes/no/n.a.>" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: CI/CD workflows and README

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`

**Interfaces:**
- Consumes:
  - `pnpm check`, `pnpm test` and `pnpm build` (Task 2);
  - `scripts/build-gifsicle.sh <triple>` (Task 4);
  - `cargo test -p ferry-core` and `cargo test -p ferry`;
  - bundle output paths `target/<triple>/release/bundle/{nsis,dmg}/`.
- Produces:
  - Workflow artifacts `ferry-windows` (`*.exe`) and `ferry-macos` (`*.dmg`) on pushes to `main`;
  - a GitHub Release with both installers attached on every `v*` tag.

- [ ] **Step 1: Write the CI workflow**

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
  pull_request:

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  core:
    name: core (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - if: runner.os == 'Linux'
        run: cargo fmt --all --check
      - run: cargo clippy -p ferry-core --all-targets -- -D warnings
      - run: cargo test -p ferry-core

  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: pnpm/action-setup@v6
      - uses: actions/setup-node@v7
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm check
      - run: pnpm test

  bundle:
    name: bundle (${{ matrix.name }})
    needs: [core, web]
    strategy:
      fail-fast: false
      matrix:
        include:
          - name: windows
            os: windows-latest
            target: x86_64-pc-windows-msvc
            rust_targets: x86_64-pc-windows-msvc
            bundles: nsis
          - name: macos
            os: macos-latest
            target: universal-apple-darwin
            rust_targets: aarch64-apple-darwin,x86_64-apple-darwin
            bundles: dmg
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
          targets: ${{ matrix.rust_targets }}
      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}
      - uses: pnpm/action-setup@v6
      - uses: actions/setup-node@v7
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - if: runner.os == 'Windows'
        uses: ilammy/msvc-dev-cmd@v1
      - name: Build gifsicle sidecar
        shell: bash
        run: scripts/build-gifsicle.sh ${{ matrix.target }}
      - run: pnpm build
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test -p ferry
      - name: Build installer
        run: pnpm tauri build --target ${{ matrix.target }} --bundles ${{ matrix.bundles }}
      - name: Smoke test installer
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          $installer = Get-ChildItem "target/${{ matrix.target }}/release/bundle/nsis/*.exe" | Select-Object -First 1
          if (-not $installer) { throw "no NSIS installer was produced" }
          Start-Process -FilePath $installer.FullName -ArgumentList '/S' -Wait
          $exe = Join-Path $env:LOCALAPPDATA 'Ferry\Ferry.exe'
          if (-not (Test-Path $exe)) {
            Get-ChildItem -Recurse (Join-Path $env:LOCALAPPDATA 'Ferry') -ErrorAction SilentlyContinue | Format-Table FullName
            throw "Ferry.exe not found at $exe"
          }
          if (-not (Test-Path (Join-Path $env:LOCALAPPDATA 'Ferry\gifsicle.exe'))) { throw "gifsicle sidecar missing from install" }
          $proc = Start-Process -FilePath $exe -PassThru
          Start-Sleep -Seconds 10
          if ($proc.HasExited) { throw "Ferry exited early with code $($proc.ExitCode)" }
          Stop-Process -Id $proc.Id -Force
          Write-Host "Ferry installed and stayed running for 10 s"
      - name: Upload installer
        if: github.ref == 'refs/heads/main'
        uses: actions/upload-artifact@v7
        with:
          name: ferry-${{ matrix.name }}
          path: |
            target/${{ matrix.target }}/release/bundle/nsis/*.exe
            target/${{ matrix.target }}/release/bundle/dmg/*.dmg
          if-no-files-found: error
          retention-days: 90
```

- [ ] **Step 2: Write the release workflow**

`.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  release:
    name: release (${{ matrix.name }})
    strategy:
      fail-fast: false
      matrix:
        include:
          - name: windows
            os: windows-latest
            target: x86_64-pc-windows-msvc
            rust_targets: x86_64-pc-windows-msvc
            bundles: nsis
          - name: macos
            os: macos-latest
            target: universal-apple-darwin
            rust_targets: aarch64-apple-darwin,x86_64-apple-darwin
            bundles: dmg
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.rust_targets }}
      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}
      - uses: pnpm/action-setup@v6
      - uses: actions/setup-node@v7
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - if: runner.os == 'Windows'
        uses: ilammy/msvc-dev-cmd@v1
      - name: Build gifsicle sidecar
        shell: bash
        run: scripts/build-gifsicle.sh ${{ matrix.target }}
      - uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: Ferry ${{ github.ref_name }}
          releaseBody: >-
            Unsigned build. Windows: SmartScreen shows "Windows protected your PC";
            choose More info → Run anyway. macOS: right-click Ferry.app → Open the first time.
          releaseDraft: false
          prerelease: false
          args: --target ${{ matrix.target }} --bundles ${{ matrix.bundles }}
```

- [ ] **Step 3: Write the README**

`README.md`:

````markdown
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
````

- [ ] **Step 4: Lint the workflows**

Run:

```bash
mkdir -p target/tools && cd target/tools && bash <(curl -sSfL https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash) && cd ../.. && target/tools/actionlint
```

Expected: actionlint prints nothing and exits 0. Fix any reported issue, then rerun.

- [ ] **Step 5: Commit**

```bash
git add .github/ README.md
git commit -m "ci: test, bundle and smoke-test installers; release on v* tags" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Publish to GitHub and verify the installers (needs the user's confirmation)

**Files:** none changed unless CI finds a defect (fix it in the owning task's files, then re-run CI).

**Interfaces:**
- Consumes: the workflows from Task 6.
- Produces: a GitHub repository with a green CI run on `main`, plus downloadable `ferry-windows` and `ferry-macos` artifacts.

- [ ] **Step 1: Ask the user before anything leaves the machine**

Ask: "Create a GitHub repository for Ferry? Please give the owner/name (default `ferry`) and visibility (private or public)." Wait for an explicit answer. Do not create or push anything without it.

- [ ] **Step 2: Install the GitHub CLI and let the user sign in**

```bash
mkdir -p "$HOME/.local/bin" target/tools/gh
GH_VERSION=$(curl -s https://api.github.com/repos/cli/cli/releases/latest | python3 -c "import sys,json; print(json.load(sys.stdin)['tag_name'].lstrip('v'))")
curl -fsSL "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_macOS_amd64.zip" -o target/tools/gh/gh.zip
unzip -o -q target/tools/gh/gh.zip -d target/tools/gh
cp "target/tools/gh/gh_${GH_VERSION}_macOS_amd64/bin/gh" "$HOME/.local/bin/gh"
"$HOME/.local/bin/gh" --version
```

Ask the user to run `! ~/.local/bin/gh auth login` themselves, because signing in is theirs to do. Then confirm with `~/.local/bin/gh auth status`.

- [ ] **Step 3: Create the repository and push**

With the answer from Step 1 (example shown for a private repo named `ferry`):

```bash
~/.local/bin/gh repo create ferry --private --source . --remote origin --push
```

Expected: the repository URL is printed, and `main` is pushed with all commits.

- [ ] **Step 4: Watch CI**

Run: `~/.local/bin/gh run watch --exit-status $(~/.local/bin/gh run list --branch main --limit 1 --json databaseId -q '.[0].databaseId')`
Expected: the `core` (×3), `web`, `bundle (windows)` and `bundle (macos)` jobs all succeed. The Windows smoke step logs `Ferry installed and stayed running for 10 s`.

If a job fails, open its log with `~/.local/bin/gh run view --log-failed` and use superpowers:systematic-debugging. If gifsicle fails to build under MSVC `nmake`, use the spec §9 fallback (MSYS2 `mingw-w64-x86_64-gcc` with `Makefile.mingw`), changing only the Windows branch of `scripts/build-gifsicle.sh` and adding an `msys2/setup-msys2` step.

- [ ] **Step 5: Download both installers**

```bash
RUN=$(~/.local/bin/gh run list --branch main --limit 1 --json databaseId -q '.[0].databaseId')
~/.local/bin/gh run download "$RUN" -n ferry-windows -D target/ci-artifacts/windows
~/.local/bin/gh run download "$RUN" -n ferry-macos -D target/ci-artifacts/macos
ls -la target/ci-artifacts/*/
```

Expected: `Ferry_0.1.0_x64-setup.exe` and `Ferry_0.1.0_universal.dmg`.

- [ ] **Step 6: Check the CI-built DMG on this Mac (M1 exit)**

```bash
hdiutil attach target/ci-artifacts/macos/Ferry_0.1.0_universal.dmg -nobrowse -mountpoint /Volumes/FerryM1
lipo -archs /Volumes/FerryM1/Ferry.app/Contents/MacOS/Ferry
open /Volumes/FerryM1/Ferry.app
```

Check each:
1. `lipo` prints `x86_64 arm64`.
2. The tray icon appears.
3. **Show test card** shows a card without taking focus from the frontmost app (repeat Task 5 Step 6 item 1).
4. Running `open /Volumes/FerryM1/Ferry.app` again opens Settings and adds **no second tray icon** (Review Focus 3).
5. **Quit Ferry** exits.

Then run `hdiutil detach /Volumes/FerryM1`.

- [ ] **Step 7: Optional release dry run (ask first)**

Ask: "Tag `v0.1.0` and publish a GitHub Release with both installers?" Only on an explicit yes:

```bash
git tag v0.1.0 && git push origin v0.1.0
~/.local/bin/gh run watch --exit-status $(~/.local/bin/gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')
~/.local/bin/gh release view v0.1.0
```

Expected: release `Ferry v0.1.0` lists the `.exe` and the `.dmg`.

- [ ] **Step 8: Report**

Tell the user:
- the repository URL;
- the CI run URL;
- where the `.exe` is (artifact `ferry-windows`, or the Release);
- the manual results from Task 5 Step 6 and Task 7 Step 6, including `HOVER: PASS/FAIL`.

Also state plainly that **Windows desktop behaviour is built but unverified**: CI only proved install + launch.
