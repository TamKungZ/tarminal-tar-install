#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
project="${OBS_PROJECT:-home:TamKungZ_}"
package="${OBS_PACKAGE:-tarminal}"
apiurl="${OSC_APIURL:-https://api.opensuse.org}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="$repo_root/obs/dist"
checkout_dir="${OBS_CHECKOUT_DIR:-$repo_root/obs/.osc-work/$project/$package}"

if [ -z "$version" ]; then
  version="$(cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[] | select(.name == "tarminal") | .version' \
    | head -n1)"
fi

if [ -z "$version" ] || [ "$version" = "null" ]; then
  echo "Could not resolve package version. Pass it explicitly: obs/push.sh 0.2.0" >&2
  exit 1
fi

command -v osc >/dev/null 2>&1 || {
  echo "osc is required. Install it first, then run this script again." >&2
  exit 1
}

"$repo_root/obs/make-sources.sh" "$version"

rm -rf "$checkout_dir"
mkdir -p "$(dirname "$checkout_dir")"
osc -A "$apiurl" checkout --output-dir "$checkout_dir" "$project" "$package"

cp "$dist_dir/tarminal.spec" "$checkout_dir/tarminal.spec"
cp "$dist_dir/tarminal.changes" "$checkout_dir/tarminal.changes"
cp "$dist_dir/tarminal-$version.tar.zst" "$checkout_dir/tarminal-$version.tar.zst"
cp "$dist_dir/vendor.tar.zst" "$checkout_dir/vendor.tar.zst"

(
  cd "$checkout_dir"
  osc -A "$apiurl" addremove

  if [ -z "$(osc -A "$apiurl" status)" ]; then
    echo "No OBS source changes to commit."
    exit 0
  fi

  osc -A "$apiurl" commit -m "Update tarminal to $version"
)
