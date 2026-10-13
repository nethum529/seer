use super::*;

impl Coordinator<'_> {
    pub(super) fn runtime(&mut self, user: &str) -> io::Result<&mut RuntimeConnection> {
        if !self.runtimes.contains_key(user) {
            let mut runtime = self.connect_runtime(user)?;
            runtime.restoring = Some(
                self.watches
                    .keys()
                    .filter(|(target, _)| target == user)
                    .map(|(_, pane)| pane.clone())
                    .collect(),
            );
            self.runtimes.insert(user.to_owned(), runtime);
            if let Err(error) = self.restore_next_watch(user) {
                self.close_runtime(user)?;
                return Err(error);
            }
        }
        self.runtimes
            .get_mut(user)
            .ok_or_else(|| io::Error::other("runtime connection is missing"))
    }

    pub(super) fn watch_sent(&mut self, user: &str, identity: &Arc<()>) -> io::Result<()> {
        if !self
            .runtimes
            .get(user)
            .is_some_and(|r| Arc::ptr_eq(&r.identity, identity))
        {
            return Ok(());
        }
        if let Err(error) = self.restore_next_watch(user) {
            self.runtime_lost(user, &error)?;
            self.write(&ServerMsg::Refused {
                reason: error.to_string(),
            })?;
        }
        Ok(())
    }

    fn restore_next_watch(&mut self, user: &str) -> io::Result<()> {
        let Some(runtime) = self.runtimes.get_mut(user) else {
            return Ok(());
        };
        let Some(panes) = &mut runtime.restoring else {
            return Ok(());
        };
        while let Some(pane) = panes.pop_front() {
            if let Some((size, viewer)) = self.watches.get(&(user.into(), pane.clone())) {
                return runtime.restore_watch(&ClientMsg::Watch {
                    user: user.into(),
                    pane,
                    cols: size.cols,
                    rows: size.rows,
                    viewer: *viewer,
                });
            }
        }
        runtime.restoring = None;
        Ok(())
    }
}
