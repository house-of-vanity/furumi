use std::path::PathBuf;
#[macro_use]
extern crate log;
use env_logger::Env;
use std::{process,path::Path};

mod config;
mod filesystem;
mod http;


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

    let memfs = filesystem::MemFS::new(&cfg);
    memfs.fetch_remote(PathBuf::from("/"), 1).await;
    polyfuse_tokio::mount(memfs, mountpoint, &[]).await?;

    Ok(())
}
/*
Mkdir { parent: 1, name: "123", mode: 493, umask: 18 }
Mknod { parent: 1, name: "123233", mode: 33188, rdev: 0, umask: 18 }
*/