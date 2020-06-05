extern crate base64;

use chrono::DateTime;
use reqwest::{header, Client, Error};
use serde::Deserialize;
use std::{
    path::PathBuf,
    process,
    thread::sleep,
    time::{Duration, SystemTime},
};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Default, Debug, Clone)]
pub struct HTTP {
    client: Client,
    server: String,
}

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

impl HTTP {
    pub fn new(server: String, username: Option<String>, password: Option<String>) -> Self {
        let mut headers = header::HeaderMap::new();
        match username {
            Some(username) => {
                info!("HTTP credentials has been configured. Securing connection.");
                let mut _buf = String::new();
                _buf.push_str(format!("{}:{}", username, password.as_ref().unwrap()).as_str());
                let creds = base64::encode(_buf);

                headers.insert(
                    header::AUTHORIZATION,
                    header::HeaderValue::from_str(format!("Basic {}", creds).as_str()).unwrap(),
                );
            }
            None => {}
        };
        let client = reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .default_headers(headers)
            .build()
            .unwrap();
        Self { client, server }
    }
    pub async fn list(&self, path: PathBuf) -> Result<Vec<RemoteEntry>, Error> {
        debug!("Fetching path '{}/{}'", self.server, path.display());
        let mut client = &self.client;
        let resp = client
            .get(format!("{}/{}", self.server, path.display()).as_str())
            .send()
            .await?
            .json::<Vec<RemoteEntry>>()
            .await?;
        debug!("Found {} entries into '{}'", resp.len(), path.display());
        Ok(resp)
    }

    pub async fn read(&self, path: PathBuf, size: usize, offset: usize) -> Result<Vec<u8>, Error> {
        debug!("Reading path '{}/{}'", self.server, path.display());
        let mut headers = header::HeaderMap::new();
        let range = format!("bytes={}-{}", offset, { offset + size - 1 });
        info!("range = {:?}", range);
        headers.insert(
            header::RANGE,
            header::HeaderValue::from_str(range.as_str()).unwrap(),
        );

        let mut client = &self.client;
        let resp = client
            .get(format!("{}/{}", self.server, path.display()).as_str())
            .headers(headers)
            .send()
            .await?
            .bytes()
            .await?;
        info!("Found {} entries into '{}'", resp.len(), path.display());
        Ok(resp.to_vec())
    }
}
