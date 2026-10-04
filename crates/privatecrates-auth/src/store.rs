//! Where a developer's tokens are kept: the operating system's keyring, or, where there is none, a file readable
//! only by the user (as the GitHub CLI does).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::Error;

const SERVICE: &str = "privatecrates";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stored {
    pub access_token: String,
    pub expires_at: i64,
    pub refresh_token: Option<String>,
    pub refresh_expires_at: Option<i64>,
}

pub enum Store {
    Keyring,
    File(PathBuf),
}

impl Store {
    /// The keyring when available. `PRIVATECRATES_CREDENTIAL_STORE=file` forces the file store.
    pub fn open() -> Result<Self, Error> {
        let file = || Ok(Self::File(config_dir()?.join("credentials.json")));
        match std::env::var("PRIVATECRATES_CREDENTIAL_STORE").as_deref() {
            Ok("file") => file(),
            Ok("keyring") => Ok(Self::Keyring),
            Ok(other) => Err(format!(
                "PRIVATECRATES_CREDENTIAL_STORE must be `keyring` or `file`, not `{other}`"
            )
            .into()),
            Err(_) if keyring::Entry::store_status().is_ok() => Ok(Self::Keyring),
            Err(_) => {
                eprintln!(
                    "note: no system keyring is available, so the PrivateCrates token is stored in a file readable \
                     only by you"
                );
                file()
            }
        }
    }

    pub fn load(&self, base: &str) -> Result<Option<Stored>, Error> {
        let text = match self {
            Self::Keyring => match entry(base)?.get_password() {
                Ok(text) => text,
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(e) => return Err(keyring_error(e)),
            },
            Self::File(path) => match read_file(path)?.remove(base) {
                Some(stored) => return Ok(Some(stored)),
                None => return Ok(None),
            },
        };
        // An unreadable entry is treated as absent: the user signs in again.
        Ok(serde_json::from_str(&text).ok())
    }

    pub fn save(&self, base: &str, stored: &Stored) -> Result<(), Error> {
        match self {
            Self::Keyring => entry(base)?
                .set_password(&serde_json::to_string(stored).map_err(json_error)?)
                .map_err(keyring_error),
            Self::File(path) => {
                let mut all = read_file(path)?;
                all.insert(base.to_owned(), stored.clone());
                write_file(path, &all)
            }
        }
    }

    pub fn delete(&self, base: &str) -> Result<(), Error> {
        match self {
            Self::Keyring => match entry(base)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(keyring_error(e)),
            },
            Self::File(path) => {
                let mut all = read_file(path)?;
                if all.remove(base).is_some() {
                    write_file(path, &all)?;
                }
                Ok(())
            }
        }
    }
}

fn entry(base: &str) -> Result<keyring::Entry, Error> {
    keyring::Entry::new(SERVICE, base).map_err(keyring_error)
}

fn keyring_error(e: keyring::Error) -> Error {
    format!(
        "the system keyring failed: {e}. Set PRIVATECRATES_CREDENTIAL_STORE=file to store the token in a file instead"
    )
    .into()
}

fn json_error(e: serde_json::Error) -> Error {
    format!("cannot serialise the PrivateCrates token: {e}").into()
}

/// Where PrivateCrates keeps its files: `PRIVATECRATES_CONFIG_DIR`, or the platform's configuration directory.
pub fn config_dir() -> Result<PathBuf, Error> {
    if let Some(dir) = std::env::var_os("PRIVATECRATES_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir).join("privatecrates"));
    }
    if let Some(dir) = std::env::var_os("APPDATA") {
        return Ok(PathBuf::from(dir).join("privatecrates"));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/privatecrates"))
        .ok_or_else(|| "cannot find a configuration directory; set PRIVATECRATES_CONFIG_DIR".into())
}

fn read_file(path: &Path) -> Result<BTreeMap<String, Stored>, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display()).into()),
    }
}

fn write_file(path: &Path, all: &BTreeMap<String, Stored>) -> Result<(), Error> {
    let fail = |e: std::io::Error| Error::from(format!("cannot write {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(fail)?;
    }
    let text = serde_json::to_string_pretty(all).map_err(json_error)?;
    // Write to a new file readable only by the user, then rename over the old one.
    let tmp = path.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp).map_err(fail)?;
    // The mode above applies only to a new file: tighten one left behind too.
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(fail)?;
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(fail)?;
    fs::rename(&tmp, path).map_err(fail)
}
