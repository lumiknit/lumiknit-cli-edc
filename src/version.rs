pub fn version_description(name: &str, desc: &str) -> String {
    format!("{} {} - {}", name, env!("CARGO_PKG_VERSION"), desc)
}
