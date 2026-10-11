#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:-/usr/bin/cc}
export CC=${CC:-/usr/bin/cc}
export CXX=${CXX:-/usr/bin/c++}
cargo build --release --locked
build_dir=${CARGO_TARGET_DIR:-target}
install -m 755 "$build_dir/release/dropdock" ./dropdock.new
mv -f ./dropdock.new ./dropdock
printf 'Built ./dropdock.\n'
