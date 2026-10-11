#!/usr/bin/env bash
# Build and install Dropdock for the current user. Never build as root.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
dry_run=false
assume_yes=false
skip_deps=false
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=true ;;
        --yes) assume_yes=true ;;
        --skip-deps) skip_deps=true ;;
        --help|-h) printf '%s\n' 'Usage: ./install.sh [--dry-run] [--yes] [--skip-deps]' 'Installs to ~/.local/bin and your XDG application menu.' '--yes approves dependency and Rust toolchain installation.'; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$arg" >&2; exit 2 ;;
    esac
done
[[ $(uname -s) == Linux ]] || { echo 'Dropdock requires Linux.' >&2; exit 1; }
if [[ $EUID == 0 ]] && ! $dry_run; then
    echo 'Run this script as your normal user, without sudo. It requests sudo only for dependencies.' >&2
    exit 1
fi
# The override is for dry-run distribution tests only; never source arbitrary files.
os_release=/etc/os-release
if $dry_run; then os_release=${DROPDOCK_OS_RELEASE:-$os_release}; fi
read_os_value() {
    local line value
    while IFS= read -r line; do
        if [[ $line == "$1="* ]]; then
            value=${line#*=}; value=${value#\"}; value=${value%\"}; value=${value#\'}; value=${value%\'}
            printf '%s' "$value"; return
        fi
    done < "$os_release"
}
[[ -r $os_release ]] || { echo 'Cannot read /etc/os-release.' >&2; exit 1; }
id=$(read_os_value ID)
like=$(read_os_value ID_LIKE)
family=unknown
for candidate in "$id" $like; do
    case "$candidate" in
        arch|cachyos|manjaro|endeavouros) family=arch ;;
        debian|ubuntu|linuxmint|pop) family=debian ;;
        fedora) family=fedora ;;
        opensuse*|suse) family=suse ;;
        *) continue ;;
    esac
    break
done
printf 'Detected: %s (%s)\n' "$id" "$family"
case "$family" in
    arch) deps=(pacman -S --needed base-devel pkgconf libx11 libxcursor libxi libxrandr libxkbcommon wayland mesa flatpak konsole polkit xdg-utils librsvg curl ca-certificates) ;;
    debian) deps=(apt-get install build-essential pkg-config libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev flatpak konsole pkexec xdg-utils librsvg2-bin ca-certificates curl) ;;
    fedora) deps=(dnf install gcc gcc-c++ make pkgconf-pkg-config libX11-devel libXcursor-devel libXi-devel libXrandr-devel libxkbcommon-devel wayland-devel mesa-libGL-devel flatpak konsole polkit xdg-utils librsvg2-tools curl) ;;
    suse) deps=(zypper install gcc gcc-c++ make pkg-config libX11-devel libXcursor-devel libXi-devel libXrandr-devel libxkbcommon-devel wayland-devel Mesa-libGL-devel flatpak konsole polkit xdg-utils rsvg-convert curl) ;;
    *)
        if ! $skip_deps; then
            echo 'Unsupported dependency manager. Install a C compiler, pkg-config, X11/Wayland/OpenGL development libraries and Rust 1.98+, then use --skip-deps.' >&2
            exit 1
        fi
        deps=()
        ;;
esac
if [[ $family != arch ]]; then
    echo 'Portable apps, Cargo projects and Flatpak are available. Native package conversion/removal and repository tools currently require Arch Linux.'
fi
confirm() {
    $assume_yes && return 0
    local answer
    read -r -p "$1 [y/N] " answer || return 1
    [[ $answer == y || $answer == Y || $answer == yes ]]
}
if ! $skip_deps; then
    printf 'Dependency command: sudo'; printf ' %q' "${deps[@]}"; printf '\n'
    if ! $dry_run; then
        confirm 'Install these dependencies?' || { echo 'Cancelled. Use --skip-deps if already installed.'; exit 1; }
        command -v sudo >/dev/null || { echo 'sudo is required to install dependencies.' >&2; exit 1; }
        flags=()
        if $assume_yes; then
            case "$family" in arch) flags=(--noconfirm);; suse) flags=();; *) flags=(-y);; esac
        fi
        # Do not refresh Arch databases without a full system upgrade.
        if [[ $family == debian ]]; then sudo apt-get update; fi
        if [[ $family == suse ]] && $assume_yes; then
            sudo zypper --non-interactive "${deps[@]:1}"
        else
            sudo "${deps[@]}" "${flags[@]}"
        fi
    fi
fi
if $dry_run; then
    echo 'Would check Rust >= 1.98, build with Cargo.lock, install ~/.local/bin/dropdock and a user application-menu launcher.'
    exit 0
fi
export PATH="$HOME/.cargo/bin:$PATH"
rust_sufficient() {
    command -v rustc >/dev/null && command -v cargo >/dev/null || return 1
    local version
    version=$(rustc --version | awk '{print $2}')
    [[ $(printf '1.98.0\n%s\n' "$version" | sort -V | head -n1) == 1.98.0 ]]
}
if ! rust_sufficient; then
    confirm 'Install/update the stable Rust toolchain using rustup?' || { echo 'Rust 1.98+ is required.'; exit 1; }
    if command -v rustup >/dev/null; then
        rustup toolchain install stable --profile minimal
        export RUSTUP_TOOLCHAIN=stable
    else
        command -v curl >/dev/null || { echo 'Install curl or Rust 1.98+ first.' >&2; exit 1; }
        installer=$(mktemp)
        trap 'rm -f -- "$installer"' EXIT
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$installer"
        sh "$installer" -y --profile minimal --default-toolchain stable --no-modify-path
        export RUSTUP_TOOLCHAIN=stable
    fi
    rust_sufficient || { echo 'Rust toolchain setup failed.' >&2; exit 1; }
fi
./build.sh
bin_dir="$HOME/.local/bin"
data_dir=${XDG_DATA_HOME:-$HOME/.local/share}
[[ $data_dir == /* ]] || { echo 'XDG_DATA_HOME must be absolute.' >&2; exit 1; }
mkdir -p -- "$bin_dir" "$data_dir/applications" "$data_dir/icons/hicolor/scalable/apps"
install -m 755 dropdock "$bin_dir/dropdock.new"
mv -f -- "$bin_dir/dropdock.new" "$bin_dir/dropdock"
install -m 644 dropdock.svg "$data_dir/icons/hicolor/scalable/apps/io.dropdock.App.svg"
# Quote Desktop Entry Exec correctly, including spaces and literal percent signs.
exec_path="$bin_dir/dropdock"
[[ $exec_path != *$'\n'* && $exec_path != *$'\r'* ]] || { echo 'Unsupported newline in HOME.' >&2; exit 1; }
exec_path=${exec_path//\\/\\\\\\\\}
exec_path=${exec_path//\"/\\\\\\\"}
exec_path=${exec_path//\$/\\\\\$}
exec_path=${exec_path//\`/\\\\\`}
exec_path=${exec_path//%/%%}
while IFS= read -r line; do
    if [[ $line == Exec=* ]]; then printf 'Exec="%s" %%F\n' "$exec_path"; else printf '%s\n' "$line"; fi
done < Dropdock.desktop > "$data_dir/applications/io.dropdock.App.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$data_dir/applications" || true
printf 'Installed. Launch Dropdock from your application menu or run %s/dropdock\n' "$bin_dir"
[[ $family != arch ]] || echo 'Optional: install debtap from a trusted Arch source and refresh its database for Debian conversion.'
