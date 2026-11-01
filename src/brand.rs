use std::path::PathBuf;

pub const NAME: &str = "Genesis Launcher";
pub const COMPACT_NAME: &str = "GenesisLauncher";
pub const DATA_DIRECTORY: &str = "GenesisLauncher";
pub const LEGACY_DATA_DIRECTORY: &str = "BlockLauncher";
pub const USER_AGENT: &str = concat!(
    "GenesisLauncher/",
    env!("CARGO_PKG_VERSION"),
    " (+Rust native launcher)"
);

pub fn data_directory() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(DATA_DIRECTORY)
}

pub fn legacy_data_directory() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(LEGACY_DATA_DIRECTORY)
}
