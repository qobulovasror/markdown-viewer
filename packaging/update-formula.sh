#!/usr/bin/env bash
# Rewrites the Homebrew formula for a release: url versions and sha256 values.
# Usage: update-formula.sh <tag, e.g. v0.2.0> <SHA256SUMS> <formula.rb>
set -euo pipefail
tag=$1 sums=$2 formula=$3

# Point every url at the new tag.
sed -E -i.bak "s#/download/v[^/]+/mdvw-v[^-]+-#/download/${tag}/mdvw-${tag}-#" "$formula"
rm -f "$formula.bak"

# Each sha256 line follows its url; take the hash for that file from SHA256SUMS.
awk -v sums="$sums" '
  BEGIN { while ((getline line < sums) > 0) { split(line, f, /  */); hash[f[2]] = f[1] } }
  /url "/ { n = split($0, p, "/"); file = p[n]; sub(/".*/, "", file) }
  /sha256 "/ && file != "" {
    if (!(file in hash)) { print "missing checksum for " file > "/dev/stderr"; exit 1 }
    sub(/sha256 "[0-9a-f]*"/, "sha256 \"" hash[file] "\""); file = ""
  }
  { print }
' "$formula" > "$formula.new"
mv "$formula.new" "$formula"
