fn main() {
    let scope = vec!["/Applications".to_string()];
    let apps = ralfred::search::apps_in(&scope);
    println!("found {} apps", apps.len());
    for (n,p) in apps.iter().take(5){ println!("{n} -> {p}"); }
}
