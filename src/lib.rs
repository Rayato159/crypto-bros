pub mod api;
pub mod chain;
pub mod indicator_strategy;
pub mod live;
pub mod live_api;
pub mod market;
pub mod solana_wire;
pub mod state;
pub mod storage;
pub mod watch;

pub fn data_directory() -> Result<std::path::PathBuf, String> {
    directories::ProjectDirs::from("dev", "CryptoBros", "CryptoBros")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .ok_or_else(|| "Cannot locate application data directory".into())
}

pub mod readout;
