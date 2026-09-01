use std::io::{self, IsTerminal, Write};

pub(crate) fn hidden(label: &str) -> io::Result<String> {
    hidden_with(label, io::stdin().is_terminal(), |prompt| {
        rpassword::prompt_password(prompt)
    })
}

pub(crate) fn visible(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value.trim().to_owned())
}

fn hidden_with(
    label: &str,
    terminal: bool,
    read_password: impl FnOnce(&str) -> io::Result<String>,
) -> io::Result<String> {
    if terminal {
        read_password(label)
    } else {
        visible(label)
    }
}

#[cfg(test)]
mod tests {
    use super::hidden_with;

    #[test]
    fn terminal_prompt_uses_the_hidden_reader() {
        let value = hidden_with("Invitation: ", true, |label| {
            assert_eq!(label, "Invitation: ");
            Ok("secret".into())
        })
        .expect("hidden prompt must succeed");

        assert_eq!(value, "secret");
    }
}
