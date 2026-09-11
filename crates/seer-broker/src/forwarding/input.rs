use super::*;

impl Coordinator<'_> {
    pub(super) fn set_all_grants(&self, can_type: bool) -> io::Result<()> {
        let users = if can_type {
            self.broker
                .registry()
                .people()?
                .into_iter()
                .map(|person| person.user_id)
                .filter(|user| user != &self.owner.user_id)
                .collect()
        } else {
            BTreeSet::new()
        };
        self.broker.grants.set_all(&self.owner.user_id, users)?;
        self.broker.publish_grants()?;
        self.write(&ServerMsg::GrantsUpdated)
    }

    pub(super) fn set_grant(&self, user: &str, can_type: bool) -> io::Result<()> {
        self.person(user)?;
        seer_core::debug_log!(
            "grant owner={} user={user} can_type={can_type}",
            self.owner.user_id
        );
        self.broker
            .grants
            .set(&self.owner.user_id, user, can_type)?;
        self.broker.publish_grants()
    }

    pub(super) fn mouse_into(
        &mut self,
        user: &str,
        pane: &str,
        mouse: seer_core::MouseInput,
    ) -> io::Result<()> {
        self.require_grant(user)?;
        let (workspace, tab) = self.runtime(user)?.location(pane)?;
        let sender = self.owner.name.clone();
        self.runtime(user)?.send(&ClientMsg::GrantedMouse {
            workspace,
            tab,
            pane: pane.into(),
            mouse,
            sender,
        })
    }

    fn require_grant(&self, user: &str) -> io::Result<()> {
        let person = self.person(user)?;
        if self.broker.grants.permits(user, &self.owner.user_id)? {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{} has not let you type",
                person.name
            )))
        }
    }

    pub(super) fn type_into(&mut self, user: &str, pane: &str, bytes: Vec<u8>) -> io::Result<()> {
        self.require_grant(user)?;
        let (workspace, tab) = self.runtime(user)?.location(pane)?;
        seer_core::debug_log!(
            "input forwarded from={} to={user} pane={pane} bytes={}",
            self.owner.user_id,
            bytes.len()
        );
        let sender = self.owner.name.clone();
        self.runtime(user)?.send(&ClientMsg::GrantedInput {
            workspace,
            tab,
            pane: pane.into(),
            bytes,
            sender,
        })
    }
}
