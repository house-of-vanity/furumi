extern crate base64;
use reqwest::{blocking::Client, header::CONTENT_LENGTH};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    ffi::OsStr,
    fmt,
    path::PathBuf,
    process,
    thread::sleep,
    time::{Duration, SystemTime},
};

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct RemoteEntry {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub mtime: Option<String>,
    pub size: Option<u64>,
}

impl RemoteEntry {
    pub fn parse_rfc2822(&self) -> SystemTime {
        let rfc2822 = DateTime::parse_from_rfc2822(&self.mtime.as_ref().unwrap()).unwrap();
        SystemTime::from(rfc2822)
    }
}

use chrono::format::ParseError;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};

pub async fn list_directory(
    server: &std::string::String,
    username: &Option<String>,
    password: &Option<String>,
    path: PathBuf,
) -> Result<Vec<RemoteEntry>, reqwest::Error> {
    info!("Fetching path '{}/{}'", server, path.display());
    let client = reqwest::Client::new();
    let http_auth = match username {
        Some(username) => {
            // info!("Using Basic Auth");
            let mut _buf = String::new();
            _buf.push_str(format!("{}:{}", username, password.as_ref().unwrap()).as_str());

            base64::encode(_buf)
        }
        None => String::new(),
    };
    //info!("AUTH: {:?}", http_auth);
    let resp = client
        .get(format!("{}/{}", server, path.display()).as_str())
        .header("Authorization", format!("Basic {}", http_auth))
        .send()
        .await?
        .json::<Vec<RemoteEntry>>()
        .await?;
    info!("Found {} entries into '{}'", resp.len(), path.display());
    Ok(resp)
}
