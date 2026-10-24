#!/usr/bin/env bash
# Prints the next release version (without the leading "v").
#
#   next-version.sh <bump> <latest-tag> <cargo-version>
#
# <bump> is patch, minor or major; <latest-tag> is the newest vX.Y.Z tag (empty
# when there is none). A Cargo.toml version above the latest tag wins, so a
# release can still be chosen by hand, e.g. 1.0.0 for a milestone.

set -euo pipefail

bump=$1
latest=${2#v}
cargo=$3

semver='^[0-9]+\.[0-9]+\.[0-9]+$'
[[ $cargo =~ $semver ]] || { echo "Cargo.toml version '$cargo' is not X.Y.Z" >&2; exit 1; }
if [[ -z $latest ]]; then
    echo "$cargo"
    exit 0
fi
[[ $latest =~ $semver ]] || { echo "latest tag 'v$latest' is not vX.Y.Z" >&2; exit 1; }

# Zero-padded so plain string comparison orders versions.
key() { IFS=. read -r a b c <<< "$1"; printf '%06d%06d%06d' "$a" "$b" "$c"; }
if [[ $(key "$cargo") > $(key "$latest") ]]; then
    echo "$cargo"
    exit 0
fi

IFS=. read -r major minor patch <<< "$latest"
case $bump in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    patch) echo "$major.$minor.$((patch + 1))" ;;
    *) echo "unknown bump '$bump' (expected patch, minor or major)" >&2; exit 1 ;;
esac
