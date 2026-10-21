use std::{fs, path::Path};

pub(super) const ANSI: [u32; 16] = [
    0x182838, 0xb04050, 0x40b060, 0xc0a050, 0x5070c0, 0xb050c0, 0x40b0c0, 0xc0c8d0, 0x607080,
    0xf08090, 0x80f0a0, 0xf0d090, 0x90b0f0, 0xf090f0, 0x80e0f0, 0xf0f8ff,
];
pub(super) const DEFAULTS: [u32; 16] = [
    0x262626, 0xf87171, 0x4ade80, 0xfacc15, 0x60a5fa, 0xc084fc, 0x22d3ee, 0xd4d4d4, 0x737373,
    0xfca5a5, 0x86efac, 0xfde047, 0x93c5fd, 0xd8b4fe, 0x67e8f9, 0xfafafa,
];
pub(super) const FG: u32 = 0xd0e0f0;
pub(super) const BG: u32 = 0x203040;
pub(super) const CURSOR: u32 = 0xe0a060;

pub(super) fn prepare(root: &Path) {
    let Ok(case) = std::env::var("COLOR_QA") else {
        return;
    };
    let config = root.join("config");
    let ghostty = format!(
        "font-family = JetBrains Mono\nforeground = #{FG:06x}\nbackground = {BG:06x}\ncursor-color = #{CURSOR:06x}\n{}",
        ANSI.iter()
            .enumerate()
            .map(|(i, c)| format!("palette = {i}=#{c:06x}\n"))
            .collect::<String>()
    );
    let kitty = format!(
        "font_family JetBrains Mono\nforeground #{FG:06x}\nbackground #{BG:06x}\ncursor #{CURSOR:06x}\n{}",
        ANSI.iter()
            .enumerate()
            .map(|(i, c)| format!("color{i} #{c:06x}\n"))
            .collect::<String>()
    );
    let mut alacritty = format!(
        "[font.normal]\nfamily = 'JetBrains Mono'\n[colors.primary]\nforeground = '#{FG:06x}'\nbackground = '#{BG:06x}'\n[colors.cursor]\ncursor = '#{CURSOR:06x}'\n"
    );
    for (offset, group) in [(0, "normal"), (8, "bright")] {
        alacritty.push_str(&format!("[colors.{group}]\n"));
        for (i, name) in [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ]
        .iter()
        .enumerate()
        {
            alacritty.push_str(&format!("{name} = '#{:06x}'\n", ANSI[offset + i]));
        }
    }
    match case.as_str() {
        "ghostty-named" => write(
            &config.join("ghostty/config"),
            &ghostty
                .replace("#d0e0f0", "Light Blue")
                .replace("203040", "gray10")
                .replace("#e0a060", "gold")
                .replace("0=#182838", "0x0=navy")
                .replace("1=#b04050", "0b1=#b45"),
        ),
        "kitty-rgb" => write(
            &config.join("kitty/kitty.conf"),
            &kitty.replace("#d0e0f0", "rgb:d0/e0/f0"),
        ),
        "ghostty" | "precedence" => write(&config.join("ghostty/config"), &ghostty),
        "ghostty-includes" => {
            write(
                &config.join("ghostty/config"),
                "background = ff0000\nconfig-file = colors.conf\n",
            );
            write(&config.join("ghostty/colors.conf"), &ghostty);
        }
        "kitty" => write(&config.join("kitty/kitty.conf"), &kitty),
        "kitty-includes" => {
            write(
                &config.join("kitty/kitty.conf"),
                "background #ff0000\ninclude colors.conf\ninclude missing.conf\n",
            );
            write(&config.join("kitty/colors.conf"), &kitty);
        }
        "alacritty" => write(&config.join("alacritty/alacritty.toml"), &alacritty),
        "alacritty-imports" => {
            write(
                &config.join("alacritty/alacritty.toml"),
                "[general]\nimport = ['colors.toml']\n[colors.primary]\nbackground = '#203040'\n",
            );
            write(
                &config.join("alacritty/colors.toml"),
                &alacritty.replace("#203040", "#ff0000"),
            );
        }
        "iterm2" | "iterm2-binary" | "iterm2-p3" | "iterm2-calibrated" => {
            let colors = ANSI
                .iter()
                .enumerate()
                .map(|(i, c)| (format!("Ansi {i} Color"), *c))
                .chain([
                    ("Foreground Color".into(), FG),
                    ("Background Color".into(), BG),
                    ("Cursor Color".into(), CURSOR),
                ]);
            let mut profile = plist::Dictionary::new();
            profile.insert("Guid".into(), "chosen".into());
            profile.insert("Normal Font".into(), "JetBrainsMono-Regular 12.5".into());
            for (key, color) in colors {
                let mut components = plist::Dictionary::new();
                components.insert("Color Space".into(), "sRGB".into());
                for (name, shift) in [
                    ("Red Component", 16),
                    ("Green Component", 8),
                    ("Blue Component", 0),
                ] {
                    components.insert(
                        name.into(),
                        (f64::from((color >> shift) & 255) / 255.).into(),
                    );
                }
                profile.insert(key, components.into());
            }
            let mut preferences = plist::Dictionary::new();
            preferences.insert("Default Bookmark Guid".into(), "chosen".into());
            preferences.insert(
                "New Bookmarks".into(),
                vec![plist::Value::Dictionary(Default::default()), profile.into()].into(),
            );
            let path = root.join("home/Library/Preferences/com.googlecode.iterm2.plist");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let value = plist::Value::Dictionary(preferences);
            if case == "iterm2-binary" {
                value.to_file_binary(&path).unwrap()
            } else {
                value.to_file_xml(&path).unwrap()
            }
            if matches!(case.as_str(), "iterm2-p3" | "iterm2-calibrated") {
                let code = r#"import AppKit
let path = CommandLine.arguments[1]
var root = try! PropertyListSerialization.propertyList(from: Data(contentsOf: URL(fileURLWithPath: path)), format: nil) as! [String: Any]
var profiles = root["New Bookmarks"] as! [[String: Any]]
for (key, value) in profiles[1] {
 guard var c = value as? [String: Any], let r = c["Red Component"] as? Double, let g = c["Green Component"] as? Double, let b = c["Blue Component"] as? Double else { continue }
 let target = CommandLine.arguments[2] == "iterm2-p3" ? NSColorSpace.displayP3 : NSColorSpace.genericRGB
 let result = NSColor(srgbRed:r,green:g,blue:b,alpha:1).usingColorSpace(target)!
 c["Red Component"] = result.redComponent; c["Green Component"] = result.greenComponent; c["Blue Component"] = result.blueComponent
 c["Color Space"] = CommandLine.arguments[2] == "iterm2-p3" ? "P3" : "Calibrated"
 profiles[1][key] = c
}
root["New Bookmarks"] = profiles
try! PropertyListSerialization.data(fromPropertyList:root, format:.xml, options:0).write(to:URL(fileURLWithPath:path))
"#;
                assert!(
                    std::process::Command::new("swift")
                        .args(["-e", code])
                        .arg(&path)
                        .arg(&case)
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
        "partial" => write(
            &config.join("ghostty/config"),
            "palette = 1=#b04050\nforeground = broken\nbackground = #12345\ncursor-color = invalid\n",
        ),
        "unreadable" => fs::create_dir_all(config.join("ghostty/config")).unwrap(),
        "malformed" => write(&config.join("alacritty/alacritty.toml"), "[colors\n"),
        "none" => {}
        _ => panic!("unknown color QA case"),
    }
    if matches!(case.as_str(), "precedence" | "partial" | "unreadable") {
        write(
            &config.join("kitty/kitty.conf"),
            &kitty.replace("#203040", "#ff0000"),
        );
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
