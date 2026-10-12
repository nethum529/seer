# ADR 0011: Install and update the desktop app with the CLI

Date: 2026-10-12
Status: Accepted

Supersedes the archive layout, local Linux build, release publication order,
and release file push steps in ADRs 0001 and 0003. ADR 0010 keeps the TUI
and CLI commands during the desktop move.

## Formats and paths

One archive ships the app, CLI, broker, runtime, icon, version, and installer
from one source commit. Keep the existing asset names:

- seer-darwin-arm64.tar.gz
- seer-darwin-x86_64.tar.gz
- seer-linux-x86_64.tar.gz

macOS ships Seer.app. The install line puts it in ~/Applications, without
sudo. Its Info.plist names Seer, its icon, and the seer URL scheme. The
installer registers it with Launch Services. Launchpad, Spotlight, Finder,
and the Dock can open the bundle. The bundle contains all four executables
in Contents/MacOS, so runtime and broker lookup works without a terminal
PATH. Fonts and Metal shaders are already embedded by Seer and GPUI.

Linux ships native ELF binaries in a tar archive. The installer puts the
four executables in ~/.local/bin. It adds seer.desktop under
$XDG_DATA_HOME/applications and the SVG icon under
$XDG_DATA_HOME/icons/hicolor/scalable/apps. XDG_DATA_HOME defaults to
~/.local/share. The entry starts the app with an absolute, escaped Exec
path and %u. It names x-scheme-handler/seer. xdg-mime sets that handler.
The GPUI app_id and StartupWMClass are both seer.

Linux x86_64 needs glibc 2.35 or newer, xdg-utils, the system Fontconfig,
FreeType, XCB, and xkbcommon libraries, and a graphics driver that GPUI can
use. The build uses Ubuntu 22.04. X11 and Wayland remain enabled. macOS
builds set MACOSX_DEPLOYMENT_TARGET to 12.0. This ticket adds no Linux arm64
asset, DMG, PKG, DEB, RPM, Flatpak, or AppImage. App stores and Windows stay
out of scope.

The CLI binaries also install in ~/.local/bin on macOS. Bare seer still
opens the TUI under ADR 0010. Open the app from the desktop launcher. A
later TUI removal ticket can start ~/Applications/Seer.app on macOS or
~/.local/bin/seer-desktop on Linux. T-455 handles opened links inside the
app. Its invite format is seer://join/CAPSULE, with the bare SEER2 capsule
after join/. This ticket registers the seer scheme for that format. The
OS registrations accept any seer URL; T-455 validates its path and capsule.

## Install and update

The public install line stays the same. With an invite argument, it keeps
the existing seer join flow. Without an argument, it installs the desktop
app and tells the person to open Seer from their applications.
During the first release build, older CLI-only archives remain installable
through this line. Only an archive without version, install.sh, Seer.app,
and seer-desktop uses that old layout. This keeps the public line usable
after its script is updated but before the first desktop draft is published.
The new updater requires the desktop release layout; it does not downgrade
to a CLI-only archive.

seer update downloads one versioned archive and runs that archive's
installer against its unpacked files. It does not fetch a second latest
archive or installer. App and CLI therefore come from the same release.
The installed version record also detects a missing or older app when the
CLI version is already current. SEER_UPDATE_URL supports a local release
mirror, like SEER_INSTALL_URL.

The installer checks required payload files before it replaces anything.
It stages new files beside their destinations, keeps the old files until
registration succeeds, and restores them on a failure. Each executable
gets a new inode, as issue 362 requires. Existing windows, brokers,
runtimes, and shells keep running on their old binaries.

After update, close and open the desktop app to use its new executable.
seer restart keeps its existing confirmation and --yes option. It ends
local shells and runtimes; opening Seer again restores the saved layout
with new shells on the installed runtime. It does not update files or
restart another person's shells. Keep it a command in this ticket. The
Update banner and its action belong to R-458.

## Release flow

scripts/seer-releases/macos-build.yml remains the workflow path in
nethum529/seer-releases under ADR 0003. Its new display name is Desktop
build. It builds Linux and both Mac architectures in separate jobs. Both
jobs check out the same source ref with SEER_SOURCE_DEPLOY_KEY, run fmt
and clippy, build both seer and seer-desktop with Rust 1.98.0 and the
lockfile, and check install and update. scripts/package-desktop.sh creates
each archive.
Rust 1.99 deprecates an unchanged seer-net API. Pinning 1.98 keeps release
gates repeatable until that API is updated in a separate ticket.

scripts/release.sh checks the required secret names and makes a draft
release. It dispatches the workflow with the tag and exact source HEAD,
then waits for the run with that tag and ref. A final workflow job
publishes the draft only after both build jobs upload their assets. A
failed build leaves a draft that cannot become the latest install target.
Fix the fault and rerun the workflow for that draft and source ref.

Release files stay in this source repo. Before releasing, the coordinator
must apply these two files through a branch and PR in seer-releases:

- scripts/install.sh -> install.sh
- scripts/seer-releases/macos-build.yml -> .github/workflows/macos-build.yml

release.sh checks that these copies match. It does not commit or push to
the default branch of either repository.

After T-455 and this ticket are merged, set the workspace package version
to an unused numeric release version. Put that source commit in GitHub.
From a clean checkout of that commit, run scripts/release.sh with gh
authenticated for seer-releases. Check the Desktop build run and all three
assets before the clean-machine checks below. To retry a failed draft,
dispatch macos-build.yml with its existing tag and source ref; release.sh
refuses a tag that already exists.

## Apple signing and notarization

The owner confirmed the Apple Developer Program team on 2026-10-12:
DON NETHUM WEERASINGHE, NYKPX446L9. The Developer ID Application certificate
now exists in the Mac login keychain as Developer ID Application:
DON NETHUM WEERASINGHE (NYKPX446L9). Apple Distribution and Apple Development
identities do not replace this certificate for this distribution path.

The coordinator confirmed that team API key LYLB48Z5NR works for
notarization. Its issuer is 4a5d34a4-7e9b-426a-b4f1-fd92a5e2c9c7. The API
refused Developer ID certificate creation. The Account Holder used the
web portal with the CSR for ~/.seer-signing/devid.key and downloaded
~/.seer-signing/devid.cer. The P12 is ~/.seer-signing/devid.p12; its password
is in ~/.seer-signing/p12-password. The coordinator tested codesign with
--options runtime --timestamp. The notary API key uploads successfully;
the first notarization was still in progress when this was recorded.
Agents must not read or copy files in ~/.seer-signing.

The owner must do these steps:

1. Keep the completed Developer ID setup. Do not create another certificate
   or export another P12. Use ~/.seer-signing/devid.p12 and the password
   already saved in ~/.seer-signing/p12-password.
2. Use the saved private API key AuthKey_LYLB48Z5NR.p8 for the tested team
   key. App Store Connect permits only one download of a key. If the
   saved file is lost, the owner must replace the key and update both its
   secret and Key ID. Do not make a new key when this saved key is present.
3. In nethum529/seer-releases, open Settings, Secrets and variables,
   Actions. Add the five repository secrets below. Keep the existing
   read-only SEER_SOURCE_DEPLOY_KEY for source checkout. Run the base64
   commands on the owner's Mac, then paste the clipboard into each secret.

| Secret | Exact value |
| --- | --- |
| SEER_DEVELOPER_ID_P12_BASE64 | base64 -i ~/.seer-signing/devid.p12 \| pbcopy |
| SEER_DEVELOPER_ID_PASSWORD | The password in ~/.seer-signing/p12-password, without the file's trailing newline. Copy it with printf '%s' "$(cat ~/.seer-signing/p12-password)" \| pbcopy. |
| SEER_NOTARY_KEY_P8_BASE64 | base64 -i /path/to/AuthKey_LYLB48Z5NR.p8 \| pbcopy. Replace /path/to with the owner's saved key location. |
| SEER_NOTARY_KEY_ID | LYLB48Z5NR |
| SEER_NOTARY_ISSUER_ID | 4a5d34a4-7e9b-426a-b4f1-fd92a5e2c9c7 |

The workflow creates a temporary keychain with a random password, imports
the P12, grants codesign access, and selects only a Developer ID Application
identity for NYKPX446L9. It signs each executable, then the enclosing app,
with hardened runtime and a secure timestamp. It verifies the bundle.
No certificate or API key goes into an archive or upload.

For each architecture, the packager makes a ZIP of the signed bundle and
runs xcrun notarytool submit with --key, --key-id, --issuer, and --wait. It
requires Accepted status, staples the ticket to Seer.app, validates the
ticket, and checks spctl before it makes the release tar archive. The
workflow removes its keychain and API key even after a failure.

Production releases require all secrets. There is no unsigned fallback in
GitHub Actions. Local packaging defaults to ad hoc signing for development
checks when SEER_SIGNING_IDENTITY is absent. A local ad hoc bundle does not
prove Gatekeeper or notarization. No agent creates a certificate or changes
the owner's signing keychain.

Apple's requirements and certificate steps:
[Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/),
[notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution),
and [notary API keys](https://developer.apple.com/documentation/notaryapi/submitting-software-for-notarization-over-the-web).
Linux entry escaping follows the
[Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/).

## Acceptance check on clean machines

The coordinator must apply the release files, add the secrets, and build
a release after this branch and T-455 are merged. This Mac worktree cannot
publish to seer-releases or prove the clean Linux room flow.

On a clean Mac for each supported CPU, run the public install line. Find
Seer in Launchpad and Spotlight. Open it and check the window and icon.
Check codesign --verify --deep --strict and spctl --assess --type execute
on ~/Applications/Seer.app. Open seer://join/CAPSULE and check the name step
with T-455. No Gatekeeper bypass or removal of quarantine is permitted.

On a clean Linux desktop, run the same line. Open Seer from the app list.
Check the icon, window, and xdg-mime query default x-scheme-handler/seer.
Open an invite with xdg-open. Check X11 and Wayland with supported graphics
drivers. On Linux, host a room and join from the Mac bundle. Check that
local shells use each person's own files and credentials.

For update, keep a shell and window open, publish a second complete test
release, and run seer update. Check the CLI, bundle version, and reopened
window all use the new release. The old shell must continue. Run seer
restart and accept its prompt. Open Seer again and check the restored
layout with new shells. Check that another person's shells continue.
