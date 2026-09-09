#![cfg(target_os = "linux")]

#[path = "support/binary.rs"]
mod binary;
mod room;
use room::*;

// Issue 338: a person who joins a remote room saves the endpoint as
// iroh:<key>, the capsule form. The runtime that this computer starts gets
// that saved value and must still publish.
#[test]
fn a_runtime_publishes_with_the_saved_iroh_endpoint() {
    let mut room = Room::start(true);
    let mut window = room.publish("alice", ALICE_SECRET);
    own_tree(&mut window);

    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    wait_for_published(&mut alice, "alice");
}
