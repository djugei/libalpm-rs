use std::collections::HashMap;

mod parse;
use parse::Config;

// Parses the string as a pacman-flavored ini file.
// Key-Value pairs outside of an explicit section are retrievable under the "" section.
fn parse_pacman_config(i: &str) -> Result<Config<'_>, nom::Err<nom::error::Error<&str>>> {
    parse::sec_kv_map(i).map(|(_, v)| v)
}

fn try_remove_first<T>(mut vec: Vec<T>) -> Option<T> {
    if vec.is_empty() {
        None
    } else {
        Some(vec.remove(0))
    }
}

/// Reads the pacman config and extracts relevant information.
/// Resolves one level of Include.
/// Does not support glob syntax in includes.
/// ret: (list of ignored packages, repo -> url)
pub fn extract_relevant_config() -> (Vec<String>, HashMap<String, String>) {
    let pacman_config = std::fs::read_to_string("/etc/pacman.conf").unwrap();
    let mut pacman_config = parse_pacman_config(&pacman_config).unwrap();
    let arch = pacman_config["options"]["Architecture"]
        .first()
        .map(|s| s.trim());
    let arch = match arch {
        Some("auto") | None => std::env::consts::ARCH,
        Some("x86_64") => "x86_64",
        _ => panic!("unknown architecture"),
    };
    let ignores = pacman_config
        .get_mut("options")
        .and_then(|m| m.remove("IgnorePkg").and_then(try_remove_first));
    let ignores: Vec<String> = if let Some(ignores) = ignores {
        ignores
            .trim()
            .split(' ')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    } else {
        Vec::new()
    };
    let mut repos = HashMap::new();
    for (k, mut v) in pacman_config {
        if k.is_empty() || k == "options" {
            continue;
        }
        let server = v
            .remove("Server")
            .and_then(try_remove_first)
            .map(ToOwned::to_owned)
            .or_else(|| {
                v.remove("Include").and_then(|v| {
                    v.into_iter()
                        .filter_map(|i| {
                            let s = std::fs::read_to_string(i).unwrap();
                            let mut inc = parse_pacman_config(&s).unwrap();
                            inc.get_mut("")
                                .unwrap()
                                .remove("Server")
                                .and_then(try_remove_first)
                                .map(ToOwned::to_owned)
                        })
                        .next()
                })
            })
            .unwrap();
        let server = server.replace("$arch", arch).replace("$repo", k);
        repos.insert(k.to_owned(), server);
    }

    (ignores, repos)
}

#[test]
fn pacman_conf() {
    let i = std::fs::read_to_string("/etc/pacman.conf").unwrap();
    let m = parse_pacman_config(&i).unwrap();
    println!("{m:#?}");

    let i = std::fs::read_to_string("/etc/pacman.d/mirrorlist").unwrap();
    let m = parse_pacman_config(&i).unwrap();
    println!("{m:#?}");
}
