#[test]
fn test_basic() {
    use alpm::vercmp;
    use libalpm_rs::db::versioncmp;
    use std::cmp::Ordering::*;
    assert_eq!(vercmp("1.4", "1.1b"), Greater);
    assert_eq!(versioncmp("1.4", "1.1b"), Greater);

    assert_eq!(vercmp("1.4", "1.1b.1"), Greater);
    assert_eq!(versioncmp("1.4", "1.1b.1"), Greater);

    assert_eq!(vercmp("0.15.1-2", "0.15.1b-10"), Less);
    assert_eq!(versioncmp("0.15.1-2", "0.15.1b-10"), Less);
}

#[test]
fn test_vercmp() {
    use alpm;
    use libalpm_rs;
    use libalpm_rs::db::QuickResolve;

    let i = libalpm_rs::db::new_interner();
    let l = libalpm_rs::db::parse_localdb(i.clone()).unwrap();
    let ii = i.borrow();

    let l: Vec<_> = l
        .into_iter()
        .map(|(k, p)| {
            let v = p.version.r(&ii);
            (k.r(&ii), v, libalpm_rs::db::versionparse(v).unwrap().1)
        })
        .collect();

    let mut failed = 0;

    for (name1, v1, vp1) in &l {
        for (name2, v2, vp2) in &l {
            let rs_ord = vp1.cmp(&vp2);
            let c_ord = alpm::vercmp(*v1, *v2);
            if rs_ord != c_ord {
                failed += 1;
                eprintln!("r: {rs_ord:?} but c: {c_ord:?}");
                eprintln!("{name1} {name2}");
                eprintln!("{v1} {v2}");
                eprintln!("{vp1:#?}");
                eprintln!("{vp2:#?}");
            }
        }
    }

    if failed > 0 {
        panic!("{failed} tests failed")
    }
}
