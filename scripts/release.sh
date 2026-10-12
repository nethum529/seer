#!/usr/bin/env bash

set -euo pipefail

readonly RELEASE_REPOSITORY="nethum529/seer-releases"
readonly RELEASE_REPOSITORY_URL="https://github.com/${RELEASE_REPOSITORY}.git"

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

check_release_secrets() {
    local secrets
    local name
    if [[ "${dry_run}" == true ]]; then
        print_command gh secret list --repo "${RELEASE_REPOSITORY}"
        return
    fi
    secrets=$(gh secret list --repo "${RELEASE_REPOSITORY}" | cut -f1)
    for name in SEER_SOURCE_DEPLOY_KEY SEER_DEVELOPER_ID_P12_BASE64 \
        SEER_DEVELOPER_ID_PASSWORD SEER_NOTARY_KEY_P8_BASE64 \
        SEER_NOTARY_KEY_ID SEER_NOTARY_ISSUER_ID; do
        if ! grep -Fxq "$name" <<< "$secrets"; then
            printf 'Add %s to %s. See ADR 0011.\n' "$name" "${RELEASE_REPOSITORY}" >&2
            exit 1
        fi
    done
}

check_release_files() {
    local checkout=$1
    if [[ "${dry_run}" == true ]]; then
        print_command cmp -s scripts/install.sh "${checkout}/install.sh"
        print_command cmp -s scripts/seer-releases/macos-build.yml "${checkout}/.github/workflows/macos-build.yml"
        return
    fi
    if ! cmp -s scripts/install.sh "${checkout}/install.sh" || \
        ! cmp -s scripts/seer-releases/macos-build.yml "${checkout}/.github/workflows/macos-build.yml"; then
        printf 'Update install.sh and .github/workflows/macos-build.yml in seer-releases first. See ADR 0011.\n' >&2
        exit 1
    fi
}

run_desktop_build() {
    local tag=$1
    local source_ref
    if [[ "${dry_run}" == true ]]; then
        print_command git rev-parse HEAD
        source_ref="<git-rev-parse-HEAD>"
    else
        source_ref=$(git rev-parse HEAD)
    fi
    run gh workflow run macos-build.yml \
        --repo "${RELEASE_REPOSITORY}" -f "tag=${tag}" -f "ref=${source_ref}"
    if [[ "${dry_run}" == true ]]; then
        print_command gh run list --repo "${RELEASE_REPOSITORY}" \
            --workflow macos-build.yml --json databaseId,displayTitle
        print_command gh run watch "<matching-run-id>" --repo "${RELEASE_REPOSITORY}" --exit-status
        return
    fi
    local run_id=""
    local attempt=0
    local title="Desktop ${tag} from ${source_ref}"
    while [[ "$attempt" -lt 30 ]]; do
        attempt=$((attempt + 1))
        run_id=$(gh run list --repo "${RELEASE_REPOSITORY}" --workflow macos-build.yml \
            --limit 20 --json databaseId,displayTitle | \
            jq -r --arg title "$title" '[.[] | select(.displayTitle == $title)][0].databaseId // empty')
        [[ -z "$run_id" ]] || break
        sleep 2
    done
    if [[ -z "$run_id" ]] || ! gh run watch "$run_id" --repo "${RELEASE_REPOSITORY}" --exit-status; then
        printf 'Desktop build failed. The release stays a draft. See the run in seer-releases.\n' >&2
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
    script_directory=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
    repository_root=$(git -C "${script_directory}/.." rev-parse --show-toplevel)
    cd "${repository_root}"

    check_source_tree
    check_release_secrets

    local version
    version=$(read_version)
    if [[ -z "${version}" ]]; then
        printf 'Cannot read the workspace version from Cargo.toml.\n' >&2
        exit 1
    fi

    local tag="v${version}"
    check_release_tag "${tag}"

    work_directory=""
    if [[ "${dry_run}" == true ]]; then
        work_directory="${TMPDIR:-/tmp}/seer-release-XXXXXX"
        print_command mktemp -d
    else
        work_directory=$(mktemp -d)
        trap 'rm -rf -- "${work_directory}"' EXIT
    fi

    local release_checkout="${work_directory}/seer-releases"
    run git clone --branch main --single-branch "${RELEASE_REPOSITORY_URL}" "${release_checkout}"
    check_release_files "${release_checkout}"

    run gh release create "${tag}" \
        --repo "${RELEASE_REPOSITORY}" \
        --title "${tag}" \
        --generate-notes \
        --draft

    run_desktop_build "${tag}"
}

main "$@"
