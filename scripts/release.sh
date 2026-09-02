#!/usr/bin/env bash

set -euo pipefail

readonly RELEASE_REPOSITORY="nethum529/seer-releases"
readonly RELEASE_REPOSITORY_URL="https://github.com/${RELEASE_REPOSITORY}.git"
readonly BINARIES=(seer seer-broker seer-runtime)

dry_run=false

usage() {
    printf 'Usage: %s [--dry-run]\n' "${0##*/}" >&2
}

print_command() {
    printf '+'
    printf ' %q' "$@"
    printf '\n'
}

run() {
    if [[ "${dry_run}" == true ]]; then
        print_command "$@"
        return
    fi

    "$@"
}

read_version() {
    awk '
        /^\[workspace\.package\][[:space:]]*$/ {
            in_workspace_package = 1
            next
        }
        in_workspace_package && /^\[/ { exit }
        in_workspace_package && /^[[:space:]]*version[[:space:]]*=/ {
            value = $0
            sub(/^[^=]*=[[:space:]]*"/, "", value)
            sub(/".*$/, "", value)
            print value
            exit
        }
    ' Cargo.toml
}

check_source_tree() {
    if [[ -n "$(git status --porcelain)" ]]; then
        printf 'The source repository has uncommitted changes.\n' >&2
        exit 1
    fi
}

check_release_tag() {
    local tag=$1
    local tag_ref="refs/tags/${tag}"
    local matching_tag

    if [[ "${dry_run}" == true ]]; then
        print_command git ls-remote --exit-code --tags "${RELEASE_REPOSITORY_URL}" "${tag_ref}"
        return
    fi

    if ! matching_tag=$(git ls-remote --tags "${RELEASE_REPOSITORY_URL}" "${tag_ref}"); then
        printf 'Cannot check tags in %s.\n' "${RELEASE_REPOSITORY}" >&2
        exit 1
    fi

    if [[ -n "${matching_tag}" ]]; then
        printf 'Tag %s already exists in %s.\n' "${tag}" "${RELEASE_REPOSITORY}" >&2
        exit 1
    fi
}

build_target() {
    local builder=$1
    local target=$2

    run cargo "${builder}" --release --package seer --target "${target}"
}

make_archive() {
    local target=$1
    local asset=$2
    local output_directory=$3

    run tar -C "target/${target}/release" -czf "${output_directory}/${asset}" "${BINARIES[@]}"
}

update_install_script() {
    local checkout=$1

    if [[ "${dry_run}" == true ]]; then
        print_command cp scripts/install.sh "${checkout}/install.sh"
        printf '+ if install.sh changed in %q\n' "${checkout}"
        print_command git -C "${checkout}" add install.sh
        print_command git -C "${checkout}" commit -s -m "Update install script"
        print_command git -C "${checkout}" push origin main
        return
    fi

    if cmp -s scripts/install.sh "${checkout}/install.sh"; then
        return
    fi

    cp scripts/install.sh "${checkout}/install.sh"
    git -C "${checkout}" add install.sh
    git -C "${checkout}" commit -s -m "Update install script"
    git -C "${checkout}" push origin main
}

parse_arguments() {
    if [[ $# -eq 0 ]]; then
        return
    fi

    if [[ $# -eq 1 && $1 == "--dry-run" ]]; then
        dry_run=true
        return
    fi

    usage
    exit 2
}

main() {
    parse_arguments "$@"

    local script_directory
    local repository_root
    script_directory=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
    repository_root=$(git -C "${script_directory}/.." rev-parse --show-toplevel)
    cd "${repository_root}"

    check_source_tree

    local version
    version=$(read_version)
    if [[ -z "${version}" ]]; then
        printf 'Cannot read the workspace version from Cargo.toml.\n' >&2
        exit 1
    fi

    local tag="v${version}"
    check_release_tag "${tag}"

    local work_directory
    if [[ "${dry_run}" == true ]]; then
        work_directory="${TMPDIR:-/tmp}/seer-release-XXXXXX"
        print_command mktemp -d
    else
        work_directory=$(mktemp -d)
        local cleanup_command
        printf -v cleanup_command 'rm -rf -- %q' "${work_directory}"
        trap "${cleanup_command}" EXIT
    fi

    local assets_directory="${work_directory}/assets"
    local release_checkout="${work_directory}/seer-releases"
    run mkdir -p "${assets_directory}"

    build_target build x86_64-unknown-linux-gnu
    if [[ "${SEER_MACOS_BUILD:-}" == 1 ]]; then
        build_target zigbuild aarch64-apple-darwin
        build_target zigbuild x86_64-apple-darwin
    else
        printf 'Skipping macOS targets, see issue 172\n'
    fi

    local linux_asset="seer-linux-x86_64.tar.gz"
    local darwin_arm_asset="seer-darwin-arm64.tar.gz"
    local darwin_x86_asset="seer-darwin-x86_64.tar.gz"
    make_archive x86_64-unknown-linux-gnu "${linux_asset}" "${assets_directory}"

    local release_assets=("${assets_directory}/${linux_asset}")
    if [[ "${SEER_MACOS_BUILD:-}" == 1 ]]; then
        make_archive aarch64-apple-darwin "${darwin_arm_asset}" "${assets_directory}"
        make_archive x86_64-apple-darwin "${darwin_x86_asset}" "${assets_directory}"
        release_assets+=(
            "${assets_directory}/${darwin_arm_asset}"
            "${assets_directory}/${darwin_x86_asset}"
        )
    fi

    run git clone --branch main --single-branch "${RELEASE_REPOSITORY_URL}" "${release_checkout}"
    update_install_script "${release_checkout}"

    run gh release create "${tag}" \
        "${release_assets[@]}" \
        --repo "${RELEASE_REPOSITORY}" \
        --title "${tag}" \
        --generate-notes
}

main "$@"
