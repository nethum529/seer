use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

const SEAT_LIFETIME_SECS: u64 = 3_600;
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
    ) -> io::Result<(Self, Option<String>)> {
        validate_name(owner_name)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.reason()))?;
        let state_dir = state_dir.as_ref().to_owned();
        fs::create_dir_all(&state_dir)?;
        let people_path = state_dir.join(PEOPLE_FILE);
        let seats_path = state_dir.join(SEATS_FILE);
        let mut people: Vec<PersonRecord> = load_json(&people_path)?;
        let seats: Vec<SeatRecord> = load_json(&seats_path)?;
        let owner_credential = if people.is_empty() {
            let credential = random_hex::<32>()?;
            people.push(new_person(owner_name, &credential, true, now_secs()?)?);
            write_json_atomically(&people_path, &people)?;
            Some(credential)
        } else {
            None
        };
        if !seats_path.exists() {
            write_json_atomically(&seats_path, &seats)?;
        }
        Ok((
            Self {
                state_dir,
                data: Mutex::new(RegistryData { people, seats }),
            },
            owner_credential,
        ))
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

    pub(crate) fn person_exists(&self, user_id: &str) -> io::Result<bool> {
        Ok(self
            .lock()?
            .people
            .iter()
            .any(|person| person.user_id == user_id))
    }

    pub(crate) fn create_seat(&self) -> io::Result<String> {
        self.create_seat_at(now_secs()?)
    }

    pub(crate) fn join(
        &self,
        seat_token: &str,
        name: &str,
    ) -> io::Result<Result<JoinResult, JoinError>> {
        self.join_at(seat_token, name, now_secs()?)
    }

    fn create_seat_at(&self, now: u64) -> io::Result<String> {
        let token = random_hex::<32>()?;
        let seat = SeatRecord {
            token_hash: credential_hash(&token),
            expires_at: now.saturating_add(SEAT_LIFETIME_SECS),
            used: false,
        };
        let mut data = self.lock()?;
        data.seats.push(seat);
        write_json_atomically(&self.state_dir.join(SEATS_FILE), &data.seats)?;
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
        data.people.push(person.clone());
        data.seats[seat_index].used = true;
        write_json_atomically(&self.state_dir.join(PEOPLE_FILE), &data.people)?;
        write_json_atomically(&self.state_dir.join(SEATS_FILE), &data.seats)?;
        Ok(Ok(JoinResult { person, credential }))
    }

    pub(crate) const fn seat_lifetime_secs() -> u64 {
        SEAT_LIFETIME_SECS
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let _data = self.data.lock().expect("registry lock must start valid");
        panic!("poison registry lock");
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, RegistryData>> {
        self.data
            .lock()
            .map_err(|_| io::Error::other("people registry lock is poisoned"))
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

fn random_hex<const N: usize>() -> io::Result<String> {
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

fn load_json<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    match File::open(path) {
        Ok(file) => serde_json::from_reader(BufReader::new(file)).map_err(invalid_json),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error),
    }
}

fn write_json_atomically<T: Serialize + ?Sized>(path: &Path, value: &T) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state file has no name"))?;
    let temporary = path.with_file_name(format!(".{}.tmp", file_name.to_string_lossy()));
    let file = File::create(&temporary)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, value).map_err(invalid_json)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    fs::rename(&temporary, path)
}

fn invalid_json(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{JoinError, PersonRecord, Registry, credential_hash, hashes_equal, load_json};
    use crate::test_support::{
        remove_directory, temporary_directory as create_temporary_directory,
    };

    #[test]
    fn registry_round_trip_and_owner_credential_is_returned_once() {
        let directory = temporary_directory("round");
        let (registry, credential) =
            Registry::open(&directory, "Owner").expect("registry must open");
        let credential = credential.expect("new owner credential must be returned");
        let owner = registry.people().expect("people must load").remove(0);

        assert_eq!(owner.user_id.len(), 32);
        assert_eq!(owner.name, "Owner");
        assert!(owner.is_owner);
        assert!(directory.join("seats.json").is_file());
        assert!(
            registry
                .person_exists(&owner.user_id)
                .expect("lookup must finish")
        );
        assert!(
            !registry
                .person_exists("missing")
                .expect("lookup must finish")
        );
        assert!(
            registry
                .authenticate(&owner.user_id, &credential)
                .expect("authentication must finish")
                .is_some()
        );
        drop(registry);

        let (reopened, second_credential) =
            Registry::open(&directory, "Ignored").expect("registry must reopen");
        assert!(second_credential.is_none());
        assert_eq!(reopened.people().expect("people must load"), vec![owner]);
        remove(directory);
    }

    #[test]
    fn atomic_write_replaces_the_file_and_removes_the_temporary_file() {
        let directory = temporary_directory("atomic");
        let path = directory.join("people.json");
        fs::write(&path, "[]\n").expect("old registry must write");

        super::write_json_atomically(&path, &["new"]).expect("registry must write");

        assert_eq!(
            fs::read_to_string(path).expect("registry must read"),
            "[\"new\"]\n"
        );
        assert!(!directory.join(".people.json.tmp").exists());
        remove(directory);
    }

    #[test]
    fn seat_expires_and_can_only_be_used_once() {
        let directory = temporary_directory("seat");
        let (registry, _) = Registry::open(&directory, "Owner").expect("registry must open");
        let token = registry.create_seat_at(10).expect("seat must be created");

        assert!(matches!(
            registry.join_at(&token, "Late", 3_610),
            Ok(Err(JoinError::InvalidSeat))
        ));
        let joined = registry
            .join_at(&token, "Guest", 3_609)
            .expect("join must finish");
        assert!(joined.is_ok());
        assert!(matches!(
            registry.join_at(&token, "Other", 3_609),
            Ok(Err(JoinError::InvalidSeat))
        ));
        remove(directory);
    }

    #[test]
    fn seat_file_survives_a_registry_reopen() {
        let directory = temporary_directory("reopen");
        let (registry, _) = Registry::open(&directory, "Owner").expect("registry must open");
        let token = registry.create_seat_at(10).expect("seat must be created");
        drop(registry);

        let (reopened, _) = Registry::open(&directory, "Owner").expect("registry must reopen");
        assert!(
            reopened
                .join_at(&token, "Guest", 11)
                .expect("join must finish")
                .is_ok()
        );
        remove(directory);
    }

    #[test]
    fn creates_a_current_one_hour_seat() {
        let directory = temporary_directory("current");
        let (registry, _) = Registry::open(&directory, "Owner").expect("registry must open");

        let token = registry.create_seat().expect("seat must be created");

        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(
            registry
                .join(&token, "Guest")
                .expect("join must finish")
                .is_ok()
        );
        remove(directory);
    }

    #[test]
    fn name_collision_and_invalid_name_do_not_consume_the_seat() {
        let directory = temporary_directory("name");
        let (registry, _) = Registry::open(&directory, "Owner").expect("registry must open");
        let token = registry.create_seat_at(10).expect("seat must be created");

        assert!(matches!(
            registry.join_at(&token, "owner", 11),
            Ok(Err(JoinError::NameInUse))
        ));
        assert!(matches!(
            registry.join_at(&token, "bad name", 11),
            Ok(Err(JoinError::InvalidName))
        ));
        assert!(
            registry
                .join_at(&token, "Guest_1", 11)
                .expect("join must finish")
                .is_ok()
        );
        remove(directory);
    }

    #[test]
    fn validates_name_boundaries() {
        assert_eq!(JoinError::InvalidName.reason(), "invalid name");
        for invalid in [
            "",
            "with space",
            "nonascii-\u{e9}",
            "123456789012345678901234567890123",
        ] {
            assert_eq!(super::validate_name(invalid), Err(JoinError::InvalidName));
        }
        for valid in ["a", "A-z_9", "12345678901234567890123456789012"] {
            assert_eq!(super::validate_name(valid), Ok(()));
        }
    }

    #[test]
    fn compares_sha256_hashes_in_constant_time() {
        let expected = credential_hash("secret");
        assert_eq!(expected.len(), 64);
        assert!(hashes_equal(&expected, &credential_hash("secret")));
        assert!(!hashes_equal(&expected, &credential_hash("wrong")));
        assert!(!hashes_equal(&expected, "short"));
    }

    #[test]
    fn rejects_invalid_registry_json() {
        let directory = temporary_directory("json");
        let path = directory.join("people.json");
        fs::write(&path, "not-json").expect("invalid registry must write");

        let error = load_json::<Vec<PersonRecord>>(&path).expect_err("invalid JSON must fail");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        remove(directory);
    }

    #[test]
    fn preserves_non_not_found_file_errors() {
        let directory = temporary_directory("file-error");
        let path = directory.join("x".repeat(300));

        load_json::<Vec<String>>(&path).expect_err("invalid file path must fail");

        remove(directory);
    }

    fn temporary_directory(name: &str) -> PathBuf {
        create_temporary_directory(&format!("sr-{name}"))
    }

    fn remove(path: impl AsRef<Path>) {
        remove_directory(path.as_ref(), "temporary directory must be removed");
    }
}
