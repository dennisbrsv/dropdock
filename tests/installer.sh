#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
for entry in 'cachyos|arch|pacman' 'ubuntu|debian|apt-get' 'fedora||dnf' 'opensuse-tumbleweed|suse|zypper' 'custom|ubuntu debian|apt-get'; do
    IFS='|' read -r id like manager <<< "$entry"
    printf 'ID=%s\nID_LIKE="%s"\n' "$id" "$like" > "$tmp/os-release"
    result=$(DROPDOCK_OS_RELEASE="$tmp/os-release" "$root/install.sh" --dry-run)
    [[ $result == *"sudo $manager "* ]] || { echo "Detection failed: $id"; exit 1; }
done
printf 'ID=unknown\n' > "$tmp/os-release"
if DROPDOCK_OS_RELEASE="$tmp/os-release" "$root/install.sh" --dry-run 2>/dev/null; then exit 1; fi
DROPDOCK_OS_RELEASE="$tmp/os-release" "$root/install.sh" --dry-run --skip-deps >/dev/null
# Deliberately ensure os-release command substitutions stay inert.
# shellcheck disable=SC2016
printf 'ID="$(touch %s/executed)"\n' "$tmp" > "$tmp/os-release"
DROPDOCK_OS_RELEASE="$tmp/os-release" "$root/install.sh" --dry-run --skip-deps >/dev/null
[[ ! -e $tmp/executed ]]
if "$root/install.sh" --invalid 2>/dev/null; then exit 1; fi
bash -n "$root/install.sh" "$root/build.sh"
echo 'Installer detection, unknown distro, inert parsing, and syntax checks passed.'
