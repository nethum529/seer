use super::*;

impl Coordinator<'_> {
    pub(super) fn handle_control(&mut self, message: ClientMsg) -> io::Result<()> {
        match message {
            ClientMsg::MouseInto { user, pane, mouse } => self.mouse_into(&user, &pane, mouse),
            ClientMsg::TypeInto { user, pane, bytes } => self.type_into(&user, &pane, bytes),
            ClientMsg::InputInto { user, pane, input } => self.input_into(&user, &pane, input),
            ClientMsg::SetAllGrants { can_type } => self.set_all_grants(can_type),
            ClientMsg::SetGrant { user, can_type } => self.set_grant(&user, can_type),
            _ => Err(io::Error::other("unsupported control message")),
        }
    }

    pub(super) fn input_into(
        &mut self,
        user: &str,
        pane: &str,
        input: seer_core::TerminalInput,
    ) -> io::Result<()> {
        if matches!(input.event, seer_core::InputEvent::Scrollback { .. }) {
            return Err(io::Error::other("scrollback belongs to the viewer"));
        }
        self.forward_input(user, pane, |workspace, tab, sender| {
            ClientMsg::GrantedTerminalInput {
                workspace,
                tab,
                pane: pane.into(),
                input,
                sender,
            }
        })
    }

    pub(super) fn set_all_grants(&self, can_type: bool) -> io::Result<()> {
        let membership = self.broker.membership()?;
        self.broker.require_member(&self.owner.user_id)?;
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
        drop(membership);
        self.broker.publish_grants()?;
        self.write(&ServerMsg::GrantsUpdated)
    }

    pub(super) fn set_grant(&self, user: &str, can_type: bool) -> io::Result<()> {
        let membership = self.broker.membership()?;
        self.broker.require_member(&self.owner.user_id)?;
        self.person(user)?;
        seer_core::debug_log!(
            "grant owner={} user={user} can_type={can_type}",
            self.owner.user_id
        );
        self.broker
            .grants
            .set(&self.owner.user_id, user, can_type)?;
        drop(membership);
        self.broker.publish_grants()
    }

    pub(super) fn mouse_into(
        &mut self,
        user: &str,
        pane: &str,
        mouse: seer_core::MouseInput,
    ) -> io::Result<()> {
        self.forward_input(user, pane, |workspace, tab, sender| {
            ClientMsg::GrantedMouse {
                workspace,
                tab,
                pane: pane.into(),
                mouse,
                sender,
            }
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
        self.forward_input(user, pane, |workspace, tab, sender| {
            ClientMsg::GrantedInput {
                workspace,
                tab,
                pane: pane.into(),
                bytes,
                sender,
            }
        })
    }

    fn forward_input(
        &mut self,
        user: &str,
        pane: &str,
        message: impl FnOnce(String, String, String) -> ClientMsg,
    ) -> io::Result<()> {
        self.require_grant(user)?;
        let broker = self.broker;
        let owner = self.owner;
        let runtime = self.runtime(user)?;
        let (workspace, tab) = runtime.location(pane)?;
        let message = message(workspace, tab, owner.name.clone());
        let permit = broker.grants.permit(user, &owner.user_id)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::PermissionDenied, "Control was revoked")
        })?;
        runtime.send_granted(&message, permit)
    }
}
