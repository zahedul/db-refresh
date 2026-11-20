<p align="center">
  <img src="icons/icon.png" width="112" alt="DB Refresh icon">
</p>

<h1 align="center">DB Refresh</h1>

<p align="center">
  A native macOS app that finds PostgreSQL dumps in Amazon S3, downloads them, and restores them into local databases — for any project, in one click.
</p>

<p align="center">
  Rust · Dioxus 0.7 · AWS SDK for Rust · macOS 13+ (Apple Silicon)
</p>

---

## Contents

- [Why](#why)
- [Features](#features)
- [How it works](#how-it-works)
- [Tech stack](#tech-stack)
- [Architecture](#architecture)
- [Data, storage and security](#data-storage-and-security)
- [Requirements](#requirements)
- [Development](#development)
- [Building and sharing a release](#building-and-sharing-a-release)
- [Testing](#testing)
- [Project structure](#project-structure)
- [Tunable constants](#tunable-constants)
- [Limitations and roadmap](#limitations-and-roadmap)

---

## Why

Refreshing a local database from production backups usually means: find the newest dump in S3, download a gigabyte, edit a restore script with the right file name and database, run it, and scroll through `pg_restore` output looking for errors. DB Refresh turns that into one button, works for many databases and projects side by side, and keeps passwords in the macOS Keychain instead of shell scripts.

End-user documentation lives in [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) and in the app's **Help** tab.

---

## Features

### Find dumps in S3
- **AWS profile picker** — reads profiles from `~/.aws/config` and `~/.aws/credentials`, including SSO profiles; `default` is pre-selected.
- **Folder browser** — browse a bucket's "folders" (S3 common prefixes) in a popover with breadcrumbs and a filter; loads the root as soon as the bucket is entered.
- **Search** — lists `.dump` objects under the chosen folder, optionally filtered by name; newest first, 10 at a time with *Show more / Show all*; stops after 50,000 objects on huge buckets.
- **Smart list** — file size, relative age ("6h ago", exact UTC time on hover), **Latest** badge per dump group, coloured group tags (e.g. `GEN`, `MAD` from `gen_…`/`mad_…` file names), filter chips per group, and **✓ On disk** for files already downloaded.
- **Remembers your choices** — profile, bucket and folder survive restarts.

### Download
- **Parallel ranged downloads** — 8 MiB parts, 8 in flight, written straight to their offset in a pre-sized file.
- **Resilient** — each failed part is retried on its own (up to 3 attempts with back-off); every part is pinned to the object's **ETag** (`If-Match`), so a backup replaced mid-download fails clearly instead of producing a corrupt mix.
- **Safe files** — data goes to `<name>.part` and is renamed only after the size is verified; cancelling or failing deletes the partial file automatically.
- **Rename before download** — editable *Save as* name with validation, and one-click *Quick names* that add a database's dump prefix.
- **Live progress** — bytes, speed and time left; several downloads at once; cancel, retry, reveal in Finder, or jump straight to restoring.
- **Download index** — remembers which S3 key was saved under which local name, so renamed files still show as on disk.

### Database targets
- **Any number of databases** — host, port, user, database, optional display name and *dump prefix* (`gen` → matches `gen_*.dump`).
- **Quick fill** — paste `postgres://user:pass@host:5432/db` (percent-encoding, IPv6 and query strings handled) to fill the form.
- **Keychain passwords** — optional, encrypted by macOS, never written to the settings file; falls back to `~/.pgpass`.
- **Touch ID** — before a saved password is used (restore, refresh, test connection), the app asks for your fingerprint via LocalAuthentication, falling back to the Mac password. On by default; a typed password never needs it.
- **Test connection** — runs `SELECT 1` with `psql` and reports the round-trip time.
- **One-click source** — each database can store where its newest dump lives (profile, bucket, folder, name filter).
- **Start from an empty database** — optional per database (default for new ones): terminates other sessions, `DROP DATABASE` + `CREATE DATABASE`, then restores — an exact copy without `--clean` noise. Checks `CREATEDB` first, so a database is never dropped without being re-created.

### Restore ("sync")
- **Pick any dump** — downloaded dumps, or any `.dump` file via the native file picker; the matching database is selected automatically from the file-name prefix.
- **Explicit confirmation** — the destructive `pg_restore --clean` is explained before anything runs.
- **Stage indicator** — *Check dump → Connect → Restore data → Done*, plus *Download* (one-click refresh) and *Recreate database* (empty-database mode), driven by marker lines from the restore script.
- **Honest results** — distinguishes *success*, *completed with N skipped items* (`pg_restore` exit 1 with "errors ignored on restore"), *failed* and *cancelled*.
- **Live log** — colour-coded (errors, warnings, success), filter chips (*All / Errors / Warnings*), *Copy log*, capped at 5,000 lines while always keeping errors and warnings.
- **Real cancel** — the restore runs in its own process group, so Cancel also stops `pg_restore`'s parallel workers.
- **Keeps running** when you switch tabs.

### One-click "Refresh latest"
- Finds the newest dump in a database's source, shows exactly what will happen (download or already on disk, target database), then downloads if needed and restores — one job, one progress view, one Cancel.

### App experience
- Three tabs: **Download**, **Sync database**, **Help** (the user guide, rendered from Markdown, works offline).
- Consistent custom UI: popover pickers, modal dialogs, empty states, light and dark mode, brand colour palette defined as CSS variables.
- Native macOS app (~16 MB), custom icon, default Edit menu so copy/paste work in every field.

---

## How it works

```
                 ┌────────────── DB Refresh (one Rust process) ──────────────┐
  ~/.aws  ──────▶│ aws::profiles      list profiles                          │
                 │ aws::s3            list folders / search dumps ───────────┼──▶ Amazon S3
                 │ aws::download      HEAD + 8× ranged GET (If-Match ETag) ◀─┼─── (ListObjectsV2,
                 │        │                                                  │     HeadObject, GetObject)
                 │        ▼                                                  │
                 │  ~/Downloads/db-refresh-dumps/<name>.dump(.part)          │
                 │        │                                                  │
                 │ database::restore  bash -c <embedded restore script> ─────┼──▶ pg_restore / psql
  Keychain ─────▶│ database::credentials  PGPASSWORD (child env only)        │       │
                 │                                                           │       ▼
                 │ ui::*  Dioxus components, signals, AppContext             │  local PostgreSQL
                 └───────────────────────────────────────────────────────────┘
```

**Restore pipeline.** `scripts/restore-postgres.sh` is compiled into the binary with `include_str!` and run as `bash -c "<script>" restore-postgres <dump> <database>`. Connection details arrive as `PGHOST`/`PGPORT`/`PGUSER` (and `PGPASSWORD` when needed) in the child's environment only. The script validates the dump (`pg_restore -l`), tests the connection (`psql … SELECT current_database()`), then runs:

```
pg_restore -w -h $HOST -p $PORT -U $USER -d $DATABASE -j 4 \
  --verbose --no-owner --no-acl --clean --if-exists <dump>
```

With **Start from an empty database** (`RECREATE_DATABASE=1`), the script instead connects to the `postgres` maintenance database, verifies the user has `CREATEDB` (so a database is never dropped without being re-created), terminates other sessions, runs `DROP DATABASE` and `CREATE DATABASE` (names quoted via psql variables), and restores without `--clean`.

`stderr` is merged into `stdout` (`exec 2>&1`) so the log keeps the real order; output is read line by line with lossy UTF-8 decoding so an odd byte can't abort a restore.

**Download pipeline.** `HeadObject` gives the size and ETag → the `.part` file is pre-sized with `set_len` → byte ranges are fetched by a `buffer_unordered(8)` stream inside one task → each part buffers ~1 MiB and writes with `pwrite` (`FileExt::write_all_at`) on `spawn_blocking` → `tokio::select!` interleaves part completion with a 200 ms progress ticker → total size is verified → rename.

---

## Tech stack

| Area | Choice | Version |
|---|---|---|
| Language | Rust (edition 2024) | rustc 1.100 |
| UI framework | [Dioxus](https://dioxuslabs.com) desktop — Rust components rendered in the system WebView (WKWebView); no JavaScript or TypeScript | 0.7.10 |
| Async runtime | Tokio (multi-threaded, provided by Dioxus desktop) | 1.54 |
| AWS | `aws-config` (profiles, SSO, credential chain) + `aws-sdk-s3` | 1.13 / 1.161 |
| Parallelism | `futures-util` (`stream::iter(..).buffer_unordered`) | 0.3 |
| Keychain | `security-framework` (generic passwords) | 3.7 |
| Touch ID | `objc2-local-authentication` + `block2` (LAContext, device-owner policy) | 0.3 / 0.6 |
| File picker | `rfd` (native macOS open panel) | 0.17 |
| Settings | `serde` + `serde_json` | 1.0 |
| Errors | `thiserror` (`AppError` with user-facing messages) | 2.0 |
| Markdown (Help tab) | `pulldown-cmark` (tables enabled) | 0.13 |
| Restore engine | PostgreSQL client tools via Homebrew `libpq` (`pg_restore`, `psql`) | any recent |
| Packaging | Dioxus CLI `dx bundle` → `.app`; `hdiutil` → `.dmg`; `codesign` | dx 0.7.10 |

---

## Architecture

The crate is one binary split into **domain modules** (no UI code, unit-tested) and **UI modules** (Dioxus components).

### Domain modules

| Module | Responsibility |
|---|---|
| `aws::profiles` | Hand-written parser for `[default]` / `[profile name]` sections; picks the saved or default profile. |
| `aws::s3` | Builds a client per profile (resolving credentials up front for clear SSO errors), paginated search, folder listing, S3 error → `AppError` mapping. |
| `aws::s3_path` | Pure helpers for prefixes: breadcrumbs, display paths, folder names. |
| `aws::download` | Parallel ranged downloader, `PartialFile` (RAII: deletes itself unless finished), progress maths. |
| `database::target` | `DatabaseTarget` and `DumpSource` models, validation, prefix-based target suggestion. |
| `database::credentials` | Keychain save / load / delete, with lazy migration from the app's former service name. |
| `database::owner_auth` | Touch ID prompt (`LAContext` device-owner policy) bridged to async with a oneshot channel. |
| `database::connection_url` | `postgres://` URL parser for Quick fill. |
| `database::restore` | Runs the embedded script, process-group cancel, `test_connection`, phase markers, ignored-error parsing. |
| `database::log_line` | Log-line classification (error / warning / success / info). |
| `dump_name` | Dump groups (`gen_…` → `gen`), prefixing, download-name validation. |
| `local_files` | Download folder, local dump listing, "already on disk" check, reveal in Finder. |
| `settings` | JSON settings with atomic writes (temp file + rename) and `#[serde(default)]` for forward compatibility. |
| `legacy_migration` | One-time move of data saved under the app's former name. |
| `format`, `clipboard`, `error` | Human-readable sizes/durations/ages, `pbcopy`, the `AppError` enum. |

### UI modules

| Module | Responsibility |
|---|---|
| `main.rs` | Window configuration, `App` root, settings autosave effect, tab bar. |
| `ui::app_context` | `AppContext` — a `Copy` struct of signals (view, settings, selection, restore state, log) shared with `use_context`. |
| `ui::connect`, `profile_picker`, `folder_browser` | *S3 source* panel and its popover pickers. |
| `ui::results`, `download_dialog`, `downloads`, `jobs` | Backups list, rename dialog, download orchestration and the Downloads panel. |
| `ui::sync::*` | Databases list, database form, Restore panel, Refresh-latest dialog, and `restore_runner` (the job state machine). |
| `ui::picker`, `modal`, `panel_header`, `plain_text_input`, `icons`, `tags` | Shared building blocks. |
| `ui::help` | Renders `docs/USER_GUIDE.md` (compiled in). |

### Key design decisions

- **One process, no IPC.** Dioxus desktop runs Rust UI and logic together; components call domain functions directly. (Tauri was considered, but would have split the app into a frontend and a backend talking over IPC.)
- **Signals for state, context for sharing.** Local UI state lives in component signals; app-wide state (`settings`, restore job, selection) lives in `AppContext`. Settings are saved by a single effect whenever they change.
- **Background work outlives components.** Downloads and restores use `spawn_forever`, so switching tabs or re-running a search never cancels them; their `Task` handles power Cancel.
- **Resource safety through `Drop`.** `PartialFile` removes unfinished downloads on cancel, error or panic — no cleanup code paths to forget.
- **One source of truth for the restore.** The shell script stays human-readable and runnable on its own, yet ships inside the binary.
- **Errors are values with advice.** Every failure becomes an `AppError` whose message tells the user what to do next (e.g. the exact `aws sso login` command).

---

## Data, storage and security

| Data | Location | Notes |
|---|---|---|
| Settings (profile, bucket, folder, database targets, download index) | `~/Library/Application Support/db-refresh/settings.json` | No secrets. Written atomically. |
| Database passwords | macOS Keychain, service `dev.dbrefresh.app`, account = target id | Optional; encrypted by macOS. |
| Downloaded dumps | `~/Downloads/db-refresh-dumps/` | Partial downloads end in `.part`. |
| AWS credentials | Your existing `~/.aws` files / SSO cache | Read via the AWS credential chain; never copied. |

- Passwords are passed to `psql`/`pg_restore` only through the **child process environment**, never on a command line (visible in `ps`).
- The app talks only to **S3** and the **configured databases**. No telemetry.
- The Help page renders HTML only from the bundled guide, never from user input.
- Saved passwords are gated by Touch ID (LocalAuthentication) unless switched off per database.
- Release builds are signed with a stable local certificate when one exists (`scripts/create-signing-certificate.sh`), so Keychain "Always Allow" survives updates; otherwise ad-hoc. Developer ID signing and notarization are supported too.

---

## Requirements

**To run the app**
- macOS 13 or later on Apple Silicon.
- AWS access configured in `~/.aws` (`aws configure` or `aws configure sso`).
- PostgreSQL client tools: `brew install libpq`. The app looks in `/opt/homebrew/opt/libpq/bin`, `/opt/homebrew/bin` and `/usr/local/bin`.

**To develop**
- Rust (stable; developed with 1.100) and Cargo.
- Dioxus CLI: `cargo install dioxus-cli --locked` (installs `dx` into `~/.cargo/bin`).
- Xcode Command Line Tools (for `codesign`, `hdiutil`, `iconutil`).

---

## Development

```bash
dx serve                         # run the app with hot reload (RSX and CSS changes apply live)
cargo test                       # unit tests
cargo clippy --all-targets -- -D warnings   # lint (warnings fail the build)
cargo fmt                        # format
```

Conventions used throughout the code:
- No `unwrap`/`expect` in application code; errors are `AppError` values.
- Early returns over nesting; self-explanatory names; comments explain *why*, not *what*.
- Domain logic stays out of components so it can be unit-tested.
- `docs/USER_GUIDE.md` uses the exact UI labels — update it when the UI changes.

---

## Building and sharing a release

```bash
scripts/build-release.sh
```

Produces in `dist/`:
- `DB-Refresh-<version>-arm64.dmg` — contains **DB Refresh.app**, an **Applications** shortcut and **User Guide.md**.
- `DB-Refresh-<version>-arm64.dmg.sha256` — checksum.

The script builds with the optimised release profile (thin LTO, single codegen unit, stripped symbols), signs the bundle, verifies the signature, builds the DMG and prints a summary. Bump `version` in `Cargo.toml` for each release.

| Mode | How | Recipient experience |
|---|---|---|
| Local certificate (recommended without a Developer ID) | run `scripts/create-signing-certificate.sh` once, then `scripts/build-release.sh` | Same as ad-hoc on other Macs, but Keychain access (and "Always Allow") survives updates. |
| Ad-hoc (default) | `scripts/build-release.sh` | Allow once: **System Settings → Privacy & Security → Open Anyway** (or `xattr -dr com.apple.quarantine "/Applications/DB Refresh.app"`). |
| Developer ID | `SIGNING_IDENTITY="Developer ID Application: Name (TEAMID)" scripts/build-release.sh` | One Gatekeeper prompt. |
| Developer ID + notarized | add `NOTARY_PROFILE=<profile>` (created with `xcrun notarytool store-credentials`) | Opens with no warnings. |

---

## Testing

`cargo test` runs **82 unit tests** (plus 1 opt-in integration test) covering the logic that is easy to get subtly wrong:

- AWS profile parsing (named profiles, SSO sessions, malformed headers).
- Search helpers: name filters, newest-first sorting, latest-per-group, folder placeholders.
- Download maths: byte ranges cover every byte exactly once, out-of-order part writes, partial-file cleanup, retry policy.
- Restore: embedded script arguments and markers, phase detection, ignored-error parsing, result classification (`exit 1` + ignored errors = partial success), log trimming that keeps errors.
- A real run of the embedded script against a missing dump (verifies the bash wiring end to end).
- An opt-in integration test (`cargo test recreate_mode -- --ignored`, needs a disposable PostgreSQL) that restores in empty-database mode and checks the stage sequence.
- Connection-URL parsing (escaping, IPv6, rejection), target validation and suggestion, settings round-trip and defaults, data migration, file-name rules, formatting, and the user guide rendering without web links.

Network, Keychain and database operations are verified manually in the app (they need real AWS access and a PostgreSQL server).

---

## Project structure

```
.
├── Cargo.toml / Cargo.lock      Rust dependencies and release profile
├── Dioxus.toml                  App name, bundle id, icon, macOS settings
├── assets/main.css              All styles (palette as CSS variables, light/dark)
├── docs/USER_GUIDE.md           End-user manual (also the in-app Help tab)
├── icons/                       icon.svg (source), icon.png, AppIcon.icns
├── scripts/
│   ├── restore-postgres.sh      Restore steps (compiled into the app)
│   ├── build-release.sh         Signed .app + .dmg + checksum
│   └── create-signing-certificate.sh   One-time local code-signing certificate
└── src/
    ├── main.rs                  Window, App root, tabs
    ├── aws/                     profiles, s3, s3_path, download
    ├── database/                target, credentials, connection_url, restore, log_line
    ├── ui/                      components (sync/ holds the Sync tab)
    ├── settings.rs, local_files.rs, dump_name.rs, format.rs
    └── error.rs, clipboard.rs, legacy_migration.rs
```

---

## Tunable constants

| Constant | Value | Where |
|---|---|---|
| Download part size / parallel parts | 8 MiB / 8 | `aws/download.rs` |
| Attempts per part | 3 (500 ms × attempt back-off) | `aws/download.rs` |
| Progress update interval | 200 ms | `aws/download.rs` |
| Max objects scanned per search | 50,000 | `aws/s3.rs` |
| Results per page | 10 | `ui/results.rs` |
| `pg_restore` parallel jobs | 4 | `database/restore.rs` |
| Log lines kept / shown | 5,000 / 1,000 | `ui/sync/restore_runner.rs`, `ui/sync/sync_panel.rs` |
| Window size / minimum | 1200×800 / 960×640 | `main.rs` |

---

## Limitations and roadmap

**Current limitations**
- Apple Silicon only (a universal build needs `rustup` targets).
- PostgreSQL custom-format dumps (`pg_dump -Fc`); plain SQL dumps are not supported.
- Restores into one database at a time.
- Downloads don't resume after quitting the app.

**Ideas**
- Universal (Intel + Apple Silicon) builds and a GitHub Actions release workflow.
- Restore history (when, which dump, result).
- Disk-usage view and clean-up of old dumps.
- Resumable downloads.
