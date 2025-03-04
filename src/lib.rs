use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use nom::bytes::complete::tag;
use nom::bytes::complete::take_until;
use nom::character::complete::{alpha1, char, newline};
use nom::error::Error;
use nom::multi::separated_list0;
use nom::sequence::{delimited, pair};
use nom::{IResult, Parser};
use string_interner::DefaultStringInterner;
use string_interner::DefaultSymbol as Istr;
//check alpm version on entry functions

type Str = Box<str>;

type Interner = Rc<RefCell<DefaultStringInterner>>;

enum Validation {
    None,
    PGP,
}

impl FromStr for Validation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "pgp" => Ok(Self::PGP),
            _ => Err(()),
        }
    }
}

enum Arch {
    X86_64,
    Any,
}

impl FromStr for Arch {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x86_64" => Ok(Self::X86_64),
            "any" => Ok(Self::Any),
            _ => Err(()),
        }
    }
}

enum XData {
    Pkg,
    Split,
}

impl FromStr for XData {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pkgtype=pkg" => Ok(Self::Pkg),
            "pkgtype=split" => Ok(Self::Split),
            _ => Err(()),
        }
    }
}

// builddate installdate, size, reason
pub struct Package {
    interner: Interner,
    base: Istr,
    name: Istr,
    version: Istr,
    arch: Arch,

    //isize: Option<u64>,
    reason: u8,
    install_date: SystemTime,

    packager: Istr,
    size: u64,
    build_date: SystemTime,
    url: Istr,
    license: Vec<Istr>,
    desc: Istr,
    validation: Validation,

    provides: Vec<Istr>,
    depends: Vec<Istr>,
    optdepends: Vec<Istr>,
    groups: Option<Vec<Istr>>,
    replaces: Option<Vec<Istr>>,
    conflicts: Option<Vec<Istr>>,

    xdata: Option<XData>,
}

impl Package {
    pub fn from_str(i: Interner, s: &str) -> Result<Self, ()> {
        let m = map(s).unwrap();
        fn str_to_systemtime(s: &&str) -> SystemTime {
            let u: u64 = s.parse().unwrap();
            UNIX_EPOCH + Duration::from_millis(u)
        }
        let intern = |s| m.get(s).map(|s| i.borrow_mut().get_or_intern(s));
        let intern_list = |s: &str| {
            m.get(s)
                .map(|s| s.split('\n').map(|l| i.borrow_mut().get_or_intern(l)))
        };
        let s = Self {
            base: intern("BASE").unwrap(),
            name: intern("NAME").unwrap(),
            version: intern("VERSION").unwrap(),
            arch: m.get("ARCH").map(|s| Arch::from_str(s)).unwrap().unwrap(),
            reason: m.get("REASON").map(|s| u8::from_str(s)).unwrap().unwrap(),
            install_date: m.get("INSTALLDATE").map(str_to_systemtime).unwrap(),
            packager: intern("PACKAGER").unwrap(),
            build_date: m.get("BUILDDATE").map(str_to_systemtime).unwrap(),
            url: intern("URL").unwrap(),
            license: intern_list("LICENSE").unwrap().collect(),
            desc: intern("DESC").unwrap(),
            size: m.get("SIZE").map(|s| u64::from_str(s).unwrap()).unwrap(),
            validation: m
                .get("VALIDATION")
                .map(|s| Validation::from_str(s))
                .unwrap()
                .unwrap(),
            depends: intern_list("DEPENDS").unwrap().collect(),
            optdepends: intern_list("OPTDEPENDS").unwrap().collect(),
            provides: intern_list("PROVIDES").unwrap().collect(),

            groups: intern_list("GROUPS").map(|m| m.collect()),
            replaces: intern_list("REPLACES").map(|m| m.collect()),
            conflicts: intern_list("CONFLICTS").map(|m| m.collect()),
            xdata: m.get("XDATA").map(|s| XData::from_str(s).unwrap()),
            interner: i,
        };
        Ok(s)
    }
}

fn entry(i: &str) -> IResult<&str, (&str, &str)> {
    let header = delimited(char('%'), alpha1, pair(char('%'), newline));
    let t = take_until("\n\n");
    (header, t).parse(i)
}

fn list(i: &str) -> IResult<&str, Vec<(&str, &str)>> {
    separated_list0(tag("\n\n"), entry).parse(i)
}

fn map(i: &str) -> Result<HashMap<&str, &str>, nom::Err<Error<&str>>> {
    let (r, h) = list(i).map(|(r, v)| (r, v.into_iter().collect()))?;
    assert_eq!(r, "\n\n");
    Ok(h)
}

//TODO: work on tars

#[test]
fn test_entry() {
    use std::io::Read;
    let mut f = std::fs::File::open("desc").unwrap();
    let mut s = Default::default();
    f.read_to_string(&mut s).unwrap();
    let (_, (h, v)) = entry(&s).unwrap();
    dbg!(h, v);
}

#[test]
fn test_list() {
    use std::io::Read;
    let mut f =
        std::fs::File::open("/var/lib/pacman/local/deltaclient-git-r129.60fbd27-1/desc").unwrap();
    let mut s = Default::default();
    f.read_to_string(&mut s).unwrap();
    let (r, l) = list(&s).unwrap();
    dbg!(l, r);
}

#[test]
fn test_local() {
    use std::io::Read;
    let mut s = String::new();
    let i = Interner::default();
    let mut fulllen = 0usize;
    for dir in std::fs::read_dir("/var/lib/pacman/local/").unwrap() {
        let dir = dir.unwrap();
        if !dir.metadata().unwrap().is_dir() {
            continue;
        }
        let desc = dir.path().join("desc");

        let mut f = std::fs::File::open(desc).unwrap();
        f.read_to_string(&mut s).unwrap();
        fulllen += map(&s).unwrap().len();
        let _pkg = Package::from_str(i.clone(), &s).unwrap();
    }
    i.borrow_mut().shrink_to_fit();
    println!("l: {}", i.borrow().len());
    println!("fulllen: {fulllen}");
}
