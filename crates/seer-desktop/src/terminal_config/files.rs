use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

fn check_depth(depth: usize) -> io::Result<()> {
    if depth >= 16 {
        return Err(io::Error::other("config include limit"));
    }
    Ok(())
}

pub(super) fn lines(path: &Path, ghostty: bool, depth: usize) -> io::Result<Vec<(String, String)>> {
    check_depth(depth)?;
    let contents = fs::read_to_string(path)?;
    let mut values = Vec::new();
    let mut includes = Vec::new();
    let mut physical = contents.lines().peekable();
    while let Some(line) = physical.next() {
        let mut line = line.trim_start().to_string();
        if !ghostty {
            while let Some(next) = physical.next_if(|line| line.trim_start().starts_with('\\')) {
                line.push_str(&next.trim_start()[1..]);
            }
        }
        let Some((key, value)) = pair(&line, ghostty) else {
            continue;
        };
        if ghostty && key == "config-file" {
            includes.push(value.to_string());
        } else if !ghostty && key == "include" {
            values.extend(lines(&include_path(path, value), false, depth + 1).unwrap_or_default());
        } else {
            values.push((key.to_string(), value.to_string()));
        }
    }
    for include in includes {
        let (optional, include) = include
            .strip_prefix('?')
            .map_or((false, include.as_str()), |path| (true, path));
        match lines(&include_path(path, include), true, depth + 1) {
            Err(error) if optional && error.kind() == io::ErrorKind::NotFound => {}
            result => values.extend(result?),
        }
    }
    Ok(values)
}

fn pair(line: &str, ghostty: bool) -> Option<(&str, &str)> {
    if line.starts_with('#') {
        return None;
    }
    let (key, value) = if ghostty {
        line.split_once('=')
    } else {
        line.split_once(char::is_whitespace)
    }?;
    Some((key.trim(), unquote(value.trim())))
}

pub(super) fn toml(path: &Path, depth: usize) -> io::Result<toml::Value> {
    check_depth(depth)?;
    let contents = fs::read_to_string(path)?;
    let current: toml::Value = toml::from_str(&contents).map_err(io::Error::other)?;
    let imports = current
        .get("general")
        .and_then(|general| general.get("import"))
        .or_else(|| current.get("import"))
        .and_then(toml::Value::as_array);
    let mut values = toml::Value::Table(Default::default());
    for import in imports
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
    {
        match toml(&include_path(path, import), depth + 1) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            result => merge(&mut values, result?),
        }
    }
    merge(&mut values, current);
    Ok(values)
}

fn merge(base: &mut toml::Value, value: toml::Value) {
    if let (Some(base), Some(value)) = (base.as_table_mut(), value.as_table()) {
        for (key, value) in value {
            if let Some(previous) = base.get_mut(key) {
                merge(previous, value.clone());
            } else {
                base.insert(key.clone(), value.clone());
            }
        }
    } else {
        *base = value;
    }
}

fn include_path(parent: &Path, value: &str) -> PathBuf {
    if let Some(value) = value.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(value);
    }
    parent.parent().unwrap_or(Path::new(".")).join(value)
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

pub(super) fn font_selector(value: &str) -> Option<String> {
    if !value.contains('=') {
        return Some(unquote(value).to_string()).filter(|value| !value.is_empty());
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    for ch in value.chars() {
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            ch if ch.is_whitespace() && quote.is_none() => {
                words.push(std::mem::take(&mut word));
            }
            ch => word.push(ch),
        }
    }
    words.push(word);
    words.into_iter().find_map(|word| {
        let (key, value) = word.split_once('=')?;
        matches!(key, "family" | "postscript_name" | "full_name" | "system")
            .then(|| value.to_string())
            .filter(|value| !value.is_empty())
    })
}
