use std::path::PathBuf;
#[macro_use]
extern crate log;
use env_logger::Env;
use std::process;
use std::ffi::OsStr;

mod config;
mod filesystem;
mod http;
use itertools::Itertools;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    let cfg = config::read();
    //let http::list_directory(&cfg.server, &cfg.username, &cfg.password, "/").await;
    warn!("{:?}", cfg);

    //let mut args = pico_args::Arguments::from_env();

    let mountpoint: PathBuf = PathBuf::from(&cfg.mountpoint);
    if !mountpoint.is_dir() {
        error!("The mountpoint must be a directory");
        process::exit(0x0004);
    }
    let options = [
        "ro",
        "fsname=furumi-http",
        // "sync_read",
        "auto_unmount",
        "allow_other",
    ].iter().join(",");

    let memfs = filesystem::MemFS::new(&cfg);
    memfs.fetch_remote(PathBuf::from("/"), 1).await;
    polyfuse_tokio::mount(memfs, mountpoint, &[
        "-o".as_ref(),
        options.as_ref(),
    ],).await?;

    Ok(())
}
