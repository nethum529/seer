#[cfg(target_os = "linux")]
use serde::{Deserialize, Serialize};
use std::process::ExitCode;
#[cfg(target_os = "linux")]
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
const PORT: u16 = 7321;
#[cfg(target_os = "linux")]
const START_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(target_os = "linux")]
const POLL_INTERVAL: Duration = Duration::from_millis(25);
#[cfg(target_os = "linux")]
#[derive(Deserialize, Serialize)]
struct BrokerConfig {
    listen: SocketAddr,
    published_addr: String,
    #[serde(default = "remote_enabled")]
    remote: bool,
    owner_name: String,
    state_dir: PathBuf,
}
pub fn run() -> ExitCode {
    #[cfg(target_os = "macos")]
    {
        eprintln!("the server runs on Linux only");
        ExitCode::FAILURE
    }

    #[cfg(target_os = "linux")]
    match run_linux() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
#[cfg(target_os = "linux")]
fn run_linux() -> io::Result<()> {
    let config_dir = config_dir()?;
    secure_directory(&config_dir)?;
    let config_path = config_dir.join("broker.toml");
    let (config, first_start) = load_or_create_config(&config_path)?;
    secure_directory(&config.state_dir)?;

    if running_broker(&config) {
        println!("Server already running at {}.", config.published_addr);
        return Ok(());
    }

    start_broker(&config_path, &config, &config_dir, first_start)
}
#[cfg(target_os = "linux")]
fn config_dir() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("seer"));
    }
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config/seer"))
}
#[cfg(target_os = "linux")]
fn state_dir() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("seer"));
    }
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".local/state/seer"))
}
#[cfg(target_os = "linux")]
fn secure_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
#[cfg(target_os = "linux")]
fn load_or_create_config(path: &Path) -> io::Result<(BrokerConfig, bool)> {
    match fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents)
            .map(|config| (config, false))
            .map_err(invalid_data),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let config = prompt_config()?;
            write_private(path, toml_text(&config)?.as_bytes())?;
            Ok((config, true))
        }
        Err(error) => Err(error),
    }
}
#[cfg(target_os = "linux")]
fn prompt_config() -> io::Result<BrokerConfig> {
    let login = env::var("USER")
        .or_else(|_| env::var("LOGNAME"))
        .unwrap_or_else(|_| "owner".to_owned());
    let owner_name = prompt("Your name", &login)?;
    let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT);
    Ok(BrokerConfig {
        listen,
        published_addr: listen.to_string(),
        remote: true,
        owner_name,
        state_dir: state_dir()?,
    })
}
#[cfg(target_os = "linux")]
fn remote_enabled() -> bool {
    true
}
#[cfg(target_os = "linux")]
fn host_name() -> String {
    env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| fs::read_to_string("/proc/sys/kernel/hostname").ok())
        .or_else(|| fs::read_to_string("/etc/hostname").ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "localhost".to_owned())
}
#[cfg(target_os = "linux")]
fn prompt(label: &str, default: &str) -> io::Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim();
    Ok(if value.is_empty() {
        default.to_owned()
    } else {
        value.to_owned()
    })
}
#[cfg(target_os = "linux")]
fn toml_text(value: &impl Serialize) -> io::Result<String> {
    toml::to_string_pretty(value).map_err(invalid_data)
}
#[cfg(target_os = "linux")]
fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
#[cfg(target_os = "linux")]
fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(contents)
}
#[cfg(target_os = "linux")]
fn running_broker(config: &BrokerConfig) -> bool {
    let pid_path = config.state_dir.join("broker.pid");
    let Ok(contents) = fs::read_to_string(pid_path) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<i32>() else {
        return false;
    };
    owns_listen_socket(pid, config.listen) && port_accepts(config.listen)
}
#[cfg(target_os = "linux")]
fn owns_listen_socket(pid: i32, listen: SocketAddr) -> bool {
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    let listening = listening_inodes(listen.port());
    entries.filter_map(Result::ok).any(|entry| {
        fs::read_link(entry.path())
            .ok()
            .and_then(|target| socket_inode(&target))
            .is_some_and(|inode| listening.contains(&inode))
    })
}
#[cfg(target_os = "linux")]
fn listening_inodes(port: u16) -> Vec<String> {
    ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .flat_map(|table| {
            table
                .lines()
                .skip(1)
                .filter_map(move |line| listening_inode(line, port))
                .collect::<Vec<_>>()
        })
        .collect()
}
#[cfg(target_os = "linux")]
fn listening_inode(line: &str, port: u16) -> Option<String> {
    let fields: Vec<_> = line.split_whitespace().collect();
    let local_port = fields.get(1)?.rsplit_once(':')?.1;
    let parsed_port = u16::from_str_radix(local_port, 16).ok()?;
    if parsed_port == port && fields.get(3) == Some(&"0A") {
        fields.get(9).map(|inode| (*inode).to_owned())
    } else {
        None
    }
}
#[cfg(target_os = "linux")]
fn socket_inode(path: &Path) -> Option<String> {
    path.to_str()?
        .strip_prefix("socket:[")?
        .strip_suffix(']')
        .map(str::to_owned)
}
#[cfg(target_os = "linux")]
fn port_accepts(listen: SocketAddr) -> bool {
    let address = if listen.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), listen.port())
    } else {
        listen
    };
    TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok()
}
#[cfg(target_os = "linux")]
fn start_broker(
    config_path: &Path,
    config: &BrokerConfig,
    config_dir: &Path,
    first_start: bool,
) -> io::Result<()> {
    let broker = find_broker()?;
    let log_path = config.state_dir.join("broker.log");
    let (log, log_start) = open_log(&log_path)?;
    let mut child = spawn_detached(&broker, config_path, log)?;
    let deadline = Instant::now() + START_TIMEOUT;

    if let Err(error) = complete_start(
        &mut child,
        config,
        config_dir,
        first_start,
        &log_path,
        log_start,
        deadline,
    ) {
        stop_child(&mut child);
        print_log_tail(&log_path);
        return Err(error);
    }
    println!("Server started at {}.", config.published_addr);
    println!("You are {}.", config.owner_name);
    crate::commands::first_invite(first_start).map_err(|error| io::Error::other(error.message))
}

#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)] // Startup needs these values to clean up one child on each error.
fn complete_start(
    child: &mut Child,
    config: &BrokerConfig,
    config_dir: &Path,
    first_start: bool,
    log_path: &Path,
    log_start: u64,
    deadline: Instant,
) -> io::Result<()> {
    wait_for_port(child, config.listen, deadline)?;
    if first_start {
        let credential = wait_for_credential(log_path, log_start, deadline)?;
        let save_result = save_owner(config_dir, config, credential);
        strip_owner_credential(log_path)?;
        save_result?;
    }
    write_private(
        &config.state_dir.join("broker.pid"),
        format!("{}\n", child.id()).as_bytes(),
    )
}

#[cfg(target_os = "linux")]
fn find_broker() -> io::Result<PathBuf> {
    let executable = env::current_exe()?;
    if let Some(candidate) = executable.parent().map(|parent| parent.join("seer-broker"))
        && candidate.is_file()
    {
        return Ok(candidate);
    }
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join("seer-broker");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "seer-broker was not found",
    ))
}

#[cfg(target_os = "linux")]
fn open_log(path: &Path) -> io::Result<(File, u64)> {
    let log = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)?;
    log.set_permissions(fs::Permissions::from_mode(0o600))?;
    let start = log.metadata()?.len();
    Ok((log, start))
}

#[cfg(target_os = "linux")]
fn spawn_detached(broker: &Path, config: &Path, log: File) -> io::Result<Child> {
    let stderr = log.try_clone()?;
    let mut command = Command::new(broker);
    command
        .arg(config)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    // Safety: setsid has no memory safety requirements and runs before exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() >= 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    command.spawn()
}

#[cfg(target_os = "linux")]
fn wait_for_port(child: &mut Child, listen: SocketAddr, deadline: Instant) -> io::Result<()> {
    loop {
        if port_accepts(listen) {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "seer-broker exited with {status}"
            )));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "seer-broker did not open the listen port within 5 seconds",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(target_os = "linux")]
fn stop_child(child: &mut Child) {
    let pid = child.id().cast_signed();
    // Safety: kill receives the process group ID created by setsid.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "linux")]
fn wait_for_credential(path: &Path, start: u64, deadline: Instant) -> io::Result<String> {
    loop {
        if let Some(credential) = read_credential(path, start)? {
            return Ok(credential);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "seer-broker did not write the owner credential within 5 seconds",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(target_os = "linux")]
fn read_credential(path: &Path, start: u64) -> io::Result<Option<String>> {
    let mut log = File::open(path)?;
    log.seek(SeekFrom::Start(start))?;
    let mut contents = String::new();
    log.read_to_string(&mut contents)?;
    Ok(contents.lines().find_map(|line| {
        line.strip_prefix("owner-credential: ")
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }))
}

#[cfg(target_os = "linux")]
fn strip_owner_credential(path: &Path) -> io::Result<()> {
    let contents = fs::read_to_string(path)?;
    let filtered = contents
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("owner-credential: "))
        .collect::<String>();
    write_private(path, filtered.as_bytes())
}

#[cfg(target_os = "linux")]
fn save_owner(config_dir: &Path, config: &BrokerConfig, credential: String) -> io::Result<()> {
    let path = config_dir.join("servers.toml");
    let mut store = crate::store::ServerStore::load_from(&path)?;
    store
        .servers
        .retain(|server| server.endpoint != config.published_addr);
    let user_id = owner_user_id(&config.state_dir).unwrap_or_else(|| config.owner_name.clone());
    store.make_current(crate::store::ServerEntry {
        endpoint: config.published_addr.clone(),
        alias: host_name(),
        user_id,
        name: config.owner_name.clone(),
        credential,
        current: true,
    });
    store.save_to(&path)
}

#[cfg(target_os = "linux")]
fn owner_user_id(state_dir: &Path) -> Option<String> {
    let contents = fs::read_to_string(state_dir.join("people.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    find_owner_id(&value)
}

#[cfg(target_os = "linux")]
fn find_owner_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("is_owner").and_then(serde_json::Value::as_bool) == Some(true) {
                return object
                    .get("user_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
            }
            object.values().find_map(find_owner_id)
        }
        serde_json::Value::Array(values) => values.iter().find_map(find_owner_id),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn print_log_tail(path: &Path) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<_> = contents.lines().collect();
    for line in lines.iter().skip(lines.len().saturating_sub(20)) {
        eprintln!("{line}");
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "start_tests.rs"]
mod tests;
