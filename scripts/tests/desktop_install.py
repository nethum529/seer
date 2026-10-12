#!/usr/bin/env python3
import argparse
import functools
import http.server
import os
from pathlib import Path
import platform
import shutil
import shlex
import signal
import subprocess
import sys
import tarfile
import tempfile
import threading

REPO = Path(__file__).resolve().parents[2]
INSTALLER = REPO / "scripts/install.sh"
BINARIES = ("seer", "seer-broker", "seer-runtime")
SEER = None


def run(args, env, success=True):
    result = subprocess.run(args, env=env, capture_output=True, text=True)
    assert (result.returncode == 0) == success, result.stdout + result.stderr
    return result.stdout


def release(root, os_name, version, seer=None, complete=True, legacy=False):
    source = root / f"source-{os_name}-{version}"
    source.mkdir()
    for binary in (BINARIES if legacy else (*BINARIES, "seer-desktop")):
        file = source / binary
        file.write_text(f'#!/bin/sh\nprintf "%s\\n" "{version} {binary} $*"\n')
        file.chmod(0o755)
    if SEER and not seer:
        fixture = source / "seer"
        fixture.write_text(f'''#!/bin/sh
if [ "${{1:-}}" = --install-exchange ]; then
    {shlex.quote(str(SEER))} "$@" || exit 1
    {shlex.quote(sys.executable)} -c 'import os, signal, sys; target = os.environ.get("SEER_TEST_INTERRUPT_PATH"); target in sys.argv[1:3] and os.kill(int(sys.argv[3]), signal.SIGKILL)' "$2" "$3" "$PPID"
    exit 0
fi
printf '%s\\n' "{version} seer $*"
''')
    if seer:
        shutil.copy2(seer, source / "seer")
    if not legacy:
        shutil.copy2(INSTALLER, source / "install.sh")
        (source / "version").write_text(version + "\n")
    if os_name == "darwin" and not legacy:
        app = source / "Seer.app/Contents"
        (app / "MacOS").mkdir(parents=True)
        (app / "Resources").mkdir()
        for binary in (*BINARIES, "seer-desktop"):
            shutil.copy2(source / binary, app / "MacOS" / binary)
        (app / "Info.plist").write_text('''<?xml version="1.0"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.seer.Seer</string>
<key>CFBundleExecutable</key><string>seer-desktop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleName</key><string>Seer</string>
<key>CFBundleIconFile</key><string>Seer.icns</string>
<key>CFBundleURLTypes</key><array><dict>
<key>CFBundleURLSchemes</key><array><string>seer</string></array>
</dict></array></dict></plist>''')
        (app / "Resources/Seer.icns").write_bytes(b"fixture icon")
        (source / "seer-desktop").unlink()
    elif not legacy:
        icon = source / "share/icons/hicolor/scalable/apps/seer.svg"
        icon.parent.mkdir(parents=True)
        icon.write_text('<svg xmlns="http://www.w3.org/2000/svg"/>')
    if not complete:
        (source / "seer-runtime").unlink()
    target = root / "releases" / version
    target.mkdir(parents=True, exist_ok=True)
    arch = "arm64" if os_name == "darwin" and platform.machine() == "arm64" else "x86_64"
    with tarfile.open(target / f"seer-{os_name}-{arch}.tar.gz", "w:gz", compresslevel=1) as archive:
        for file in source.iterdir():
            archive.add(file, arcname=file.name)
    return target


def environment(root, os_name):
    home = root / f"Home {os_name} % dollar$ quote\" back` slash\\"
    home.mkdir()
    tools = root / f"tools-{os_name}"
    tools.mkdir()
    uname = tools / "uname"
    uname.write_text(f'#!/bin/sh\ncase "$1" in -s) echo {"Darwin" if os_name == "darwin" else "Linux"};; -m) echo {"arm64" if os_name == "darwin" and platform.machine() == "arm64" else "x86_64"};; esac\n')
    uname.chmod(0o755)
    mime = tools / "xdg-mime"
    mime.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$HOME/mime-calls"\n')
    mime.chmod(0o755)
    return dict(os.environ, HOME=str(home), SHELL="/bin/sh", TMPDIR=str(root),
                XDG_DATA_HOME=str(home / "data"), XDG_CONFIG_HOME=str(home / "config"),
                PATH=f'{tools}:/usr/bin:/bin:/usr/sbin:/sbin', SEER_INSTALL_NO_PATH="1")


def desktop(home, os_name):
    if os_name == "darwin":
        return home / "Applications/Seer.app/Contents/MacOS/seer-desktop"
    return home / ".local/bin/seer-desktop"


def install_contract(root, os_name):
    env = environment(root, os_name)
    home = Path(env["HOME"])
    for version in ("first", "second"):
        asset_dir = release(root, os_name, version)
        env["SEER_INSTALL_URL"] = asset_dir.as_uri()
        run(["sh", str(INSTALLER)], env)
        launcher = desktop(home, os_name)
        assert launcher.is_file(), f"The install line did not install {launcher}"
        assert run([str(launcher), "seer://join/SEER2-fixture"], env).strip() == f"{version} seer-desktop seer://join/SEER2-fixture"
        for binary in BINARIES:
            assert run([str(home / ".local/bin" / binary)], env).strip() == f"{version} {binary}"
        if os_name == "linux":
            entry = home / "data/applications/seer.desktop"
            assert "MimeType=x-scheme-handler/seer;" in entry.read_text()
            assert (home / "data/icons/hicolor/scalable/apps/seer.svg").is_file()
            assert "default seer.desktop x-scheme-handler/seer" in (home / "mime-calls").read_text()
        else:
            assert (home / "Applications/Seer.app/Contents/Resources/Seer.icns").is_file()
    env["SEER_INSTALL_URL"] = release(root, os_name, "broken", complete=False).as_uri()
    run(["sh", str(INSTALLER)], env, success=False)
    assert run([str(desktop(home, os_name))], env).startswith("second ")
    assert run([str(home / ".local/bin/seer")], env).startswith("second ")
    print(f"PASS: {os_name} install, reinstall, launcher, scheme, and incomplete release")


def invalid_payload_contract(root, os_name):
    root.mkdir()
    env = environment(root, os_name)
    home = Path(env["HOME"])
    release(root, os_name, "working")
    env["SEER_INSTALL_SOURCE"] = str(root / f"source-{os_name}-working")
    run(["sh", str(INSTALLER)], env)
    payloads = [Path(binary) for binary in BINARIES]
    if os_name == "darwin":
        payloads += [Path("Seer.app/Contents/MacOS") / binary for binary in (*BINARIES, "seer-desktop")]
    else:
        payloads.insert(0, Path("seer-desktop"))
    failures = []
    for payload in payloads:
        for invalid in ("empty", "directory", "not-executable"):
            baseline = {file.relative_to(home): file.read_bytes() for file in home.rglob("*") if file.is_file()}
            source = root / f"source-{os_name}-working"
            broken = root / "broken"
            shutil.copytree(source, broken)
            file = broken / payload
            file.unlink()
            if invalid == "directory":
                file.mkdir()
            else:
                file.write_bytes(b"" if invalid == "empty" else b"payload")
                file.chmod(0o755 if invalid == "empty" else 0o644)
            env["SEER_INSTALL_SOURCE"] = str(broken)
            result = subprocess.run(["sh", str(INSTALLER)], env=env, capture_output=True, text=True)
            if result.returncode == 0:
                failures.append(f"{os_name} accepted {invalid} payload {payload}")
            if {file.relative_to(home): file.read_bytes() for file in home.rglob("*") if file.is_file()} != baseline:
                failures.append(f"{os_name} changed the install for {invalid} payload {payload}")
            shutil.rmtree(broken)
            env["SEER_INSTALL_SOURCE"] = str(source)
            run(["sh", str(INSTALLER)], env)
    assert not failures, "\n".join(failures)
    print(f"PASS: {os_name} rejects empty, directory, and non-executable payloads without changing the install")


def interrupted_install_contract(root, os_name, binary):
    root.mkdir()
    env = environment(root, os_name)
    home = Path(env["HOME"])
    for version in ("working", "replacement"):
        release(root, os_name, version)
    env["SEER_INSTALL_SOURCE"] = str(root / f"source-{os_name}-working")
    run(["sh", str(INSTALLER)], env)
    target = home / ("Applications/Seer.app" if binary == "app" else f".local/bin/{binary}")
    executable = target / "Contents/MacOS/seer-desktop" if binary == "app" else target
    old_inode = executable.stat().st_ino
    env["SEER_TEST_INTERRUPT_PATH"] = str(target)
    move = Path(env["PATH"].split(":")[0]) / "mv"
    move.write_text(f'''#!{sys.executable}
import os, signal, subprocess, sys
result = subprocess.run(["/bin/mv", *sys.argv[1:]])
if result.returncode == 0 and os.environ["SEER_TEST_INTERRUPT_PATH"] in sys.argv[1:]:
    os.kill(os.getppid(), signal.SIGKILL)
sys.exit(result.returncode)
''')
    move.chmod(0o755)
    env["SEER_INSTALL_SOURCE"] = str(root / f"source-{os_name}-replacement")
    result = subprocess.run(["sh", str(INSTALLER)], env=env, capture_output=True, text=True)
    assert result.returncode == -signal.SIGKILL, result.stdout + result.stderr
    assert executable.is_file(), f"{os_name} interrupted update removed {target}"
    assert run([str(executable)], env).startswith("replacement ")
    assert executable.stat().st_ino != old_inode
    assert any(home.rglob("old")), "The interrupted update lost its rollback copy"
    if binary == "seer":
        baseline = {file.relative_to(home): file.read_bytes() for file in home.rglob("*") if file.is_file()}
        move.write_text(move.read_text().replace("os.kill(os.getppid(), signal.SIGKILL)", "sys.exit(1)"))
        env["SEER_INSTALL_SOURCE"] = str(root / f"source-{os_name}-working")
        run(["sh", str(INSTALLER)], env, success=False)
        assert {file.relative_to(home): file.read_bytes() for file in home.rglob("*") if file.is_file()} == baseline
    print(f"PASS: {os_name} interrupted {binary} replacement keeps the new executable and rollback copy")


def legacy_install_contract(root, os_name):
    root.mkdir()
    env = environment(root, os_name)
    home = Path(env["HOME"])
    env["SEER_INSTALL_URL"] = release(root, os_name, "legacy", legacy=True).as_uri()
    run(["sh", str(INSTALLER)], env)
    for binary in BINARIES:
        assert run([str(home / ".local/bin" / binary)], env).startswith("legacy ")
    assert not desktop(home, os_name).exists()
    print(f"PASS: {os_name} keeps the public install line working before the first desktop release")


class Releases(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/latest/"):
            self.send_response(302)
            self.send_header("Location", f"http://127.0.0.1:{self.server.server_port}" + self.path.replace("/latest/", "/download/v99.0.0/"))
            self.end_headers()
        else:
            super().do_GET()

    def log_message(self, *_args):
        pass


def update_contract(root, seer):
    os_name = "darwin" if platform.system() == "Darwin" else "linux"
    env = environment(root, "update")
    env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    home = Path(env["HOME"])
    initial = release(root, os_name, "old", seer)
    env["SEER_INSTALL_URL"] = initial.as_uri()
    run(["sh", str(INSTALLER)], env)
    newest = release(root, os_name, "99.0.0")
    served = root / "http/download/v99.0.0"
    shutil.copytree(newest, served)
    handler = functools.partial(Releases, directory=str(root / "http"))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        env["SEER_UPDATE_URL"] = f"http://127.0.0.1:{server.server_port}/latest"
        text = run([str(home / ".local/bin/seer"), "update"], env)
        assert "Updated to 99.0.0" in text, text
        assert run([str(desktop(home, os_name))], env).startswith("99.0.0 ")
        for binary in BINARIES:
            assert run([str(home / ".local/bin" / binary)], env).startswith("99.0.0 ")
        print("PASS: real seer update downloads one release and replaces the app and CLI")
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--seer", type=Path)
    args = parser.parse_args()
    if platform.system() == "Darwin" and not args.seer:
        parser.error("macOS bundle replacement needs --seer with the current CLI")
    SEER = args.seer.resolve() if args.seer else None
    with tempfile.TemporaryDirectory(prefix="seer457-") as directory:
        root = Path(directory)
        for os_name in (("darwin", "linux") if platform.system() == "Darwin" else ("linux",)):
            legacy_install_contract(root / f"legacy-{os_name}", os_name)
            install_contract(root, os_name)
            invalid_payload_contract(root / f"invalid-{os_name}", os_name)
            for binary in (("app", "seer") if os_name == "darwin" else ("seer-desktop", "seer")):
                interrupted_install_contract(root / f"interrupt-{os_name}-{binary}", os_name, binary)
        if args.seer:
            update_contract(root, args.seer.resolve())
