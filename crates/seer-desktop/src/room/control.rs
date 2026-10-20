use futures::channel::oneshot;

use super::*;

impl Room {
    pub(crate) fn set_all_grants(
        &mut self,
        can_type: bool,
    ) -> oneshot::Receiver<Result<(), &'static str>> {
        let (reply, response) = oneshot::channel();
        let request_id = self.next_grants_id.checked_add(1);
        let error = if !self.can_share() || request_id.is_none() {
            Some("Cannot share control now. Try again.")
        } else if self.writer.as_ref().is_some_and(|writer| {
            writer
                .send(ClientMsg::SetAllGrants {
                    can_type,
                    request_id,
                })
                .is_ok()
        }) {
            None
        } else {
            Some("Room is offline. Try again.")
        };
        self.control_error = None;
        if let Some(error) = error {
            let _ = reply.send(Err(error));
        } else if let Some(request_id) = request_id {
            self.next_grants_id = request_id;
            self.all_grants = Some((request_id, reply));
        }
        response
    }

    pub(super) fn all_grants_reply(&mut self, request_id: u64, error: Option<String>) {
        if self
            .all_grants
            .as_ref()
            .is_some_and(|(pending, _)| *pending == request_id)
        {
            self.finish_all_grants(
                error.map_or(Ok(()), |_| Err("Share control was refused. Try again.")),
            );
        }
    }

    pub(crate) fn finish_all_grants(&mut self, result: Result<(), &'static str>) {
        if let Some((_, reply)) = self.all_grants.take() {
            self.control_error = result.err();
            let _ = reply.send(result);
        }
    }
}
