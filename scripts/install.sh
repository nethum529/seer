#!/bin/sh

set -u

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

require_executable() {
    [ -f "$1" ] && [ -s "$1" ] && [ -x "$1" ] || fail "The release has an invalid executable: $1. Ask the owner to publish it again."
}

rename_into_place() {
    if [ "$os" = darwin ] && [ -d "$1" ] && [ -d "$2" ]; then
        "$unpacked/seer" --install-exchange "$1" "$2"
    else
        [ ! -d "$2" ] || return 1
        mv -f "$1" "$2"
    fi
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
[ "$os-$arch" != linux-arm64 ] || fail "Linux releases need an x86_64 CPU."

install_dir=${SEER_BIN_DIR:-"$HOME/.local/bin"}
base_url=${SEER_INSTALL_URL:-https://github.com/nethum529/seer-releases/releases/latest/download}
asset="seer-$os-$arch.tar.gz"
temp_dir=$(mktemp -d 2>/dev/null) || fail "Cannot create a temporary directory. Free disk space and try again."
change_count=0
committed=false

cleanup() {
    while [ "$change_count" -gt 0 ]; do
        record="$temp_dir/$change_count"
        destination=$(cat "$record/destination")
        stage=$(cat "$record/stage")
        restored=true
        if [ "$committed" = false ]; then
            if [ -f "$record/changed" ]; then
                if [ -f "$record/backed-up" ]; then
                    rename_into_place "$stage/old" "$destination" || restored=false
                else
                    rm -rf "$destination"
                fi
            fi
        fi
        if [ "$restored" = true ]; then
            rm -rf "$stage"
        else
            printf 'Cannot restore %s. Its backup remains at %s/old.\n' "$destination" "$stage" >&2
        fi
        change_count=$((change_count - 1))
    done
    rm -rf "$temp_dir"
}
trap cleanup 0
trap 'exit 1' 1 2 15

prepare() {
    source=$1
    destination=$2
    parent=$(dirname "$destination")
    mkdir -p "$parent" || fail "Cannot create $parent. Check its permissions."
    stage=$(mktemp -d "$parent/.seer-install.XXXXXX") || fail "Cannot stage $destination. Check its permissions."
    change_count=$((change_count + 1))
    record="$temp_dir/$change_count"
    mkdir "$record" || fail "Cannot prepare the install."
    printf '%s\n' "$destination" > "$record/destination"
    printf '%s\n' "$stage" > "$record/stage"
    cp -R "$source" "$stage/new" || fail "Cannot stage $destination. Check disk space and permissions."
}

replace_files() {
    index=1
    while [ "$index" -le "$change_count" ]; do
        record="$temp_dir/$index"
        destination=$(cat "$record/destination")
        stage=$(cat "$record/stage")
        if [ -e "$destination" ] || [ -L "$destination" ]; then
            cp -Rp "$destination" "$stage/old" || fail "Cannot back up $destination. Check disk space and permissions."
            touch "$record/backed-up" || fail "Cannot record the backup."
        fi
        touch "$record/changed" || fail "Cannot record the install."
        # Issue 362: macOS keeps code signatures by inode. Rename a new file
        # into place so a running binary keeps its old inode and signature.
        rename_into_place "$stage/new" "$destination" || fail "Cannot install $destination. Check its permissions."
        index=$((index + 1))
    done
}

unpacked=${SEER_INSTALL_SOURCE:-"$temp_dir/unpack"}
if [ -z "${SEER_INSTALL_SOURCE:-}" ]; then
    mkdir "$unpacked" || fail "Cannot prepare the install."
    curl -fsSL "${base_url%/}/$asset" -o "$temp_dir/$asset" 2>/dev/null || fail "Cannot download Seer. Check the internet connection and try again."
    tar -xzf "$temp_dir/$asset" -C "$unpacked" 2>/dev/null || fail "Cannot unpack Seer. Try the install command again."
fi

for binary in seer seer-broker seer-runtime; do
    require_executable "$unpacked/$binary"
done

legacy_release=false
if [ ! -e "$unpacked/version" ] && [ ! -e "$unpacked/install.sh" ] &&
    [ ! -e "$unpacked/Seer.app" ] && [ ! -e "$unpacked/seer-desktop" ]; then
    legacy_release=true
fi

if [ "$legacy_release" = true ]; then
    :
elif [ "$os" = darwin ]; then
    app="$unpacked/Seer.app"
    for binary in seer seer-broker seer-runtime seer-desktop; do
        require_executable "$app/Contents/MacOS/$binary"
    done
    [ -s "$app/Contents/Info.plist" ] && [ -s "$app/Contents/Resources/Seer.icns" ] || fail "The release is missing the app metadata or icon."
    prepare "$app" "$HOME/Applications/Seer.app"
else
    require_executable "$unpacked/seer-desktop"
    icon="$unpacked/share/icons/hicolor/scalable/apps/seer.svg"
    [ -s "$icon" ] || fail "The release is missing the Seer icon."
    command -v xdg-mime >/dev/null 2>&1 || fail "Install xdg-utils, then run the install command again."
    data_dir=${XDG_DATA_HOME:-"$HOME/.local/share"}
    prepare "$unpacked/seer-desktop" "$install_dir/seer-desktop"
    prepare "$icon" "$data_dir/icons/hicolor/scalable/apps/seer.svg"
    # Desktop entries escape string values before they parse Exec quoting.
    exec_path=$(printf '%s' "$install_dir/seer-desktop" | sed 's/\\/\\\\\\\\/g; s/"/\\\\"/g; s/`/\\\\`/g; s/\$/\\\\$/g; s/%/%%/g')
    cat > "$temp_dir/seer.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Seer
Comment=Share terminals with your room
Exec="$exec_path" %u
Icon=seer
Terminal=false
Categories=Development;Utility;
MimeType=x-scheme-handler/seer;
StartupWMClass=seer
EOF
    prepare "$temp_dir/seer.desktop" "$data_dir/applications/seer.desktop"
fi

for binary in seer seer-broker seer-runtime; do
    chmod 755 "$unpacked/$binary" || fail "Cannot make Seer executable."
    prepare "$unpacked/$binary" "$install_dir/$binary"
done
if [ -s "$unpacked/version" ]; then
    prepare "$unpacked/version" "${XDG_DATA_HOME:-"$HOME/.local/share"}/seer/version"
fi
replace_files

if [ "$legacy_release" = true ]; then
    :
elif [ "$os" = darwin ]; then
    register=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
    "$register" -f "$HOME/Applications/Seer.app" || fail "Cannot register Seer.app with macOS."
else
    xdg-mime default seer.desktop x-scheme-handler/seer || fail "Cannot register Seer links. Check the desktop settings."
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$data_dir/applications" || fail "Cannot refresh the app list."
    fi
fi
committed=true
cleanup
trap - 0 1 2 15

if [ "${SEER_INSTALL_NO_PATH:-}" != 1 ]; then
    case ":${PATH:-}:" in
        *":$install_dir:"*) ;;
        *)
            case "${SHELL:-}" in
                */zsh) rc_file="$HOME/.zshrc"; path_line='export PATH="$HOME/.local/bin:$PATH"' ;;
                */bash) rc_file="$HOME/.bashrc"; path_line='export PATH="$HOME/.local/bin:$PATH"' ;;
                */fish)
                    rc_file="$HOME/.config/fish/config.fish"
                    mkdir -p "$HOME/.config/fish" || fail "Cannot update the fish config. Add $install_dir to PATH."
                    path_line='set -gx PATH "$HOME/.local/bin" $PATH'
                    ;;
                *) printf '%s\n' "Add $install_dir to PATH to use seer later." ;;
            esac
            if [ -n "${rc_file:-}" ]; then
                printf '%s\n' "$path_line" >> "$rc_file" || fail "Cannot update $rc_file. Add $install_dir to PATH."
                printf '%s\n' "Restart the terminal to use seer later."
            fi
            ;;
    esac
fi

if [ "$#" -gt 0 ]; then
    if (exec < /dev/tty) 2>/dev/null; then
        exec "$install_dir/seer" join "$1" < /dev/tty
    fi
    exec "$install_dir/seer" join "$1"
fi
if [ "$legacy_release" = true ]; then
    printf '%s\n' "Installed Seer. Run seer to open it."
else
    printf '%s\n' "Installed Seer. Open Seer from your applications."
fi
