use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use seer_core::proto::ServerMsg;

use crate::registry::{PersonRecord, load_json, write_json_atomically};

mod permit;
pub(crate) use permit::Permit;

type Owners = BTreeMap<String, BTreeMap<String, Arc<Permit>>>;

pub(crate) struct Grants {
    directory: PathBuf,
    owners: Mutex<Owners>,
}

impl Grants {
    pub(crate) fn open(directory: &Path, people: &[PersonRecord]) -> io::Result<Self> {
        let directory = directory.join("grants");
        std::fs::create_dir_all(&directory)?;
        let mut owners = BTreeMap::new();
        let members: BTreeSet<_> = people.iter().map(|person| &person.user_id).collect();
        for person in people {
            let path = directory.join(format!("{}.json", person.user_id));
            let mut users: BTreeSet<String> = load_json(&path)?;
            let saved_count = users.len();
            users.retain(|user| members.contains(user));
            if users.len() != saved_count {
                write_json_atomically(&path, &users)?;
            }
            owners.insert(
                person.user_id.clone(),
                users
                    .into_iter()
                    .map(|user| (user, Arc::new(Permit::default())))
                    .collect(),
            );
        }
        Ok(Self {
            directory,
            owners: Mutex::new(owners),
        })
    }

    pub(crate) fn set(&self, owner: &str, user: &str, can_type: bool) -> io::Result<()> {
        self.update(owner, |users| {
            if can_type {
                users.insert(user.to_owned());
            } else {
                users.remove(user);
            }
        })
    }

    pub(crate) fn set_all(&self, owner: &str, users: BTreeSet<String>) -> io::Result<()> {
        self.update(owner, |current| *current = users)
    }

    pub(crate) fn remove_person(&self, user: &str) -> io::Result<()> {
        let owners: Vec<_> = self
            .lock()?
            .iter()
            .filter(|(owner, users)| owner.as_str() == user || users.contains_key(user))
            .map(|(owner, _)| owner.clone())
            .collect();
        let mut result = Ok(());
        for owner in owners {
            result = result.and(self.update(&owner, |users| {
                if owner == user {
                    users.clear();
                } else {
                    users.remove(user);
                }
            }));
        }
        result
    }

    pub(crate) fn permits(&self, owner: &str, user: &str) -> io::Result<bool> {
        Ok(self.permit(owner, user)?.is_some())
    }

    pub(crate) fn permit(&self, owner: &str, user: &str) -> io::Result<Option<Arc<Permit>>> {
        Ok(self
            .lock()?
            .get(owner)
            .and_then(|users| users.get(user))
            .cloned())
    }

    fn update(&self, owner: &str, change: impl FnOnce(&mut BTreeSet<String>)) -> io::Result<()> {
        let mut owners = self.lock()?;
        let grants = owners.entry(owner.to_owned()).or_default();
        let mut users = grants.keys().cloned().collect();
        change(&mut users);
        let mut result =
            write_json_atomically(&self.directory.join(format!("{owner}.json")), &users);
        let mut cancelled = Vec::new();
        for (user, grant) in grants.iter() {
            if !users.contains(user) {
                match grant.revoke() {
                    Ok(streams) => cancelled.extend(streams),
                    Err(error) => result = result.and(Err(error)),
                }
            }
        }
        grants.retain(|user, _| users.contains(user));
        if result.is_ok() {
            for user in users {
                grants
                    .entry(user)
                    .or_insert_with(|| Arc::new(Permit::default()));
            }
        }
        drop(owners);
        permit::cancel(cancelled);
        result
    }

    pub(crate) fn message(&self, user: &str) -> io::Result<ServerMsg> {
        let owners = self.lock()?;
        Ok(ServerMsg::Grants {
            can_type_here: owners
                .get(user)
                .into_iter()
                .flat_map(|users| users.keys())
                .cloned()
                .collect(),
            you_may_type_into: owners
                .iter()
                .filter(|(_, users)| users.contains_key(user))
                .map(|(owner, _)| owner.clone())
                .collect(),
        })
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, Owners>> {
        self.owners
            .lock()
            .map_err(|_| io::Error::other("grant lock is poisoned"))
    }
}
