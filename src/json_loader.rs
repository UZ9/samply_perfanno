use anyhow::Result;
use std::{io::Read, path::PathBuf};

use crate::types::Profile;

pub fn load_firefox_profile(gzip_profile_path: &PathBuf) -> Result<Profile> {
    let mut json = Vec::new();

    let bytes = std::fs::read(gzip_profile_path)?;

    flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut json)?;

    Ok(serde_json::from_slice(&json)?)
}
