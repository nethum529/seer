use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use seer_core::proto::ServerMsg;

use crate::registry::{PersonRecord, load_json, write_json_atomically};

pub(crate) struct Grants {
    directory: PathBuf,
    owners: Mutex<BTreeMap<String, BTreeSet<String>>>,
}

impl Grants {
    pub(crate) fn open(directory: &Path, people: &[PersonRecord]) -> io::Result<Self> {
        let directory = directory.join("grants");
        std::fs::create_dir_all(&directory)?;
        let mut owners = BTreeMap::new();
        for person in people {
            owners.insert(
                person.user_id.clone(),
                load_json(&directory.join(format!("{}.json", person.user_id)))?,
            );
        }
        Ok(Self {
            directory,
            owners: Mutex::new(owners),
        })
    }

    pub(crate) fn set(&self, owner: &str, user: &str, can_type: bool) -> io::Result<()> {
        let mut owners = self.lock()?;
        let mut users = owners.get(owner).cloned().unwrap_or_default();
        if can_type {
            users.insert(user.to_owned());
        } else {
            users.remove(user);
        }
        write_json_atomically(&self.directory.join(format!("{owner}.json")), &users)?;
        owners.insert(owner.to_owned(), users);
        Ok(())
    }

    pub(crate) fn permits(&self, owner: &str, user: &str) -> io::Result<bool> {
        Ok(self
            .lock()?
            .get(owner)
            .is_some_and(|users| users.contains(user)))
    }

    pub(crate) fn message(&self, user: &str) -> io::Result<ServerMsg> {
        let owners = self.lock()?;
        Ok(ServerMsg::Grants {
            can_type_here: owners.get(user).into_iter().flatten().cloned().collect(),
            you_may_type_into: owners
                .iter()
                .filter(|(_, users)| users.contains(user))
                .map(|(owner, _)| owner.clone())
                .collect(),
        })
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, BTreeMap<String, BTreeSet<String>>>> {
        self.owners
            .lock()
            .map_err(|_| io::Error::other("grant lock is poisoned"))
    }
}

#[derive(Default)]
pub(super) struct LineMarker {
    started: bool,
    after_cr: bool,
}

impl LineMarker {
    pub(super) fn prefix(&mut self, name: &str, bytes: Vec<u8>) -> io::Result<String> {
        let input = String::from_utf8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "input must be UTF-8"))?;
        let mut output = String::new();
        for character in input.chars() {
            if self.after_cr && character == '\n' {
                output.push(character);
                self.after_cr = false;
                continue;
            }
            if !self.started {
                output.push_str(&format!("[seer: {name}] "));
                self.started = true;
            }
            output.push(character);
            self.after_cr = character == '\r';
            if matches!(character, '\r' | '\n') {
                self.started = false;
            }
        }
        Ok(output)
    }
}
