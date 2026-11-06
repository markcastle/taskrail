#!/bin/sh
# Installs or updates Task Rail on macOS from the latest GitHub Release.
#
#   curl -fsSL https://raw.githubusercontent.com/markcastle/taskrail/main/install.sh | sh
#
# Downloads the universal DMG, copies the app into /Applications, clears the
# quarantine flag that Gatekeeper puts on downloads, and opens it. Set
# GITHUB_TOKEN if the repository is private.

set -eu

REPO="markcastle/taskrail"
APP="Task Rail"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This installer is for macOS. On Windows, run in PowerShell:" >&2
  echo "  irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex" >&2
  exit 1
fi

auth_header=""
if [ -n "${GITHUB_TOKEN:-}" ]; then
  auth_header="Authorization: Bearer $GITHUB_TOKEN"
fi

echo "Looking up the latest $APP release..."
if [ -n "$auth_header" ]; then
  json=$(curl -fsSL -H "$auth_header" "https://api.github.com/repos/$REPO/releases/latest")
else
  json=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
fi || {
  echo "Could not read the latest release of $REPO. If the repository is private, set GITHUB_TOKEN first." >&2
  exit 1
}

tag=$(printf '%s' "$json" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
if [ -n "$auth_header" ]; then
  # Private repositories only hand out assets through the API URL.
  url=$(printf '%s' "$json" | tr -d '\n' | sed 's/},/}\n/g' | grep '\.dmg"' | sed -n 's/.*"url": *"\(https:\/\/api[^"]*\)".*/\1/p' | head -1)
else
  url=$(printf '%s' "$json" | sed -n 's/.*"browser_download_url": *"\([^"]*\.dmg\)".*/\1/p' | head -1)
fi
if [ -z "$url" ]; then
  echo "Release $tag has no macOS build." >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading $APP $tag..."
if [ -n "$auth_header" ]; then
  curl -fL# -H "$auth_header" -H "Accept: application/octet-stream" -o "$tmp/app.dmg" "$url"
else
  curl -fL# -o "$tmp/app.dmg" "$url"
fi

mount=$(hdiutil attach -nobrowse -readonly "$tmp/app.dmg" | awk -F'\t' '/\/Volumes\//{print $NF}')
if [ -z "$mount" ]; then
  echo "Could not open the downloaded disk image." >&2
  exit 1
fi
src=$(find "$mount" -maxdepth 1 -name '*.app' | head -1)

# A running copy would hold the files we are about to replace.
osascript -e "quit app \"$APP\"" >/dev/null 2>&1 || true
sleep 1

if [ -z "$src" ]; then
  echo "The disk image did not contain an app. Contents of $mount:" >&2
  ls -la "$mount" >&2
  hdiutil detach "$mount" -quiet || true
  exit 1
fi

echo "Installing into /Applications..."
rm -rf "/Applications/$APP.app"
cp -R "$src" "/Applications/"
hdiutil detach "$mount" -quiet || true
if [ ! -d "/Applications/$APP.app" ]; then
  echo "Copying into /Applications failed. You can drag the app there yourself from the disk image." >&2
  exit 1
fi

# Downloads carry a quarantine flag, and an unsigned app would be refused on
# first open. Clear it, and give an unsigned build an ad-hoc signature so
# Apple silicon will run it. A build signed with a real certificate is left alone.
xattr -dr com.apple.quarantine "/Applications/$APP.app" 2>/dev/null || true
if ! codesign -dv "/Applications/$APP.app" 2>&1 | grep -q 'Authority='; then
  codesign --force --deep --sign - "/Applications/$APP.app" >/dev/null 2>&1 || true
fi

# By path, not by name: a just-copied app is not always in the launch
# database yet, and "open -a" would say it cannot find it.
if open "/Applications/$APP.app"; then
  echo "$APP $tag is installed and running. It lives in the menu bar; Ctrl+Shift+Space adds a task."
else
  echo "$APP $tag is installed in /Applications. Open it from there; the first time, right-click it and choose Open." >&2
  exit 1
fi
