# Dropdock

A native Rust / egui application manager for CachyOS and Arch-based KDE desktops. No Tauri, browser UI, or webview.

## Install Dropdock

```sh
git clone https://github.com/dennisbrsv/dropdock.git
cd dropdock
./install.sh
```

The installer reads `/etc/os-release`, detects the distribution family, shows the dependency command, builds the locked Rust source, and installs Dropdock for your user. Run it **without sudo**; only dependency installation requests elevated privileges. It offers to install/update Rust using [rustup](https://rust-lang.org/tools/install/) when Rust 1.98 or newer is unavailable.

- `./install.sh --dry-run`: inspect detection and the dependency plan without changing anything.
- `./install.sh --skip-deps`: build and install using your existing dependencies.
- `./install.sh --yes`: accept dependency and Rust installation prompts automatically.

Dependency setup recognizes Arch/CachyOS, Debian/Ubuntu, Fedora, and openSUSE. Other distributions can use `--skip-deps` after provisioning dependencies. Immutable distributions may require manual setup. The installer builds on your machine rather than distributing a binary compiled against another distro's system libraries.

**Application support differs from installer support.** Native package installation/removal, Debian-to-Arch conversion, PKGBUILD builds and repository tools currently support **Arch-based distributions only**. Other distributions can use portable application imports, Cargo projects and Flatpak; their native apt/dnf/zypper packages appear as external launchers and are not managed by Dropdock. Only CachyOS has been tested interactively so far; other distro dependency plans have automated detection tests, not full desktop validation.

Launch **Dropdock** from your application menu or run `~/.local/bin/dropdock`. Install locations:

- `~/.local/bin/dropdock`
- `${XDG_DATA_HOME:-~/.local/share}/applications/io.dropdock.App.desktop`
- `${XDG_DATA_HOME:-~/.local/share}/icons/hicolor/scalable/apps/io.dropdock.App.svg`

Re-running the installer updates the app without removing its managed applications. Uninstall Dropdock itself by removing these three files; its application data is retained. Flatpak transactions currently use Konsole. Desktop file pickers need a working XDG desktop portal supplied by your desktop environment. Optional Arch Debian conversion needs debtap and its refreshed database; the installer does not run AUR recipes automatically.

## Interface

Version 0.2 adds a refined dark interface, embedded Fira Sans typography, native application icons, searchable application rows, selectable repository results, and focused confirmation dialogs. Ctrl+O opens a file; Ctrl+F opens library search. The layout supports windows down to 860 × 640. Fira Sans licensing is included in `assets/fonts/OFL.txt`. KDE icon lookup and SVG rendering use `kiconfinder6` and `rsvg-convert` when available, with built-in fallbacks.

## Install

Drop a file or folder into the window, or choose it with the file picker. Inspect the details, then confirm the next step.

| Input | Behavior |
| --- | --- |
| Debian `.deb` | Converts with debtap in a private workspace. Review the resulting Arch package, dependencies and any install script, then confirm installation. |
| Arch `.pkg.tar.zst`, `.xz`, `.gz` | Reads package metadata; previews the pacman transaction; installs through pacman after confirmation and Polkit authentication. |
| AppImage | Validates the AppImage header, copies it into your user application directory, makes the copy executable, and creates a menu launcher. |
| Flatpak reference / bundle | Opens a real Flatpak terminal transaction so its permissions and dependencies remain visible. Installs for the current user. |
| Portable folder | Select the main executable. Copies the folder and creates a launcher. Enable “Run in terminal” for CLI applications. |
| Rust project with Cargo.toml | Builds a private copy using cargo, then offers the generated binaries for local installation. Build scripts execute as your user. Projects needing additional resources may need a complete portable folder instead. |
| Folder with PKGBUILD | Runs makepkg as the user in a private copy; offers generated packages for review. Missing build dependencies are reported rather than installed silently. |
| RPM | Provides native Arch repository search and installation. **This release does not convert RPM packages.** |
| Other repositories / formats | Reports the limitation. There is no universal automatic installer for arbitrary source trees. |

Only prepare or install software you trust. Conversion does not guarantee runtime compatibility. No dependency checks are disabled. The GUI itself never runs as root; privileged package operations go through Polkit. Source builds are **not sandboxed** and can execute build scripts as your user.

## My apps

Search application names or package identifiers and filter by source. Enable **Include system packages** to inspect libraries and CLI tools too.

- **Arch / CachyOS:** removes the selected owning package using `pacman -R`. It does not cascade removal or remove dependencies recursively. Pacman blocks removal when another package requires it. Several essential system components are protected by Dropdock as well.
- **Flatpak:** uninstalls from the correct user or system installation; retains application data.
- **Snap:** removes with Snap's normal behavior, including snapshots when configured; does not request purge.
- **Steam:** opens Steam's uninstall dialog. Finish the removal there and refresh Dropdock. Steam entries are discovered from launchers, so stale shortcuts can remain visible.
- **Dropdock local installs:** deletes only its managed copy and launcher. Original downloads/source folders are retained.
- **External launchers:** displayed for visibility, but cannot be uninstalled generically. Browser apps, manually assembled apps and other launchers need their original manager.

An application may share its package with other applications. Read the removal preview. Desktop entries are not a complete software database: hidden launchers are omitted from the app view, while full package mode exposes the package inventory.

## Tools and activity

The Tools page searches enabled repositories and can refresh debtap's dependency database. Native repository installs retain pacman's terminal confirmation. Activity logs are visible, copyable and exportable; recent history persists across launches.

Dropdock 0.2 contains a narrowly matched workaround for debtap 3.6.3's pkgfile-cache filename check: it copies the installed script into its private workspace and fixes only that expression. The system debtap script is never edited. The original script's copyright and license are preserved in the working copy.

Local apps use manual updates. Package-managed updates continue through their usual managers. Prepared files are retained so you can inspect them; you may remove old folders from the work directory when no operation is running.

## Files

- `~/.local/share/dropdock/apps/`: managed local application copies.
- `~/.local/share/dropdock/installed.json`: local installation records.
- `~/.local/share/dropdock/work/`: conversion/build workspaces and outputs.
- `~/.local/share/dropdock/activity.log`: recent activity (rotates at about 2 MB).
- `~/.local/share/applications/dropdock-app-*.desktop`: local app launchers.

Removing Dropdock itself does not remove apps installed with it. Keep its registry if you want to manage those local copies later.

## Build

Run `./build.sh`. Requires Rust/Cargo and a C toolchain. The script respects your Cargo/toolchain selection and uses the system C linker by default. Rust 1.98+ is required.

Runtime tools: pacman, pkexec/Polkit and a working desktop authentication agent; debtap for Debian conversion; Flatpak and Konsole for Flatpak transactions; makepkg for Arch recipes; cargo for Rust projects. Snap and Steam integration are used when present. Tested on this CachyOS KDE Wayland session; not a universal Linux installer.

Developer checks:

```sh
cargo test --locked
cargo clippy --locked -- -D warnings
cargo fmt --check
bash tests/installer.sh
./dropdock --inventory
# Explicitly prepares but DOES NOT install:
./dropdock --prepare /path/to/package.deb
```

The backend tests cover package classification, desktop parsing/escaping, changed-file rejection, local install/remove recovery, external symlink rejection, portable dependency preservation, recursive-copy rejection, and log cleanup. A harmless Debian fixture was converted to an Arch package; a minimal Cargo project was built; the live UI was rendered and inspected. Privileged installs/uninstalls and third-party application runtime compatibility were not tested by modifying existing user applications.

## Upstream components

- [egui / eframe](https://github.com/emilk/egui)
- [debtap](https://github.com/helixarch/debtap)
- [pacman](https://man.archlinux.org/man/pacman.8.en)
- [Flatpak](https://docs.flatpak.org/)

Each dependency retains its own license.

## License and privacy

Project source is MIT licensed. Embedded Fira Sans fonts retain the SIL Open Font License in `assets/fonts/OFL.txt`; dependencies retain their own licenses. The public repository excludes generated binaries, local screenshots, activity logs and installation state. Dropdock does not add telemetry or embed credentials. Package managers and build tools contact their configured repositories when requested.
