pub mod config;
pub mod db;
pub mod util;

/// Calculates which packages need upgrades,
/// limited to the databases passed in with db_filter.
/// Currently just panics when anything goes wrong.
/// Ex: ```upgrade_urls(&["core", "extra", "multilib"])```
///
/// (upgrade_url, (old_name, old_version, old_arch), (new_name, new_version, new_filename))
pub fn upgrade_urls(db_filter: &[&str]) -> Vec<(String, db::Package, db::Package)> {
    use db::QuickResolve;
    let (ignore, repos) = config::extract_relevant_config();
    let repo_names: Vec<&str> = repos
        .keys()
        .map(String::as_str)
        .filter(|r| db_filter.contains(r))
        .collect();
    let i = db::new_interner();
    let ignore: Vec<_> = ignore
        .into_iter()
        .map(|s| i.borrow_mut().get_or_intern(s.trim()))
        .collect();
    let ups = db::update_candidates(&i, &repo_names, &ignore);
    let i = i.borrow();
    let mut ret = Vec::new();
    for (dbname, from, to) in ups.into_iter() {
        let filename = to.filename.unwrap().r(&i);
        let cache_file = format!("/var/cache/pacman/pkg/{filename}");
        let url = if std::fs::exists(&cache_file).unwrap() {
            format!("file://{cache_file}")
        } else {
            format!("{}/{filename}", repos[dbname])
        };
        ret.push((url, from, to));
    }
    ret
}

#[test]
fn test_upgrade_urls() {
    let ts = std::time::SystemTime::now();
    for (u, _, _) in upgrade_urls(&["core", "extra", "multilib"]) {
        println!("{}", u);
    }
    let passed = std::time::SystemTime::now().duration_since(ts).unwrap();
    println!("finding upgrades took {passed:?}")
}
