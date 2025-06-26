mod parse;
pub use parse::new_interner;
pub use parse::{Interner, Istr, Package, QuickResolve};
pub use parse::{versioncmp, versionparse};
use std::{collections::HashMap, io::Read};

const LOCAL_DBPATH: &str = "/var/lib/pacman/local/";
const SYNC_DBPATH: &str = "/var/lib/pacman/sync/";

/// returns name -> package
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

        let pkg = Package::from_str(i.clone(), s).expect("package parsing failed");
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
        .iter()
        .map(|name| (name, parse_syncdb(i.clone(), name).unwrap()))
        .collect();
    i.borrow_mut().shrink_to_fit();
    let i = i.borrow();
    let mut upgrades = Vec::new();
    for (name, package) in local.iter().filter(|(s, _)| !ignore.contains(s)) {
        let package_version = package.version.r(&i);
        let package_version = parse::versionparse(package_version).unwrap();
        for (dbname, db) in &syncs {
            for (sync_name, sync_package) in db {
                let is_upgrade = if *sync_name == *name {
                    let sync_package_version = sync_package.version.r(&i);
                    let sync_package_version = parse::versionparse(sync_package_version).unwrap();
                    match package_version.cmp(&sync_package_version) {
                        std::cmp::Ordering::Less => true,
                        std::cmp::Ordering::Equal => false,
                        std::cmp::Ordering::Greater => {
                            use log;
                            log::warn!(
                                "downgrade? {name:?}: {package_version:?} to {sync_package_version:?}",
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

/// auto-unlocks on drop
pub struct DBLock(#[allow(dead_code)] std::fs::File);

impl DBLock {
    pub fn new() -> Result<Self, ()> {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(false)
            .open("/var/lib/pacman/db.lck")
        {
            Ok(f) => Ok(Self(f)),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Err(())
                } else {
                    panic!("unexpected error while locking database");
                }
            }
        }
    }
}

impl Drop for DBLock {
    fn drop(&mut self) {
        std::fs::remove_file("/var/lib/pacman/db.lck").expect("error unlocking database")
    }
}

#[test]
fn test_update() {
    use std::time::SystemTime;
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
    use std::time::SystemTime;
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
fn test_local() {
    use std::time::SystemTime;
    let ts = SystemTime::now();
    let i = new_interner();
    parse_localdb(i.clone()).unwrap();
    i.borrow_mut().shrink_to_fit();
    println!("local interning: {}", i.borrow().len());
    let passed = SystemTime::now().duration_since(ts).unwrap();
    println!("local took {passed:?} seconds");
}
