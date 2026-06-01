pub fn version_description(name: &str, desc: &str) -> String {
    format!(
        "{} {} - {}\nCopyright 2026 lumiknit<aasr4r4@gmail.com>",
        name,
        env!("CARGO_PKG_VERSION"),
        desc
    )
}
