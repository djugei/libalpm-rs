use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{take_until, take_while, take_while1},
    character::complete::{alphanumeric1, char, multispace0},
    combinator::{iterator, opt, recognize},
    multi::many0,
    sequence::{delimited, terminated},
};
use std::collections::HashMap;

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

pub(super) fn sec_kv_map(i: &str) -> IResult<&str, Config<'_>> {
    let (i, prelude) = opt(key_value_map).parse(i)?;
    let mut i = iterator(i, (terminated(section, opt(multispace0)), key_value_map));
    let mut ret: HashMap<_, _> = i.by_ref().collect();
    if let Some(prelude) = prelude {
        ret.insert("", prelude);
    }
    i.finish().map(|(i, ())| (i, ret))
}

/// Section -> (Key -> List<Value>)
pub type Config<'c> = HashMap<&'c str, HashMap<&'c str, Vec<&'c str>>>;

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
