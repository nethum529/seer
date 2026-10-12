#!/usr/bin/env bash

set -euo pipefail

if [[ $# != 4 ]]; then
    printf 'Usage: %s <platform> <binary-directory> <output-directory> <version>\n' "${0##*/}" >&2
    exit 2
fi
platform=$1
binaries=$(cd "$2" && pwd)
mkdir -p "$3"
output=$(cd "$3" && pwd)
version=$4
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf 'Use a numeric release version.\n' >&2; exit 2; }
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

for binary in seer seer-broker seer-runtime seer-desktop; do
    [[ -x "$binaries/$binary" ]] || { printf 'Missing executable: %s\n' "$binary" >&2; exit 1; }
done
cp "$root/scripts/install.sh" "$stage/install.sh"
printf '%s\n' "$version" > "$stage/version"

make_icon() {
    local iconset="$stage/Seer.iconset"
    local pixels
    mkdir "$iconset"
    for pixels in 16 32 128 256 512; do
        sips -z "$pixels" "$pixels" "$root/scripts/desktop/seer.png" --out "$iconset/icon_${pixels}x${pixels}.png" >/dev/null
        sips -z "$((pixels * 2))" "$((pixels * 2))" "$root/scripts/desktop/seer.png" --out "$iconset/icon_${pixels}x${pixels}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$stage/Seer.app/Contents/Resources/Seer.icns"
    rm -rf "$iconset"
}

sign_macos() {
    local identity=${SEER_SIGNING_IDENTITY:--}
    local options=(--force --sign "$identity")
    local binary
    if [[ "$identity" != - ]]; then
        [[ "$identity" == "Developer ID Application:"* ]] || { printf 'Use a Developer ID Application identity.\n' >&2; exit 1; }
        options+=(--options runtime --timestamp)
    fi
    for binary in seer seer-broker seer-runtime seer-desktop; do
        codesign "${options[@]}" "$stage/Seer.app/Contents/MacOS/$binary"
    done
    codesign "${options[@]}" "$stage/Seer.app"
    codesign --verify --deep --strict "$stage/Seer.app"
    if [[ "$platform" == "darwin-$(uname -m)" ]]; then
        local output
        output=$("$stage/Seer.app/Contents/MacOS/seer-runtime" 2>&1 || true)
        [[ "$output" == *"usage: seer-runtime"* ]] || { printf 'The signed runtime did not start: %s\n' "$output" >&2; exit 1; }
        "$stage/Seer.app/Contents/MacOS/seer" help >/dev/null
    fi
    for binary in seer seer-broker seer-runtime; do
        cp "$stage/Seer.app/Contents/MacOS/$binary" "$stage/$binary"
    done
}

notarize_macos() {
    [[ -n "${SEER_NOTARY_KEY:-}" ]] || return 0
    [[ "${SEER_SIGNING_IDENTITY:--}" != - ]] || { printf 'Notarization needs Developer ID signing.\n' >&2; exit 1; }
    ditto -c -k --keepParent "$stage/Seer.app" "$stage/notary.zip"
    xcrun notarytool submit "$stage/notary.zip" \
        --key "$SEER_NOTARY_KEY" --key-id "$SEER_NOTARY_KEY_ID" \
        --issuer "$SEER_NOTARY_ISSUER_ID" --wait --output-format json > "$stage/notary.json"
    if [[ $(plutil -extract status raw "$stage/notary.json") != Accepted ]]; then
        cat "$stage/notary.json" >&2
        exit 1
    fi
    xcrun stapler staple "$stage/Seer.app"
    xcrun stapler validate "$stage/Seer.app"
    spctl --assess --type execute --verbose=2 "$stage/Seer.app"
    rm "$stage/notary.zip" "$stage/notary.json"
}

case "$platform" in
    darwin-arm64 | darwin-x86_64)
        mkdir -p "$stage/Seer.app/Contents/MacOS" "$stage/Seer.app/Contents/Resources"
        for binary in seer seer-broker seer-runtime seer-desktop; do
            cp "$binaries/$binary" "$stage/Seer.app/Contents/MacOS/$binary"
        done
        sed "s/@VERSION@/$version/g" "$root/scripts/desktop/Info.plist" > "$stage/Seer.app/Contents/Info.plist"
        plutil -lint "$stage/Seer.app/Contents/Info.plist"
        make_icon
        sign_macos
        notarize_macos
        ;;
    linux-x86_64)
        for binary in seer seer-broker seer-runtime seer-desktop; do
            cp "$binaries/$binary" "$stage/$binary"
        done
        mkdir -p "$stage/share/icons/hicolor/scalable/apps"
        cp "$root/scripts/desktop/seer.svg" "$stage/share/icons/hicolor/scalable/apps/seer.svg"
        ;;
    *) printf 'Unsupported platform: %s\n' "$platform" >&2; exit 2 ;;
esac
tar -C "$stage" -czf "$output/seer-$platform.tar.gz" .
printf 'Created %s/seer-%s.tar.gz\n' "$output" "$platform"
