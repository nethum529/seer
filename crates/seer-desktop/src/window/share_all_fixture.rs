use std::{io, net::TcpStream};

use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};

pub(super) fn reply(socket: &mut TcpStream, message: &ClientMsg) -> io::Result<()> {
    match message {
        ClientMsg::ListPeople => {
            codec::encode(
                socket,
                &ServerMsg::People {
                    people: ["Host", "Kyler", "Maya"]
                        .map(|name| Person {
                            user_id: if name == "Host" {
                                "host".into()
                            } else {
                                name.into()
                            },
                            name: name.into(),
                            host: name == "Host",
                            online: true,
                            idle_secs: 0,
                            attached_clients: 1,
                            peekable: true,
                            state: PersonState::Active,
                            tabs: 0,
                            foreground: String::new(),
                        })
                        .into(),
                },
            )?;
            codec::encode(
                socket,
                &ServerMsg::Grants {
                    can_type_here: Vec::new(),
                    you_may_type_into: Vec::new(),
                },
            )
        }
        ClientMsg::SetAllGrants {
            can_type,
            request_id: Some(request_id),
        } => {
            println!("native room received SetAllGrants can_type={can_type}");
            codec::encode(
                socket,
                &ServerMsg::Grants {
                    can_type_here: if *can_type {
                        vec!["Kyler".into(), "Maya".into()]
                    } else {
                        Vec::new()
                    },
                    you_may_type_into: Vec::new(),
                },
            )?;
            codec::encode(
                socket,
                &ServerMsg::AllGrantsResult {
                    request_id: *request_id,
                    error: None,
                },
            )
        }
        _ => Ok(()),
    }
}
