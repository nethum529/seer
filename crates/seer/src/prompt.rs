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

pub(crate) fn visible_with_default(label: &str, default: Option<&str>) -> io::Result<String> {
    let prompt = match (io::stdin().is_terminal(), default) {
        (false, _) => format!("{label}: "),
        (true, Some(default)) => format!("{label} [{default}]: "),
        (true, None) => format!("{label}: "),
    };
    let value = visible(&prompt)?;
    if value.is_empty() {
        Ok(default.unwrap_or_default().to_owned())
    } else {
        Ok(value)
    }
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
