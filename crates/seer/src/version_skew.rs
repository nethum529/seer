use seer_core::version::{major_minor, version_mismatch};

/// The two versions in a refusal from a room server of another minor version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VersionRefusal {
    pub(crate) server: String,
    pub(crate) client: String,
}

pub(crate) fn version_refusal(reason: &str) -> Option<VersionRefusal> {
    let (server, client) = version_mismatch(reason)?;
    Some(VersionRefusal {
        server: server.to_owned(),
        client: client.to_owned(),
    })
}

impl VersionRefusal {
    fn client_is_newer(&self) -> bool {
        major_minor(&self.client) > major_minor(&self.server)
    }

    /// The step that ends the mismatch. A newer Seer waits for the room owner
    /// to update, or stops and starts the room server when this computer runs
    /// it. An older Seer must update.
    pub(crate) fn advice(&self, room_runs_here: bool) -> String {
        let step = match (self.client_is_newer(), room_runs_here) {
            (true, true) => "Stop and start the room server: seer stop, then seer start.",
            (true, false) => "The room owner must update.",
            (false, _) => "Run: seer update",
        };
        format!(
            "The room runs Seer {}. You run Seer {}. {step}",
            self.server, self.client
        )
    }
}

/// The message for a refusal from a room server, with the shared rule applied
/// to a version refusal.
pub(crate) fn refusal_message(reason: &str, room_runs_here: bool) -> String {
    match version_refusal(reason) {
        Some(refusal) => refusal.advice(room_runs_here),
        None => format!("refused: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{refusal_message, version_refusal};

    #[test]
    fn the_rule_names_the_side_that_must_update() {
        let room_is_older = "version mismatch: server 0.5.7, client 0.6.0. Run: seer update";
        assert_eq!(
            refusal_message(room_is_older, false),
            "The room runs Seer 0.5.7. You run Seer 0.6.0. The room owner must update."
        );
        assert_eq!(
            refusal_message(room_is_older, true),
            "The room runs Seer 0.5.7. You run Seer 0.6.0. Stop and start the room server: seer stop, then seer start."
        );
        let room_is_newer = "version mismatch: server 0.7.0, client 0.6.0. Run: seer update";
        assert_eq!(
            refusal_message(room_is_newer, true),
            "The room runs Seer 0.7.0. You run Seer 0.6.0. Run: seer update"
        );
        assert_eq!(
            refusal_message("invalid credentials", true),
            "refused: invalid credentials"
        );
        assert_eq!(
            version_refusal("version mismatch: server 0.6.1, client 0.6.0"),
            None
        );
    }
}
