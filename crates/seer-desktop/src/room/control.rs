use futures::channel::oneshot;

use super::*;

impl Room {
    pub(crate) fn set_all_grants(
        &mut self,
        can_type: bool,
    ) -> oneshot::Receiver<Result<(), &'static str>> {
        let (reply, response) = oneshot::channel();
        let error = if !self.can_share() {
            Some("Cannot share control now. Try again.")
        } else if self
            .writer
            .as_ref()
            .is_some_and(|writer| writer.send(ClientMsg::SetAllGrants { can_type }).is_ok())
        {
            None
        } else {
            Some("Room is offline. Try again.")
        };
        self.control_error = None;
        if let Some(error) = error {
            let _ = reply.send(Err(error));
        } else {
            self.all_grants = Some(reply);
        }
        response
    }

    pub(crate) fn finish_all_grants(&mut self, result: Result<(), &'static str>) {
        if let Some(reply) = self.all_grants.take() {
            self.control_error = result.err();
            let _ = reply.send(result);
        }
    }
}
