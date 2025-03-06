use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
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

type Interner = Rc<RefCell<DefaultStringInterner>>;
pub fn new_interner() -> Interner {
    let i = StringInterner::<_>::new();
    Rc::new(RefCell::new(i))
}

pub enum Validation {
    None = 1,
    Md5Sum = 1 << 1,
    Sha256Sum = 1 << 2,
    Signature = 1 << 3,
}

impl FromStr for Validation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "pgp" => Ok(Self::Signature),
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

    // explicit = 0, depend = 1, unknown = 2
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
    //TODO: custom error type, no more unwraps
    pub fn from_str(i: Interner, s: &str) -> Result<Self, ()> {
        use std::cell::RefMut;
        let m = parse_to_map(s).unwrap();
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
            assert!(m.is_empty(), "{m:#?}");
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

pub fn parse_to_map(i: &str) -> Result<HashMap<&str, &str>, nom::Err<Error<&str>>> {
    let (r, h) = list(i).map(|(r, v)| (r, v.into_iter().collect()))?;
    assert_eq!(r, "\n\n");
    Ok(h)
}

const LOCAL_DBPATH: &'static str = "/var/lib/pacman/local/";
const SYNC_DBPATH: &'static str = "/var/lib/pacman/sync/";

pub fn parse_localdb(i: Interner) -> std::io::Result<HashMap<Istr, Package>> {
    let lim = rlimit::increase_nofile_limit(u64::MAX).unwrap() - 100;
    let sem = local_sync::semaphore::Semaphore::new(lim as usize);
    let sem = Rc::new(sem);

    let pkgs = monoio::start::<monoio::IoUringDriver, _>(async {
        let v = monoio::fs::read(format!("{LOCAL_DBPATH}/ALPM_DB_VERSION")).await?;
        let e = "invalid version";
        let v = String::from_utf8(v).expect(e);
        let v: u64 = v.trim().parse().expect(e);
        assert_eq!(v, 9, "{e}");

        let mut futs = Vec::new();
        for dir in std::fs::read_dir(LOCAL_DBPATH).unwrap() {
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
                let pkg = Package::from_str(i.clone(), &s).unwrap();
                std::mem::drop(lock);
                pkg
            }));
        }
        let mut pkgs = HashMap::new();
        for f in futs {
            let pkg = f.await;
            pkgs.insert(pkg.name, pkg);
        }
        Ok(pkgs)
    });
    i.borrow_mut().shrink_to_fit();
    pkgs
    // ~170ms
}

pub fn parse_syncdb(i: Interner, name: &str) -> std::io::Result<HashMap<Istr, Package>> {
    let dbfile = format!("{SYNC_DBPATH}/{name}.db");
    let dbfile = std::fs::File::open(dbfile)?;
    let mut dbfile = flate2::read::GzDecoder::new(dbfile);

    let mut archive = Vec::new();
    dbfile.read_to_end(&mut archive)?;
    let seek_archive = std::io::Cursor::new(&archive);
    let mut seek_archive = tar::Archive::new(seek_archive);

    let mut pkgs = HashMap::new();
    for entry in seek_archive.entries_with_seek()? {
        let entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        // Avoid a copy by indexing into the archive
        let start = entry.raw_file_position() as usize;
        let size = entry.size() as usize;
        let end = start + size;
        let slice = &archive[start..end];
        let s = std::str::from_utf8(slice).unwrap();

        let pkg = Package::from_str(i.clone(), &s).expect("package parsing failed");
        pkgs.insert(pkg.name, pkg);
    }

    Ok(pkgs)
}

#[test]
fn test_syncdb() {
    let ts = SystemTime::now();

    let i = new_interner();

    let _core = parse_syncdb(i.clone(), "core").unwrap();
    println!("core done");
    let _multilib = parse_syncdb(i.clone(), "multilib").unwrap();
    println!("multilib done");
    let _extra = parse_syncdb(i.clone(), "extra").unwrap();
    println!("extra done");

    let passed = SystemTime::now().duration_since(ts).unwrap();
    println!("took {passed:?} seconds");
}

#[test]
fn test_monoio_localdb() {
    let ts = SystemTime::now();
    let i = new_interner();
    assert!(parse_localdb(i.clone()).is_ok());
    i.borrow_mut().shrink_to_fit();
    println!("mono interning: {}", i.borrow().len());
    let passed = SystemTime::now().duration_since(ts).unwrap();
    println!("mono took {passed:?} seconds");
}

#[test]
fn test_entry() {
    use std::io::Read;
    let mut f = std::fs::File::open("/var/lib/pacman/local/base-3-2/desc").unwrap();
    let mut s = Default::default();
    f.read_to_string(&mut s).unwrap();
    let (_, (h, v)) = entry(&s).unwrap();
    dbg!(h, v);
}

#[test]
fn test_list() {
    use std::io::Read;
    let mut f = std::fs::File::open("/var/lib/pacman/local/base-3-2/desc").unwrap();
    let mut s = Default::default();
    f.read_to_string(&mut s).unwrap();
    let (r, l) = list(&s).unwrap();
    dbg!(l, r);
}

#[test]
fn test_local() {
    let ts = SystemTime::now();
    use std::io::Read;
    let mut s = String::new();
    let i = new_interner();
    for dir in std::fs::read_dir(LOCAL_DBPATH).unwrap() {
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
    println!("local interning: {}", i.borrow().len());
    let passed = SystemTime::now().duration_since(ts).unwrap();
    println!("local took {passed:?} seconds");
    //~2.4 sec
}
