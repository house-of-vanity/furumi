use clap::{App, Arg};
use std::process;

extern crate chrono;
extern crate config;

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Config {
    pub server: String,
    pub mountpoint: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub conf_file: String,
}

pub fn read() -> Config {
    // Parse opts and args
    let cli_args = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("conf")
                .short("c")
                .long("conf")
                .help("Config file to use")
                .default_value("/etc/furumi.yml")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    info!("Logger initialized. Set RUST_LOG=[debug,error,info,warn,trace] Default: info");
    info!(
        "Starting {} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    // Read config file and env vars
    let config_file = cli_args.value_of("conf").unwrap();
    let mut settings = config::Config::default();
    settings = match settings.merge(config::File::with_name(config_file)) {
        Ok(conf_content) => {
            info!("Using config file {}", config_file);
            conf_content.to_owned()
        }
        Err(e) => {
            error!("Can't read config file - {}", e);
            process::exit(0x0001);
        }
    };
    let server = match settings.get_str("server") {
        Ok(server) => server,
        Err(_) => {
            error!("Server is not set in config. Set `server` directive.");
            process::exit(0x0002);
        }
    };
    let mountpoint = match settings.get_str("mountpoint") {
        Ok(mountpoint) => mountpoint,
        Err(_) => {
            error!("Mountpoint is not set in config. Set `mountpoint` directive.");
            process::exit(0x0003);
        }
    };

    let username = match settings.get_str("username") {
        Ok(username) => Some(username),
        Err(_) => None,
    };
    let password = match settings.get_str("password") {
        Ok(password) => Some(password),
        Err(_) => None,
    };
    if password == None || username == None {
        warn!("Insecure server detected. Set `username` and `password` directives to use auth.");
    }
    Config {
        server,
        username,
        password,
        mountpoint,
        conf_file: config_file.to_string(),
    }
}
