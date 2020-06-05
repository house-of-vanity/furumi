use std::path::PathBuf;
#[macro_use]
extern crate log;
use env_logger::Env;
use std::process;

mod config;
mod filesystem;
mod client;
use itertools::Itertools;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    let cfg = config::read();

    let mountpoint: PathBuf = PathBuf::from(&cfg.mountpoint);
    if !mountpoint.is_dir() {
        error!("The mountpoint must be a directory");
        process::exit(0x0004);
    }
    let options = [
        "ro",
        "fsname=furumi-http",
        "auto_unmount",
        "allow_other",
    ].iter().join(",");

    let memfs = filesystem::MemFS::new(&cfg);
    match memfs.fetch_remote(PathBuf::from("/"), 1).await {
        Err(e) => {
            error!("Connection failed. Check server address and credentials {}", e);
            process::exit(0x0005);
        }
        _ => {}
    }

    polyfuse_tokio::mount(memfs, mountpoint, &[
        "-o".as_ref(),
        options.as_ref(),
    ],).await?;

    Ok(())
}
