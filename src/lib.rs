use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base64::Engine;
use base64::prelude::BASE64_STANDARD_NO_PAD as B64;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::{alphanumeric1, char, newline};
use nom::error::Error;
use nom::multi::separated_list0;
use nom::sequence::{delimited, pair};
use nom::{IResult, Parser};
use string_interner::DefaultStringInterner;
use string_interner::DefaultSymbol as Istr;
use string_interner::StringInterner;
//check alpm version on entry functions

type Interner = Rc<RefCell<DefaultStringInterner>>;

pub enum Validation {
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

pub enum Arch {
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

pub enum XData {
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

pub struct Package {
    pub interner: Interner,
    pub base: Istr,
    pub name: Istr,
    pub version: Istr,
    pub arch: Arch,

    pub reason: Option<u8>,
    pub install_date: Option<SystemTime>,
    pub validation: Option<Validation>,

    pub packager: Istr,
    pub isize: Option<u64>,
    pub csize: Option<u64>,
    pub build_date: SystemTime,
    pub url: Istr,
    pub license: Vec<Istr>,
    pub desc: Istr,
    pub filename: Option<Istr>,
    pub md5sum: Option<[u8; 24]>,
    pub sha256sum: Option<[u8; 48]>,
    //TODO: direct value
    pub pgpsig: Option<Istr>,

    pub provides: Option<Vec<Istr>>,
    pub depends: Option<Vec<Istr>>,
    pub optdepends: Option<Vec<Istr>>,
    pub makedepends: Option<Vec<Istr>>,
    pub checkdepends: Option<Vec<Istr>>,
    pub groups: Option<Vec<Istr>>,
    pub replaces: Option<Vec<Istr>>,
    pub conflicts: Option<Vec<Istr>>,

    pub xdata: Option<XData>,
}

impl Package {
    pub fn from_str(i: Interner, s: &str) -> Result<Self, ()> {
        use std::cell::RefMut;
        let m = map(s).unwrap();
        //TODO: clone can be avoided if the package construction is done in 2 steps
        let ii = i.clone();
        let mut ir = i.borrow_mut();
        fn str_to_systemtime(s: &&str) -> SystemTime {
            let u: u64 = s.parse().unwrap();
            UNIX_EPOCH + Duration::from_millis(u)
        }
        let intern =
            |s, ir: &mut RefMut<'_, StringInterner<_>>| m.get(s).map(|s| ir.get_or_intern(s));
        let intern_list = |s: &str, ir: &mut RefMut<'_, StringInterner<_>>| {
            m.get(s).map(move |s| {
                s.split('\n')
                    .map(|l| ir.get_or_intern(l))
                    .collect::<Vec<_>>()
            })
        };
        let s = Self {
            base: intern("BASE", &mut ir).unwrap(),
            name: intern("NAME", &mut ir).unwrap(),
            version: intern("VERSION", &mut ir).unwrap(),
            arch: m.get("ARCH").map(|s| Arch::from_str(s).unwrap()).unwrap(),
            reason: m.get("REASON").map(|s| u8::from_str(s).unwrap()),
            install_date: m.get("INSTALLDATE").map(str_to_systemtime),
            packager: intern("PACKAGER", &mut ir).unwrap(),
            build_date: m.get("BUILDDATE").map(str_to_systemtime).unwrap(),
            url: intern("URL", &mut ir).unwrap(),
            license: intern_list("LICENSE", &mut ir).unwrap(),
            desc: intern("DESC", &mut ir).unwrap(),
            isize: m
                .get("SIZE")
                .or_else(|| m.get("ISIZE"))
                .map(|s| u64::from_str(s).unwrap()),
            csize: m.get("CSIZE").map(|s| u64::from_str(s).unwrap()),
            validation: m
                .get("VALIDATION")
                .map(|s| Validation::from_str(s).unwrap()),
            filename: intern("FILENAME", &mut ir),
            md5sum: m
                .get("MD5SUM")
                .map(|s| B64.decode(s).unwrap().try_into().unwrap()),
            sha256sum: m
                .get("SHA265SUM")
                .map(|s| B64.decode(s).unwrap().try_into().unwrap()),
            pgpsig: intern("PGPSIG", &mut ir),

            depends: intern_list("DEPENDS", &mut ir),
            optdepends: intern_list("OPTDEPENDS", &mut ir),
            makedepends: intern_list("MAKEDEPENDS", &mut ir),
            checkdepends: intern_list("CHECKDEPENDS", &mut ir),
            provides: intern_list("PROVIDES", &mut ir),

            groups: intern_list("GROUPS", &mut ir),
            replaces: intern_list("REPLACES", &mut ir),
            conflicts: intern_list("CONFLICTS", &mut ir),
            xdata: m.get("XDATA").map(|s| XData::from_str(s).unwrap()),
            interner: ii,
        };
        #[cfg(debug_assertions)]
        {
            let mut m = m;
            for token in [
                "BASE",
                "NAME",
                "VERSION",
                "ARCH",
                "REASON",
                "INSTALLDATE",
                "VALIDATION",
                "PACKAGER",
                "SIZE",
                "ISIZE",
                "CSIZE",
                "BUILDDATE",
                "URL",
                "LICENSE",
                "DESC",
                "FILENAME",
                "MD5SUM",
                "SHA256SUM",
                "PGPSIG",
                "PROVIDES",
                "DEPENDS",
                "OPTDEPENDS",
                "MAKEDEPENDS",
                "CHECKDEPENDS",
                "GROUPS",
                "REPLACES",
                "CONFLICTS",
                "XDATA",
            ] {
                m.remove(token);
            }
            if m.len() > 0 {
                panic!("{:#?}", m);
            }
        }
        Ok(s)
    }
}

fn entry(i: &str) -> IResult<&str, (&str, &str)> {
    let header = delimited(char('%'), alphanumeric1, pair(char('%'), newline));
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

#[cfg(test)]
//const DBPATH: &'static str = "/var/lib/pacman/local/";
const DBPATH: &'static str = "/home/work/x/";
#[test]
fn test_local() {
    use std::io::Read;
    let mut s = String::new();
    let i = Interner::default();
    for dir in std::fs::read_dir(DBPATH).unwrap() {
        let dir = dir.unwrap();
        if !dir.metadata().unwrap().is_dir() {
            continue;
        }
        let desc = dir.path().join("desc");

        let mut f = std::fs::File::open(desc).unwrap();
        f.read_to_string(&mut s).unwrap();
        let _pkg = Package::from_str(i.clone(), &s).unwrap();
    }
    i.borrow_mut().shrink_to_fit();
    println!("l: {}", i.borrow().len());
    //~2.4 sec
}

#[test]
fn test_monoio_local() {
    let lim = rlimit::increase_nofile_limit(u64::MAX).unwrap() - 100;
    let sem = local_sync::semaphore::Semaphore::new(lim as usize);
    let sem = Rc::new(sem);

    let i = Interner::default();
    monoio::start::<monoio::IoUringDriver, _>(async {
        let mut futs = Vec::new();
        for dir in std::fs::read_dir(DBPATH).unwrap() {
            let dir = dir.unwrap();
            if !dir.metadata().unwrap().is_dir() {
                continue;
            }
            let desc = dir.path().join("desc");

            let i = i.clone();
            let sem = sem.clone();
            futs.push(monoio::spawn(async move {
                let lock = sem.acquire().await.unwrap();
                let s = monoio::fs::read(desc).await.unwrap();
                let s = String::from_utf8(s).unwrap();
                let _pkg = Package::from_str(i.clone(), &s).unwrap();
                std::mem::drop(lock);
            }));
        }
        for f in futs {
            f.await;
        }
    });
    i.borrow_mut().shrink_to_fit();
    println!("l: {}", i.borrow().len());
}
