#!/usr/bin/env bash
# Tests the workspace against every supported arrow version.
#
# Keep the version list below in sync with the `arrow` version
# requirement in the root Cargo.toml.

set -eu
cd "$(dirname "$0")/.."

# The floor of every supported arrow major.
# The locked (newest) version is tested last.
ARROW_VERSIONS=("57.0.0" "58.0.0")

backup="$(mktemp)"
cp Cargo.lock "$backup"
restore() { cp "$backup" Cargo.lock; }
trap restore EXIT

for version in "${ARROW_VERSIONS[@]}"; do
    echo "=== arrow ${version} ==="
    cargo update --package arrow --precise "$version" --quiet
    cargo test --all-features --quiet
done

restore
echo "=== arrow (locked) ==="
cargo test --all-features --quiet

echo "All supported arrow versions pass."
