use std::collections::HashMap;

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{take_until, take_while, take_while1},
    character::complete::{alphanumeric1, char, multispace0},
    combinator::{iterator, opt, recognize},
    multi::many0,
    sequence::{delimited, terminated},
};

fn section(i: &str) -> IResult<&str, &str> {
    delimited(char('['), take_until("]"), char(']')).parse(i)
}

#[test]
fn test_section() {
    assert_eq!(section("[test]").unwrap().1, "test");
    assert_eq!(section("[test]]").unwrap().1, "test");
    assert_eq!(section("[test]\n").unwrap().1, "test");
    assert!(section("[test").is_err());
    assert!(section("test").is_err());
    assert!(section("test]").is_err());
}

fn kv(i: &str) -> IResult<&str, (&str, Option<&str>)> {
    let (i, name) = recognize(alt((
        alphanumeric1,
        // comments
        delimited(char('#'), take_while(|c| c != '\n'), char('\n')),
    )))
    .parse(i)?;
    if name.starts_with('#') {
        return Ok((i, (name, None)));
    }

    let eq = (many0(char(' ')), char('='), many0(char(' ')));
    let value = take_while1(|c| c != '\n' && c != ';');
    let trailer = take_while(|c| c == '\n' || c == ';');

    let (i, val) = opt((eq, value, trailer)).parse(i)?;
    let val = val.map(|(_, v, _)| v);

    Ok((i, (name, val)))
}

#[test]
fn test_kv() {
    assert_eq!(kv("a=b"), Ok(("", ("a", "b".into()))));
    assert_eq!(kv("a=b b2 b3"), Ok(("", ("a", "b b2 b3".into()))));
    assert_eq!(kv("a=b  "), Ok(("", ("a", "b  ".into()))));
    assert_eq!(kv("a   =   b"), Ok(("", ("a", "b".into()))));
    assert_eq!(kv("a=b\n\n\n"), Ok(("", ("a", "b".into()))));
    assert_eq!(kv("a\n=\nb"), Ok(("\n=\nb", ("a", None))));
}

fn key_value_map(i: &str) -> IResult<&str, HashMap<&str, Vec<&str>>> {
    let mut i = iterator(i, terminated(kv, opt(multispace0)));
    // skip comments

    let mut ret: HashMap<&str, Vec<&str>> = HashMap::new();
    for (k, v) in i.by_ref().filter(|(n, _)| !n.starts_with('#')) {
        ret.entry(k).or_default().extend(v);
    }

    i.finish().map(|(i, ())| (i, ret))
}

#[test]
fn test_kvm() {
    let parse = key_value_map("a=b; b=c; d=e").unwrap();
    assert_eq!(parse.0, "");
    assert_eq!(parse.1["a"], vec!("b"));
    assert_eq!(parse.1["b"], vec!("c"));
    assert_eq!(parse.1["d"], vec!("e"));
    let parse = key_value_map("a=b\n b=c\n d=e").unwrap();
    assert_eq!(parse.0, "");
    assert_eq!(parse.1["a"], vec!("b"));
    assert_eq!(parse.1["b"], vec!("c"));
    assert_eq!(parse.1["d"], vec!("e"));
}

fn sec_kv_map(i: &str) -> IResult<&str, Config<'_>> {
    let (i, prelude) = opt(key_value_map).parse(i)?;
    let mut i = iterator(i, (terminated(section, opt(multispace0)), key_value_map));
    let mut ret: HashMap<_, _> = i.by_ref().collect();
    if let Some(prelude) = prelude {
        ret.insert("", prelude);
    }
    i.finish().map(|(i, ())| (i, ret))
}

/// Section -> (Key -> List<Value>)
type Config<'c> = HashMap<&'c str, HashMap<&'c str, Vec<&'c str>>>;

// Parses the string as a pacman-flavored ini file.
// Key-Value pairs outside of an explicit section are retrievable under the "" section.
fn parse_pacman_config(i: &str) -> Result<Config<'_>, nom::Err<nom::error::Error<&str>>> {
    sec_kv_map(i).map(|(_, v)| v)
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
fn test_sec_kv_map() {
    let parse = sec_kv_map("a=0\n#b=9\n\n[a]a=1;b=2;c=3\n[b]a=-1;b=-2;c=-3\n[c]a=1;a=2");
    dbg!(&parse);
    use nom::Finish;
    let parse = parse.finish().unwrap();
    assert_eq!(parse.1["a"]["c"], vec!("3"));
    assert_eq!(parse.1["b"]["c"], vec!("-3"));
    assert_eq!(parse.1["c"]["a"], vec!("1", "2"));
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
