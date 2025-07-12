#[test]
fn test_basic() {
    use alpm::vercmp;
    use libalpm_rs::db::versioncmp;
    use std::cmp::Ordering::*;
    assert_eq!(vercmp("1.4", "1.1b"), Greater);
    assert_eq!(versioncmp("1.4", "1.1b"), Greater);

    assert_eq!(vercmp("1.4", "1.1b.1"), Greater);
    assert_eq!(versioncmp("1.4", "1.1b.1"), Greater);

    assert_eq!(vec![0, 0, 0].cmp(&vec![0, 0]), Greater);

    assert_eq!(vercmp("0.15.1-2", "0.15.1b-2"), Less);
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
            (k.r(&ii), v, libalpm_rs::db::versionparse(v).unwrap())
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

#[test]
fn test_rpmtestsuite() {
    use std::cmp::Ordering;
    let mut failed = 0;
    let f = std::fs::read_to_string("rpmvercmp.at").unwrap();
    for line in f.split('\n').filter(|l| l.starts_with("RPMVERCMP")) {
        // fuck this not dealing with tilde precedence
        if line.contains('~') {
            continue;
        }
        let (_, line) = line.split_once('(').unwrap();
        let (v1, line) = line.split_once(',').unwrap();
        let (v2, line) = line.split_once(',').unwrap();
        let (res, _) = line.split_once(')').unwrap();
        let v2 = v2.trim();
        let res: i8 = res.trim().parse().unwrap();
        let res = match res {
            -1 => Ordering::Less,
            0 => Ordering::Equal,
            1 => Ordering::Greater,
            _ => panic!("invalid ordering"),
        };
        let v = libalpm_rs::db::versioncmp(v1, v2);
        if v != res {
            failed += 1;
            println!("r: {v:?}\nt: {res:?}\nv1: {v1}\nv2: {v2}\n");
        };
        assert_eq!(v, alpm::vercmp(v1, v2), "{v1} {v2} {res:?}");
    }
    if failed > 0 {
        panic!("{failed} failed test cases");
    }
}
