import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import shlex
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tarfile
import tempfile
import termios
import threading
import time


def receive(connection):
    def exact(size):
        body = bytearray()
        while len(body) < size:
            chunk = connection.recv(size - len(body))
            if not chunk:
                raise EOFError
            body.extend(chunk)
        return body

    size = struct.unpack("!I", exact(4))[0]
    return json.loads(exact(size))


def send(connection, message):
    body = json.dumps(message).encode()
    connection.sendall(struct.pack("!I", len(body)) + body)


def room(listener, stopped, refused):
    connections = []

    def respond(connection):
        with connection:
            try:
                message = receive(connection)
                if "Join" in message:
                    reply = {"Refused": {"reason": "invalid seat"}} if refused else {
                        "Joined": {"user_id": "u-401", "credential": "secret", "name": "Alice"}
                    }
                elif "Hello" in message and message["Hello"]["credential"]:
                    reply = {"Welcome": {
                        "user_id": "u-401", "name": "Alice", "client_id": "c-401",
                        "tree": {"workspaces": [], "next_workspace_id": 1},
                    }}
                else:
                    reply = {"Refused": {"reason": "invalid credentials"}}
                send(connection, reply)
                while not stopped.is_set():
                    if select.select([connection], [], [], 0.1)[0]:
                        request = receive(connection)
                        if request == "ListPeople":
                            send(connection, {"People": {"people": []}})
            except (EOFError, OSError):
                pass

    listener.settimeout(0.1)
    while not stopped.is_set():
        try:
            connection, _ = listener.accept()
        except socket.timeout:
            continue
        worker = threading.Thread(target=respond, args=(connection,))
        worker.start()
        connections.append(worker)
    for worker in connections:
        worker.join(timeout=2)
        assert not worker.is_alive(), "room fixture must stop"


def check(cli, runtime, path, captured, refused):
    with tempfile.TemporaryDirectory(prefix="s401-", dir="/tmp") as directory:
        root = Path(directory)
        release = root / "release"
        release.mkdir()
        shutil.copy2(cli, release / "seer")
        shutil.copy2(runtime, release / "seer-runtime")
        shutil.copy2(cli.parent / "seer-broker", release / "seer-broker")
        os_name = "darwin" if sys.platform == "darwin" else "linux"
        arch = "arm64" if os.uname().machine in ("arm64", "aarch64") else "x86_64"
        archive = root / f"seer-{os_name}-{arch}.tar.gz"
        if path == "install":
            with tarfile.open(archive, "w:gz", compresslevel=0) as output:
                for binary in release.iterdir():
                    output.add(binary, arcname=binary.name)
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        invite = f"SEER1-127.0.0.1-{listener.getsockname()[1]}-seat401"
        installer = Path(__file__).resolve().parents[1] / "install.sh"
        command = f"{shlex.quote(str(release / 'seer'))} join {invite}"
        if path == "install":
            command = f"curl -fsSL {shlex.quote(installer.as_uri())} | sh -s -- {invite}"
        log = root / "stdout"
        if captured:
            command = f"({command}) > {shlex.quote(str(log))}"
        environment = dict(os.environ, HOME=str(root), XDG_CONFIG_HOME=str(root / "config"),
                           XDG_STATE_HOME=str(root / "state"), XDG_DATA_HOME=str(root / "data"),
                           SEER_BIN_DIR=str(root / "bin"), SEER_INSTALL_URL=root.as_uri(),
                           SEER_INSTALL_NO_PATH="1", TERM="xterm-256color", SHELL="/bin/sh")
        local = None
        if not refused:
            state = root / "state/seer/runtimes/u-401"
            state.mkdir(parents=True)
            local = subprocess.Popen(
                [runtime, state / "socket", "u-401", "/bin/sh", "qa401"],
                cwd=root,
                env=dict(environment, SEER_SNAPSHOT_DIR=str(state)),
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
        stopped = threading.Event()
        pid, master = pty.fork()
        if pid == 0:
            os.chdir(root)
            os.execve("/bin/sh", ["sh", "-c", command], environment)
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        broker = threading.Thread(target=room, args=(listener, stopped, refused))
        broker.start()
        terminal = bytearray()
        prompted = opened = finished = False
        status = None
        deadline = time.monotonic() + 12
        try:
            if local is not None:
                while not (state / "socket").exists() and time.monotonic() < deadline:
                    assert local.poll() is None, "QA runtime must start"
                    time.sleep(0.02)
                assert (state / "socket").exists(), "QA runtime must be ready"
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.05)[0]:
                    try:
                        terminal.extend(os.read(master, 65536))
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                output = bytes(terminal) + (log.read_bytes() if log.exists() else b"")
                if b"Name" in terminal and not prompted:
                    os.write(master, b"Alice\n")
                    prompted = True
                opened = b"\x1b[?1049h" in terminal
                finished = opened and b"\x1b[1;1H" in terminal and b" Seer " in terminal
                waited, status = os.waitpid(pid, os.WNOHANG)
                if waited:
                    pid = None
                    break
                if finished:
                    break
            if refused:
                assert pid is None and os.waitstatus_to_exitcode(status) != 0, output
                assert not opened and b"expired" in output, output
                assert not (root / "config/seer/servers.toml").exists()
                assert not (root / "state/seer/runtimes/u-401/runtime.pid").exists()
            else:
                assert opened and finished and pid is not None, (
                    f"{path}, captured={captured}: successful join must open the overlay "
                    f"in this terminal; status={status}, output={output!r}"
                )
                os.write(master, b"\x1b[<2;95;1M\x1b[<2;95;1mq")
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    if select.select([master], [], [], 0.05)[0]:
                        try:
                            os.read(master, 65536)
                        except OSError as error:
                            if error.errno != errno.EIO:
                                raise
                    waited, status = os.waitpid(pid, os.WNOHANG)
                    if waited:
                        pid = None
                        break
                assert pid is None, "the overlay must quit"
            print(f"PASS: {path}, captured={captured}, refused={refused}", flush=True)
        finally:
            stopped.set()
            os.close(master)
            if pid is not None:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                os.waitpid(pid, 0)
            if local is not None:
                local.terminate()
                local.wait(timeout=5)
            broker.join(timeout=3)
            listener.close()
            assert not broker.is_alive(), "room fixture must stop"


def check_noninteractive(cli, path):
    with tempfile.TemporaryDirectory(prefix="s401-", dir="/tmp") as directory:
        root = Path(directory)
        environment = dict(os.environ, HOME=str(root), XDG_CONFIG_HOME=str(root / "config"),
                           XDG_STATE_HOME=str(root / "state"), XDG_DATA_HOME=str(root / "data"))
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        invite = f"SEER1-127.0.0.1-{listener.getsockname()[1]}-seat401"
        stopped = threading.Event()
        broker = threading.Thread(target=room, args=(listener, stopped, False))
        broker.start()
        master = slave = process = None
        try:
            if path == "no-terminal":
                master, slave = pty.openpty()
                assert os.isatty(slave)
            process = subprocess.Popen(
                [cli, "join", invite], cwd=root, env=environment,
                stdin=slave if slave is not None else subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True,
            )
            if master is not None:
                os.write(master, b"Alice\n")
            output, _ = process.communicate(
                input=b"Alice\n" if slave is None else None, timeout=12,
            )
            assert process.returncode == 0, (path, process.returncode, output)
            assert b"\x1b[?1049h" not in output, output
            saved = (root / "config/seer/servers.toml").read_text()
            assert 'name = "Alice"' in saved and 'credential = "secret"' in saved, saved
            assert not (root / "state/seer/runtimes").exists()
            print(f"PASS: {path}", flush=True)
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.communicate(timeout=5)
            for descriptor in (master, slave):
                if descriptor is not None:
                    os.close(descriptor)
            stopped.set()
            broker.join(timeout=3)
            listener.close()
            assert not broker.is_alive(), "room fixture must stop"


if __name__ == "__main__":
    cli, runtime, path = sys.argv[1:]
    if path in ("no-terminal", "piped"):
        check_noninteractive(Path(cli).resolve(), path)
    else:
        for captured, refused in ((False, False), (True, False), (False, True), (True, True)):
            check(Path(cli).resolve(), Path(runtime).resolve(), path, captured, refused)
