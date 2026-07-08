#!/usr/bin/env bash
set -euo pipefail

version="${1:-0.2.0}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$repo_root/obs/dist"
work_dir="$repo_root/obs/.work"
source_dir="$work_dir/tarminal-$version"

rm -rf "$work_dir"
mkdir -p "$out_dir" "$work_dir"
rm -f "$out_dir/tarminal-$version.tar.zst" "$out_dir/vendor.tar.zst"

git -C "$repo_root" archive --format=tar --prefix="tarminal-$version/" HEAD \
  | zstd -T0 -19 -f -o "$out_dir/tarminal-$version.tar.zst"

git -C "$repo_root" archive --format=tar --prefix="tarminal-$version/" HEAD \
  | tar -x -C "$work_dir"

mkdir -p "$source_dir/.cargo"
cargo vendor --locked --manifest-path "$source_dir/Cargo.toml" "$source_dir/vendor" \
  > "$source_dir/.cargo/config.toml"

tar -C "$source_dir" --zstd -cf "$out_dir/vendor.tar.zst" vendor .cargo

cp "$repo_root/obs/tarminal.spec" "$out_dir/tarminal.spec"
cp "$repo_root/obs/tarminal.changes" "$out_dir/tarminal.changes"
cp "$repo_root/obs/_service" "$out_dir/_service"

printf '%s\n' "Created OBS sources in $out_dir"
