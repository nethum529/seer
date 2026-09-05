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

check_source_deploy_key() {
    if [[ "${dry_run}" == true ]]; then
        print_command gh secret list --repo "${RELEASE_REPOSITORY}"
        return
    fi

    if ! gh secret list --repo "${RELEASE_REPOSITORY}" | cut -f1 | grep -Fxq SEER_SOURCE_DEPLOY_KEY; then
        printf 'Add the SEER_SOURCE_DEPLOY_KEY secret to %s, see issue 179\n' \
            "${RELEASE_REPOSITORY}" >&2
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

write_release_notes() {
    local target=$1
    local notes_file=$2
    local binary
    local bytes
    local crate_count

    if [[ "${dry_run}" == true ]]; then
        print_command cargo tree --workspace --prefix none --format '{p}' --no-dedupe
        for binary in "${BINARIES[@]}"; do
            print_command wc -c "target/${target}/release/${binary}"
        done
        printf '+ write binary sizes and crate count to %q\n' "${notes_file}"
        return
    fi

    crate_count=$(cargo tree --workspace --prefix none --format '{p}' --no-dedupe | sort -u | wc -l | tr -d '[:space:]')
    printf 'Build size for %s:\n\n' "${target}" > "${notes_file}"
    for binary in "${BINARIES[@]}"; do
        bytes=$(wc -c < "target/${target}/release/${binary}" | tr -d '[:space:]')
        printf '%s: %s bytes\n' "${binary}" "${bytes}" >> "${notes_file}"
    done
    printf '\nCrate count from cargo tree: %s\n' "${crate_count}" >> "${notes_file}"
    cat "${notes_file}"
}

update_release_files() {
    local checkout=$1
    local workflow_directory="${checkout}/.github/workflows"
    local workflow_file="${workflow_directory}/macos-build.yml"

    if [[ "${dry_run}" == true ]]; then
        print_command cp scripts/install.sh "${checkout}/install.sh"
        print_command mkdir -p "${workflow_directory}"
        print_command cp scripts/seer-releases/macos-build.yml "${workflow_file}"
        printf '+ if release files changed in %q\n' "${checkout}"
        print_command git -C "${checkout}" add install.sh .github/workflows/macos-build.yml
        print_command git -C "${checkout}" commit -s -m "Update release files"
        print_command git -C "${checkout}" push origin main
        return
    fi

    mkdir -p "${workflow_directory}"
    cp scripts/install.sh "${checkout}/install.sh"
    cp scripts/seer-releases/macos-build.yml "${workflow_file}"
    if [[ -z "$(git -C "${checkout}" status --porcelain -- \
        install.sh .github/workflows/macos-build.yml)" ]]; then
        return
    fi

    git -C "${checkout}" add install.sh .github/workflows/macos-build.yml
    git -C "${checkout}" commit -s -m "Update release files"
    git -C "${checkout}" push origin main
}

run_macos_build() {
    local tag=$1
    local source_ref

    if [[ "${dry_run}" == true ]]; then
        print_command git rev-parse HEAD
        source_ref="<git-rev-parse-HEAD>"
    else
        source_ref=$(git rev-parse HEAD)
    fi

    run gh workflow run macos-build.yml \
        --repo "${RELEASE_REPOSITORY}" \
        -f "tag=${tag}" \
        -f "ref=${source_ref}"

    if [[ "${dry_run}" == true ]]; then
        print_command gh run list \
            --repo "${RELEASE_REPOSITORY}" \
            --workflow macos-build.yml \
            --limit 1 \
            --json databaseId \
            --jq '.[0].databaseId'
        print_command gh run watch "<run-id>" --repo "${RELEASE_REPOSITORY}" --exit-status
        return
    fi

    local run_id
    run_id=$(gh run list \
        --repo "${RELEASE_REPOSITORY}" \
        --workflow macos-build.yml \
        --limit 1 \
        --json databaseId \
        --jq '.[0].databaseId')
    if ! gh run watch "${run_id}" --repo "${RELEASE_REPOSITORY}" --exit-status; then
        printf 'macOS build failed, see the run in seer-releases\n' >&2
        exit 1
    fi
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
    check_source_deploy_key

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

    local linux_asset="seer-linux-x86_64.tar.gz"
    make_archive x86_64-unknown-linux-gnu "${linux_asset}" "${assets_directory}"

    run git clone --branch main --single-branch "${RELEASE_REPOSITORY_URL}" "${release_checkout}"
    update_release_files "${release_checkout}"

    local notes_file="${work_directory}/release-notes.txt"
    write_release_notes x86_64-unknown-linux-gnu "${notes_file}"

    run gh release create "${tag}" \
        "${assets_directory}/${linux_asset}" \
        --repo "${RELEASE_REPOSITORY}" \
        --title "${tag}" \
        --generate-notes \
        --notes-file "${notes_file}"

    run_macos_build "${tag}"
}

main "$@"
