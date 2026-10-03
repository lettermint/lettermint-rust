#!/usr/bin/env bash
# Set the crate version from a release tag (v1.2.3 or 1.2.3) in Cargo.toml and
# Cargo.lock, then verify that Cargo agrees. The manifest in git is not bumped
# by hand: the git tag is the source of truth for the published version.
set -euo pipefail

tag="${1:-${RELEASE_TAG:-}}"
if [[ -z "$tag" ]]; then
  echo "Usage: set-release-version.sh <tag> (or set RELEASE_TAG)" >&2
  exit 1
fi

version="${tag#v}"
semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?$'
if ! [[ "$version" =~ $semver ]]; then
  echo "Release tag must use 1.2.3 or v1.2.3, with an optional prerelease suffix." >&2
  exit 1
fi

# Cargo.toml: replace only the first `version = "..."` inside [package].
awk -v version="$version" '
  /^\[/ { in_package = ($0 == "[package]") }
  in_package && !done && /^version[[:space:]]*=/ {
    print "version = \"" version "\""
    done = 1
    next
  }
  { print }
  END { if (!done) exit 1 }
' Cargo.toml > Cargo.toml.new || { echo "No [package] version found in Cargo.toml." >&2; rm -f Cargo.toml.new; exit 1; }
mv Cargo.toml.new Cargo.toml

# Cargo.lock: replace the version of the lettermint package entry only.
awk -v version="$version" '
  /^\[\[package\]\]/ { in_entry = 0 }
  /^name = "lettermint"$/ { in_entry = 1 }
  in_entry && !done && /^version = / {
    print "version = \"" version "\""
    done = 1
    next
  }
  { print }
  END { if (!done) exit 1 }
' Cargo.lock > Cargo.lock.new || { echo "No lettermint entry found in Cargo.lock." >&2; rm -f Cargo.lock.new; exit 1; }
mv Cargo.lock.new Cargo.lock

# Verify that Cargo reads the requested version and that the lockfile is still valid.
package_id="$(cargo pkgid --locked)"
package_version="${package_id##*[#@]}"
if [[ "$package_version" != "$version" ]]; then
  echo "Cargo reports version ${package_version}, expected ${version}." >&2
  exit 1
fi

echo "Crate version set to ${version}."
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "version=${version}" >> "$GITHUB_OUTPUT"
fi
