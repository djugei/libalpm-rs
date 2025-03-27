use libalpm_rs;

fn main() {
    let i = libalpm_rs::db::new_interner();
    let (_, dbs) = libalpm_rs::config::extract_relevant_config();
    let dbs = dbs
        .keys()
        .map(|k| libalpm_rs::db::parse_syncdb(i.clone(), k).unwrap())
        .reduce(|mut acc, e| {
            acc.extend(e);
            acc
        })
        .unwrap();
    let mut dbs: Vec<_> = dbs.into_values().collect();

    dbs.sort_unstable_by_key(|v| v.isize);

    let ii = i.borrow();
    println!("isize");
    dbs.iter().rev().take(10).for_each(|p| {
        if let Some(isize) = p.isize {
            println!(
                "{}: {} {}",
                bytesize::ByteSize::b(isize),
                ii.resolve(p.name).unwrap(),
                isize
            )
        }
    });

    println!("csize");
    dbs.sort_unstable_by_key(|v| v.csize);
    dbs.iter().rev().take(10).for_each(|p| {
        if let Some(csize) = p.csize {
            println!(
                "{}: {}",
                bytesize::ByteSize::b(csize),
                ii.resolve(p.name).unwrap()
            )
        }
    });
}
