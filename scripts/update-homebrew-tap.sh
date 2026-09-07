#!/usr/bin/env bash
# Updates the Homebrew tap formula for arch-live after a release.
# Run from CI with HOMEBREW_TAP_TOKEN, VERSION, and TAP_REPO set.
set -euo pipefail

VERSION="${VERSION:?VERSION required (e.g. 0.2.0)}"
GITHUB_TOKEN="${HOMEBREW_TAP_TOKEN:?HOMEBREW_TAP_TOKEN required}"
TAP_REPO="${TAP_REPO:-karthikeyasomayajula/homebrew-tap}"
FORMULA_PATH="Formula/arch-live.rb"
BASE_URL="https://github.com/karthikeyasomayajula/archlive/releases/download/v${VERSION}"

ARM_FILE="arch-live-${VERSION}-aarch64-apple-darwin.tar.gz"
X86_FILE="arch-live-${VERSION}-x86_64-apple-darwin.tar.gz"
LINUX_X86_FILE="arch-live-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
LINUX_ARM_FILE="arch-live-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"

echo "Downloading release assets for v${VERSION}..."
curl -fsSL -o arm.tar.gz "${BASE_URL}/${ARM_FILE}"
curl -fsSL -o x86.tar.gz "${BASE_URL}/${X86_FILE}"
curl -fsSL -o linux_x86.tar.gz "${BASE_URL}/${LINUX_X86_FILE}"
curl -fsSL -o linux_arm.tar.gz "${BASE_URL}/${LINUX_ARM_FILE}"

if command -v sha256sum >/dev/null; then
  ARM_SHA=$(sha256sum arm.tar.gz | cut -d' ' -f1)
  X86_SHA=$(sha256sum x86.tar.gz | cut -d' ' -f1)
  LINUX_X86_SHA=$(sha256sum linux_x86.tar.gz | cut -d' ' -f1)
  LINUX_ARM_SHA=$(sha256sum linux_arm.tar.gz | cut -d' ' -f1)
else
  ARM_SHA=$(shasum -a 256 arm.tar.gz | cut -d' ' -f1)
  X86_SHA=$(shasum -a 256 x86.tar.gz | cut -d' ' -f1)
  LINUX_X86_SHA=$(shasum -a 256 linux_x86.tar.gz | cut -d' ' -f1)
  LINUX_ARM_SHA=$(shasum -a 256 linux_arm.tar.gz | cut -d' ' -f1)
fi

echo "SHA256 aarch64-apple-darwin: ${ARM_SHA}"
echo "SHA256 x86_64-apple-darwin:  ${X86_SHA}"
echo "SHA256 x86_64-linux-gnu:     ${LINUX_X86_SHA}"
echo "SHA256 aarch64-linux-gnu:    ${LINUX_ARM_SHA}"

FORMULA=$(cat <<EOF
class ArchLive < Formula
  desc "Zero-config real-time architecture visualizer for Node.js and Bun applications"
  homepage "https://github.com/karthikeyasomayajula/archlive"
  version "${VERSION}"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "${BASE_URL}/${ARM_FILE}"
      sha256 "${ARM_SHA}"
    else
      url "${BASE_URL}/${X86_FILE}"
      sha256 "${X86_SHA}"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "${BASE_URL}/${LINUX_ARM_FILE}"
      sha256 "${LINUX_ARM_SHA}"
    else
      url "${BASE_URL}/${LINUX_X86_FILE}"
      sha256 "${LINUX_X86_SHA}"
    end
  end

  def install
    bin.install "arch-live"
  end

  test do
    system "#{bin}/arch-live", "--help"
  end
end
EOF
)

echo "Fetching current formula SHA from tap repo..."
FILE_META=$(curl -sSL \
  -H "Authorization: token ${GITHUB_TOKEN}" \
  -H "Accept: application/vnd.github.v3+json" \
  "https://api.github.com/repos/${TAP_REPO}/contents/${FORMULA_PATH}" 2>/dev/null || echo '{}')

FILE_SHA=$(echo "$FILE_META" | grep -o '"sha": "[^"]*"' | head -1 | cut -d'"' -f4 || true)

CONTENT=$(printf '%s' "$FORMULA" | base64 | tr -d '\n')

if [ -n "$FILE_SHA" ]; then
  PAYLOAD="{\"message\": \"feat: update arch-live to v${VERSION}\", \"content\": \"${CONTENT}\", \"sha\": \"${FILE_SHA}\"}"
else
  # Formula doesn't exist yet — create it
  PAYLOAD="{\"message\": \"feat: add arch-live formula v${VERSION}\", \"content\": \"${CONTENT}\"}"
fi

echo "Pushing formula update to ${TAP_REPO}..."
curl -fsSL -X PUT \
  -H "Authorization: token ${GITHUB_TOKEN}" \
  -H "Accept: application/vnd.github.v3+json" \
  -H "Content-Type: application/json" \
  "https://api.github.com/repos/${TAP_REPO}/contents/${FORMULA_PATH}" \
  -d "$PAYLOAD"

echo "Homebrew tap updated: ${TAP_REPO}@${FORMULA_PATH} → v${VERSION}"
