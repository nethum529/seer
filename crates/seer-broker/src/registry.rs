use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, BufReader, BufWriter, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

const SEAT_LIFETIME_SECS: u64 = 3_600;
pub(crate) const MAX_SEAT_LIFETIME_SECS: u64 = 604_800;
const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
const REGISTRY_FILE: &str = "registry.json";
const PEOPLE_FILE: &str = "people.json";
const SEATS_FILE: &str = "seats.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PersonRecord {
    pub(crate) user_id: String,
    pub(crate) name: String,
    credential_hash: String,
    created_at: u64,
    pub(crate) is_owner: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SeatRecord {
    token_hash: String,
    expires_at: u64,
    used: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct RegistryData {
    people: Vec<PersonRecord>,
    seats: Vec<SeatRecord>,
}

pub(crate) struct Registry {
    state_dir: PathBuf,
    data: Mutex<RegistryData>,
}

pub(crate) struct JoinResult {
    pub(crate) person: PersonRecord,
    pub(crate) credential: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JoinError {
    InvalidSeat,
    InvalidName,
    NameInUse,
}

impl JoinError {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::InvalidSeat => "invalid seat",
            Self::InvalidName => "invalid name",
            Self::NameInUse => "name is in use",
        }
    }
}

impl Registry {
    pub(crate) fn open(
        state_dir: impl AsRef<Path>,
        owner_name: &str,
    ) -> io::Result<(Self, Option<(String, String)>)> {
        validate_name(owner_name)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.reason()))?;
        let state_dir = state_dir.as_ref().to_owned();
        fs::create_dir_all(&state_dir)?;
        fs::set_permissions(&state_dir, Permissions::from_mode(DIRECTORY_MODE))?;
        let people_path = state_dir.join(PEOPLE_FILE);
        let seats_path = state_dir.join(SEATS_FILE);
        let registry_path = state_dir.join(REGISTRY_FILE);
        let registry_exists = registry_path.exists();
        let mut data = if registry_exists {
            set_private_file(&registry_path)?;
            load_json(&registry_path)?
        } else {
            RegistryData {
                people: load_json(&people_path)?,
                seats: load_json(&seats_path)?,
            }
        };
        let owner_identity = if data.people.is_empty() {
            let credential = random_hex::<32>()?;
            let person = new_person(owner_name, &credential, true, now_secs()?)?;
            let user_id = person.user_id.clone();
            data.people.push(person);
            Some((user_id, credential))
        } else {
            None
        };
        if !registry_exists || owner_identity.is_some() {
            write_json_atomically(&registry_path, &data)?;
        }
        Ok((
            Self {
                state_dir,
                data: Mutex::new(data),
            },
            owner_identity,
        ))
    }

    pub(crate) fn remint_owner(&self) -> io::Result<(String, String)> {
        let mut data = self.lock()?;
        let mut next = data.clone();
        let owner = next
            .people
            .iter_mut()
            .find(|person| person.is_owner)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "owner is unavailable"))?;
        let credential = random_hex::<32>()?;
        owner.credential_hash = credential_hash(&credential);
        let user_id = owner.user_id.clone();
        write_json_atomically(&self.state_dir.join(REGISTRY_FILE), &next)?;
        *data = next;
        Ok((user_id, credential))
    }

    pub(crate) fn authenticate(
        &self,
        user_id: &str,
        credential: &str,
    ) -> io::Result<Option<PersonRecord>> {
        let supplied_hash = credential_hash(credential);
        let data = self.lock()?;
        Ok(data
            .people
            .iter()
            .find(|person| person.user_id == user_id)
            .filter(|person| hashes_equal(&person.credential_hash, &supplied_hash))
            .cloned())
    }

    pub(crate) fn people(&self) -> io::Result<Vec<PersonRecord>> {
        Ok(self.lock()?.people.clone())
    }

    pub(crate) fn person(&self, user_id: &str) -> io::Result<Option<PersonRecord>> {
        let data = self.lock()?;
        Ok(data
            .people
            .iter()
            .find(|person| person.user_id == user_id)
            .cloned())
    }

    pub(crate) fn remove_person(&self, user_id: &str) -> io::Result<()> {
        let mut data = self.lock()?;
        let mut next = data.clone();
        next.people.retain(|person| person.user_id != user_id);
        write_json_atomically(&self.state_dir.join(REGISTRY_FILE), &next)?;
        *data = next;
        Ok(())
    }

    pub(crate) fn create_seat(&self, lifetime_secs: u64) -> io::Result<String> {
        self.create_seat_at(now_secs()?, lifetime_secs)
    }

    pub(crate) fn join(
        &self,
        seat_token: &str,
        name: &str,
    ) -> io::Result<Result<JoinResult, JoinError>> {
        self.join_at(seat_token, name, now_secs()?)
    }

    fn create_seat_at(&self, now: u64, lifetime_secs: u64) -> io::Result<String> {
        let token = random_hex::<32>()?;
        let seat = SeatRecord {
            token_hash: credential_hash(&token),
            expires_at: now.saturating_add(lifetime_secs.min(MAX_SEAT_LIFETIME_SECS)),
            used: false,
        };
        let mut data = self.lock()?;
        let mut next = data.clone();
        next.seats.push(seat);
        write_json_atomically(&self.state_dir.join(REGISTRY_FILE), &next)?;
        *data = next;
        Ok(token)
    }

    fn join_at(
        &self,
        seat_token: &str,
        name: &str,
        now: u64,
    ) -> io::Result<Result<JoinResult, JoinError>> {
        let token_hash = credential_hash(seat_token);
        let mut data = self.lock()?;
        let Some(seat_index) = data.seats.iter().position(|seat| {
            hashes_equal(&seat.token_hash, &token_hash) && !seat.used && seat.expires_at > now
        }) else {
            return Ok(Err(JoinError::InvalidSeat));
        };
        if let Err(error) = validate_name(name) {
            return Ok(Err(error));
        }
        if data
            .people
            .iter()
            .any(|person| person.name.eq_ignore_ascii_case(name))
        {
            return Ok(Err(JoinError::NameInUse));
        }

        let credential = random_hex::<32>()?;
        let person = new_person(name, &credential, false, now)?;
        let mut next = data.clone();
        next.people.push(person.clone());
        next.seats[seat_index].used = true;
        write_json_atomically(&self.state_dir.join(REGISTRY_FILE), &next)?;
        *data = next;
        Ok(Ok(JoinResult { person, credential }))
    }

    pub(crate) const fn seat_lifetime_secs() -> u64 {
        SEAT_LIFETIME_SECS
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, RegistryData>> {
        self.data
            .lock()
            .map_err(|_| io::Error::other("people registry lock is poisoned"))
    }
}

// The seats stay, so an invitation made before a stop still works after the
// next start.
pub fn clear_people(state_dir: &Path) -> io::Result<()> {
    let registry_path = state_dir.join(REGISTRY_FILE);
    match load_json_required::<RegistryData>(&registry_path) {
        Ok(mut data) => {
            data.people.clear();
            write_json_atomically(&registry_path, &data)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match fs::remove_file(state_dir.join(PEOPLE_FILE)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn new_person(
    name: &str,
    credential: &str,
    is_owner: bool,
    created_at: u64,
) -> io::Result<PersonRecord> {
    Ok(PersonRecord {
        user_id: random_hex::<16>()?,
        name: name.to_owned(),
        credential_hash: credential_hash(credential),
        created_at,
        is_owner,
    })
}

fn validate_name(name: &str) -> Result<(), JoinError> {
    if (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(JoinError::InvalidName)
    }
}

fn credential_hash(value: &str) -> String {
    encode_hex(&Sha256::digest(value.as_bytes()))
}

fn hashes_equal(expected: &str, supplied: &str) -> bool {
    constant_time_eq::constant_time_eq(expected.as_bytes(), supplied.as_bytes())
}

pub(crate) fn random_hex<const N: usize>() -> io::Result<String> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(encode_hex(&bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn now_secs() -> io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(io::Error::other)
}

pub(crate) fn load_json<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    match load_json_required(path) {
        Ok(value) => Ok(value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error),
    }
}

pub(crate) fn load_json_required<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let file = File::open(path)?;
    serde_json::from_reader(BufReader::new(file)).map_err(invalid_json)
}

pub(crate) fn set_private_file(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, Permissions::from_mode(FILE_MODE))
}

pub(crate) fn write_json_atomically<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state file has no name"))?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "state file has no directory")
    })?;
    let temporary = path.with_file_name(format!(".{}.tmp", file_name.to_string_lossy()));
    let result = (|| {
        {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(FILE_MODE)
                .open(&temporary)?;
            file.set_permissions(Permissions::from_mode(FILE_MODE))?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, value).map_err(invalid_json)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
        }
        fs::rename(&temporary, path)?;
        let _ = File::open(parent).and_then(|directory| directory.sync_all());
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn invalid_json(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{JoinError, Registry};
    use crate::test_support::{
        remove_directory, temporary_directory as create_temporary_directory,
    };

    #[test]
    fn seat_uses_the_requested_lifetime() {
        let directory = temporary_directory("seat-lifetime");
        let (registry, _) = Registry::open(&directory, "Owner").expect("registry must open");
        let lifetime = 24 * 60 * 60;
        let token = registry
            .create_seat_at(10, lifetime)
            .expect("seat must be created");
        let default_token = registry
            .create_seat_at(10, Registry::seat_lifetime_secs())
            .expect("seat must be created");
        let late = registry.join_at(&token, "Late", 10 + 25 * 60 * 60);
        let early = registry
            .join_at(&token, "Early", 10 + 23 * 60 * 60)
            .expect("join must finish");
        assert!(early.is_ok());
        assert!(matches!(late, Ok(Err(JoinError::InvalidSeat))));
        assert!(matches!(
            registry.join_at(&default_token, "LateDefault", 3_610),
            Ok(Err(JoinError::InvalidSeat))
        ));
        assert!(
            registry
                .join_at(&default_token, "EarlyDefault", 3_609)
                .expect("join must finish")
                .is_ok()
        );
        remove(directory);
    }

    fn temporary_directory(name: &str) -> PathBuf {
        create_temporary_directory(&format!("sr-{name}"))
    }

    fn remove(path: impl AsRef<Path>) {
        remove_directory(path.as_ref(), "temporary directory must be removed");
    }
}
