#!/usr/bin/env bash
# Updates version strings in README.md after a release (run from CI).
set -euo pipefail

VERSION="${1:?Usage: sync-readme-version.sh <semver without v, e.g. 0.2.0>}"
TAG="v${VERSION}"
README="${2:-README.md}"

if [ ! -f "$README" ]; then
  echo "README not found: $README" >&2
  exit 1
fi

OLD_TAG=$(grep -oE 'archlive-version:v[0-9]+\.[0-9]+\.[0-9]+' "$README" | head -1 | cut -d: -f2)
if [ -z "$OLD_TAG" ]; then
  echo "Could not find <!-- archlive-version:vX.Y.Z --> in ${README}" >&2
  exit 1
fi
OLD_VER="${OLD_TAG#v}"

perl -pi -e "s/<!-- archlive-version:.*? -->/<!-- archlive-version:${TAG} -->/" "$README"
perl -pi -e "s/VERSION=${OLD_VER}/VERSION=${VERSION}/g" "$README"
perl -pi -e "s|releases/download/${OLD_TAG}/|releases/download/${TAG}/|g" "$README"
perl -pi -e "s/arch-live-${OLD_VER}-/arch-live-${VERSION}-/g" "$README"
perl -pi -e "s/git tag v\\d+\\.\\d+\\.\\d+/git tag ${TAG}/" "$README"

echo "Updated ${README}: ${OLD_VER} → ${VERSION}"
