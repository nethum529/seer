#!/bin/sh

set -u

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

os_name=$(uname -s 2>/dev/null) || fail "Cannot detect the operating system. Use macOS or Linux."
case "$os_name" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) fail "Unsupported operating system. Use macOS or Linux." ;;
esac

arch_name=$(uname -m 2>/dev/null) || fail "Cannot detect the CPU type. Use arm64 or x86_64."
case "$arch_name" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64 | amd64) arch=x86_64 ;;
    *) fail "Unsupported CPU type. Use arm64 or x86_64." ;;
esac

install_dir="$HOME/.local/bin"
base_url=${SEER_INSTALL_URL:-https://github.com/nethum529/seer-releases/releases/latest/download}
asset="seer-$os-$arch.tar.gz"

mkdir -p "$install_dir" 2>/dev/null || fail "Cannot create $install_dir. Check its permissions and run the install command again."
temp_dir=$(mktemp -d 2>/dev/null) || fail "Cannot create a temporary directory. Free disk space and run the install command again."
cleanup() {
    rm -rf "$temp_dir"
}
trap cleanup 0

archive="$temp_dir/$asset"
mkdir "$temp_dir/unpack" 2>/dev/null || fail "Cannot prepare the install. Run the install command again."
curl -fsSL "${base_url%/}/$asset" -o "$archive" 2>/dev/null || fail "Cannot download seer. Check the internet connection and run the install command again."
tar -xzf "$archive" -C "$temp_dir/unpack" 2>/dev/null || fail "Cannot unpack seer. Run the install command again."

for binary in seer seer-broker seer-runtime; do
    [ -f "$temp_dir/unpack/$binary" ] || fail "The release is incomplete. Ask the owner to publish all seer binaries."
    cp "$temp_dir/unpack/$binary" "$install_dir/$binary" 2>/dev/null || fail "Cannot install seer in $install_dir. Check its permissions and run the install command again."
    chmod 755 "$install_dir/$binary" 2>/dev/null || fail "Cannot make seer executable. Check $install_dir permissions and run the install command again."
done

cleanup
trap - 0

case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
        case "${SHELL:-}" in
            */zsh)
                rc_file="$HOME/.zshrc"
                path_line='export PATH="$HOME/.local/bin:$PATH"'
                ;;
            */bash)
                rc_file="$HOME/.bashrc"
                path_line='export PATH="$HOME/.local/bin:$PATH"'
                ;;
            */fish)
                rc_file="$HOME/.config/fish/config.fish"
                mkdir -p "$HOME/.config/fish" 2>/dev/null || fail "Cannot update the fish config. Add $install_dir to PATH."
                path_line='set -gx PATH "$HOME/.local/bin" $PATH'
                ;;
            *) printf '%s\n' "Add $install_dir to PATH to use seer later." ;;
        esac
        if [ -n "${rc_file:-}" ]; then
            printf '%s\n' "$path_line" >> "$rc_file" 2>/dev/null || fail "Cannot update $rc_file. Add $install_dir to PATH."
            printf '%s\n' "Restart the terminal to use seer later."
        fi
        ;;
esac

if [ "$#" -gt 0 ]; then
    if [ -r /dev/tty ]; then
        exec "$install_dir/seer" join "$1" < /dev/tty
    fi
    exec "$install_dir/seer" join "$1"
fi

printf '%s\n' "Installed seer. Run: seer join <capsule>"
