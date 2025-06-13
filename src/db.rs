use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Read;
use std::ops::Deref;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base64::prelude::BASE64_STANDARD_NO_PAD as B64;
use base64::Engine;
use nom::bytes::complete::tag;
use nom::bytes::complete::take_until;
use nom::bytes::complete::take_while;
use nom::bytes::complete::take_while1;
use nom::character::complete::satisfy;
use nom::character::complete::{alphanumeric1, char, newline};
use nom::combinator::opt;
use nom::error::Error;
use nom::multi::many1;
use nom::multi::separated_list0;
use nom::sequence::terminated;
use nom::sequence::{delimited, pair};
use nom::AsChar;
use nom::Finish;
use nom::{IResult, Parser};
use string_interner::DefaultStringInterner;
use string_interner::DefaultSymbol as Istr;
use string_interner::StringInterner;

type InnerInterner = DefaultStringInterner;
type Interner = Rc<RefCell<InnerInterner>>;
pub fn new_interner() -> Interner {
    let i = StringInterner::<_>::new();
    Rc::new(RefCell::new(i))
}

#[derive(Clone)]
pub enum Validation {
    None = 1,
    Md5Sum = 1 << 1,
    Sha256Sum = 1 << 2,
    Signature = 1 << 3,
}

impl FromStr for Validation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "md5" => Ok(Self::Md5Sum),
            "sha256" => Ok(Self::Sha256Sum),
            "pgp" => Ok(Self::Signature),
            s => Err(format!("Unsupported validation: {}", s)),
        }
    }
}

#[derive(Copy, Clone)]
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

impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Any => "any",
        }
    }
}

#[derive(Clone)]
// TODO: Possibly just keep this as a string/don't keep it at all
// its unclear to me what even uses this data.
pub enum XData {
    Pkg,
    Split,
    Debug,
}

impl FromStr for XData {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pkgtype=pkg" => Ok(Self::Pkg),
            "pkgtype=split" => Ok(Self::Split),
            "pkgtype=debug" => Ok(Self::Debug),
            s => Err(format!("unknown package type {}", s)),
        }
    }
}

#[derive(Clone)]
pub struct Package {
    pub i: Interner,
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
    pub pgpsig: Option<Istr>,

    pub provides: Option<Vec<Istr>>,
    pub depends: Option<Vec<Istr>>,
    pub optdepends: Option<Vec<Istr>>,
    pub makedepends: Option<Vec<Istr>>,
    pub checkdepends: Option<Vec<Istr>>,
    pub groups: Option<Vec<Istr>>,
    pub replaces: Option<HashSet<Istr>>,
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
            replaces: intern_list("REPLACES", &mut ir).map(|l| l.into_iter().collect()),
            conflicts: intern_list("CONFLICTS", &mut ir),
            xdata: m.get("XDATA").map(|s| XData::from_str(s).unwrap()),
            i: ii,
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
    let v = std::fs::read(format!("{LOCAL_DBPATH}/ALPM_DB_VERSION"))?;
    let e = "invalid version";
    let v = String::from_utf8(v).expect(e);
    let v: u64 = v.trim().parse().expect(e);
    assert_eq!(v, 9, "{e}");

    let mut s = String::with_capacity(32_000);
    let mut pkgs = HashMap::new();
    for dir in std::fs::read_dir(LOCAL_DBPATH).unwrap() {
        let dir = dir.unwrap();
        if !dir.metadata().unwrap().is_dir() {
            continue;
        }

        let desc = dir.path().join("desc");
        let mut desc = std::fs::File::open(desc)?;
        s.clear();
        desc.read_to_string(&mut s)?;

        let pkg = Package::from_str(i.clone(), &s).unwrap();
        pkgs.insert(pkg.name, pkg);
    }
    Ok(pkgs)
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

/// only gets upgrades, no new dependencies
pub fn update_candidates<'db>(
    i: &Interner,
    dbs: &'db [&str],
    ignore: &[Istr],
) -> Vec<(&'db str, Package, Package)> {
    let local = parse_localdb(i.clone()).unwrap();

    let syncs: Vec<_> = dbs
        .into_iter()
        .map(|name| (name, parse_syncdb(i.clone(), name).unwrap()))
        .collect();
    i.borrow_mut().shrink_to_fit();
    let i = i.borrow();
    let mut upgrades = Vec::new();
    for (name, package) in local.iter().filter(|(s, _)| !ignore.contains(s)) {
        let package_version = package.version.r(&i);
        let package_version = versionparse(package_version).finish().unwrap();
        for (dbname, db) in &syncs {
            for (sync_name, sync_package) in db {
                let is_upgrade = if *sync_name == *name {
                    let sync_package_version = sync_package.version.r(&i);
                    let sync_package_version = versionparse(sync_package_version).finish().unwrap();
                    match package_version.cmp(&sync_package_version) {
                        std::cmp::Ordering::Less => true,
                        std::cmp::Ordering::Equal => false,
                        std::cmp::Ordering::Greater => {
                            use log;
                            log::warn!(
                                "downgrade? {:?}: {:?} to {:?}",
                                name,
                                package_version,
                                sync_package_version
                            );
                            false
                        }
                    }
                } else if let Some(r) = &sync_package.replaces {
                    r.contains(name)
                } else {
                    false
                };

                if is_upgrade {
                    upgrades.push((**dbname, package.clone(), sync_package.clone()));
                }
            }
        }
    }
    upgrades
}

pub trait QuickResolve {
    fn r<'i, I: Deref<Target = InnerInterner>>(self, i: &'i I) -> &'i str;
}

impl QuickResolve for Istr {
    fn r<'i, I: Deref<Target = InnerInterner>>(self, i: &'i I) -> &'i str {
        i.deref().resolve(self).unwrap()
    }
}

type Version<'v> = (
    Option<u64>,
    Vec<Result<&'v str, u64>>,
    Option<Vec<Result<&'v str, u64>>>,
);

//TODO: do not allocate, this is pretty wasteful overall!
#[inline(always)]
pub fn versionparse(i: &str) -> IResult<&str, Version, ()> {
    let epoch = (take_while(|c: char| c.is_numeric()), char(':'))
        .map(|i| i.0)
        .map_res(|s| u64::from_str(s));
    let (i, epoch) = opt(epoch).parse(i)?;

    let (pre, post) = if let Some((pre, post)) = i.rsplit_once('-') {
        (pre, Some(post))
    } else {
        (i, None)
    };

    let (v_rem, version) = version_segment_parse(pre)?;
    let release = post.map(version_segment_parse).transpose()?;
    let (r_rem, release) = release.unzip();

    Ok((r_rem.unwrap_or(v_rem), (epoch, version, release)))
}

#[inline(always)]
fn version_segment_parse(i: &str) -> IResult<&str, Vec<Result<&str, u64>>, ()> {
    many1(
        terminated(
            take_while1(|c: char| c.is_alphanum()),
            opt(satisfy(|c| !c.is_alphanumeric())),
        )
        .map(|segment| match u64::from_str(segment) {
            Ok(n) => Err(n),
            Err(_e) => Ok(segment),
        }),
    )
    .parse(i)
}

pub fn versioncmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (_res, va) = versionparse(a).unwrap();
    let (_res, vb) = versionparse(b).unwrap();

    va.cmp(&vb)
}

#[test]
fn test_version() {
    let v1 = "2025.Q1.2-1";
    let (_rem, (epoch, version, release)) = versionparse(v1).finish().unwrap();
    println!("{epoch:?} {version:?} {release:?}");
    assert!(epoch.is_none());
    assert_eq!(version.len(), 3);
    println!("{version:?}");
    assert!(release.is_some());
}

#[test]
fn test_versions() {
    let i = new_interner();
    let local = parse_localdb(i.clone()).unwrap();
    let local = ("local", local);

    let syncs =
        ["core", "extra", "multilib"].map(|name| (name, parse_syncdb(i.clone(), name).unwrap()));

    let i = i.borrow();

    let mut error = 0;

    for (_dbname, db) in std::iter::once(local).chain(syncs.into_iter()) {
        for (_pkgname, pkg) in db.iter() {
            let v = pkg.version.r(&i);
            match versionparse(&v) {
                Err(e) => {
                    println!("error parsing {v} as version: {e}");
                    error += 1;
                }
                Ok((i, (epoch, version, release))) => {
                    if !i.is_empty() {
                        println!(
                            "{i} remaining after parsing {v} as {epoch:?} {version:?} {release:?}"
                        );
                        error += 1;
                    }

                    // Try to reconstruct the version string
                    let mut s = epoch.map(|e| format!("{e}:")).unwrap_or_default();
                    let vs = version
                        .into_iter()
                        .map(|e| match e {
                            Ok(v) => v.to_owned(),
                            Err(v) => v.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(".");
                    s.push_str(&vs);
                    s.push('-');
                    if let Some(release) = release {
                        let rs = release
                            .into_iter()
                            .map(|e| match e {
                                Ok(v) => v.to_owned(),
                                Err(v) => v.to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(".");
                        s.push_str(&rs);
                    }
                    // leading zeroes are not preserved
                    s.retain(|c| c != '0');
                    let mut v = v.to_string();
                    v.retain(|c| c != '0');
                    // Separators are not preserved, so we can only match length
                    if v.len() != s.len() {
                        println!("v: {v}");
                        println!("s: {s}");
                        println!();
                        error += 1
                    }
                }
            }
        }
    }

    if error > 0 {
        panic!("{error} errors while parsing version numbers");
    }
}

#[test]
fn test_update() {
    let ts = SystemTime::now();
    let i = new_interner();
    let vers = update_candidates(&i, &["core", "extra", "multilib"], &[]);

    let i = i.borrow();
    for (dbname, from, to) in vers {
        let from_name = from.name.r(&i);
        let from_version = from.version.r(&i);
        let to_name = to.name.r(&i);
        let to_version = to.version.r(&i);

        println!("upgrading {from_name} {from_version} to {to_name} {to_version} in {dbname}");
    }

    let passed = SystemTime::now().duration_since(ts).unwrap();
    println!("update_candidates took {passed:?} seconds");
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
    println!("syncdb took {passed:?} seconds");
}

#[test]
fn test_entry() {
    use std::io::Read;
    let mut f = std::fs::File::open("/var/lib/pacman/local/base-3-2/desc").unwrap();
    let mut s = Default::default();
    f.read_to_string(&mut s).unwrap();
    let (_, (_h, _v)) = entry(&s).unwrap();
}

#[test]
fn test_list() {
    use std::io::Read;
    let mut f = std::fs::File::open("/var/lib/pacman/local/base-3-2/desc").unwrap();
    let mut s = Default::default();
    f.read_to_string(&mut s).unwrap();
    let (_r, _l) = list(&s).unwrap();
}

#[test]
fn test_local() {
    let ts = SystemTime::now();
    let i = new_interner();
    parse_localdb(i.clone()).unwrap();
    i.borrow_mut().shrink_to_fit();
    println!("local interning: {}", i.borrow().len());
    let passed = SystemTime::now().duration_since(ts).unwrap();
    println!("local took {passed:?} seconds");
}
