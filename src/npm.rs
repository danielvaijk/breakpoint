use crate::pkg::Pkg;
use crate::tar::{Tarball, TarballError};
use hmac_sha512::Hash;
use std::path::PathBuf;
use std::{fs, io};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NpmError {
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Tarball error: {0}")]
    Tarball(#[from] TarballError),
    #[error("Validation error: {0}")]
    Validation(String),
}

pub fn fetch_latest_pkg_of(pkg: &Pkg) -> Result<Pkg, NpmError> {
    let request_url = &pkg.registry_url.join(&pkg.name)?;
    let response = reqwest::blocking::get(request_url.to_string())?;

    if !response.status().is_success() {
        return Err(NpmError::Validation(format!(
            "Failed to fetch package information from registry: {}.",
            response.status()
        )));
    }

    let response_body = response.text()?;
    let response_body = json::parse(&response_body)?;

    let dist_tags = &response_body["dist-tags"];

    if !dist_tags.is_object() {
        return Err(NpmError::Validation(
            "Registry package missing dist-tags information.".to_string(),
        ));
    }

    let latest_version = &dist_tags["latest"];

    if !latest_version.is_string() {
        return Err(NpmError::Validation(
            "Unexpected latest dist-tag value for latest package".to_string(),
        ));
    }

    let dist = &response_body["versions"][latest_version.to_string()]["dist"];
    let tarball_url = &dist["tarball"];
    let tarball_checksum = &dist["integrity"];

    if !tarball_url.is_string() {
        return Err(NpmError::Validation(
            "Couldn't find tarball URL for latest package".to_string(),
        ));
    }

    if !tarball_checksum.is_string() {
        return Err(NpmError::Validation(
            "Couldn't find tarball checksum for latest package".to_string(),
        ));
    }

    let version = latest_version.to_string();
    let tarball = Some(Tarball::new(
        &tarball_url.to_string(),
        &tarball_checksum.to_string(),
    )?);

    Ok(Pkg {
        version,
        tarball,
        ..pkg.clone()
    })
}

pub fn download_pkg_if_needed(pkg: &Pkg, output_dir: &PathBuf) -> Result<(), NpmError> {
    let output_dir = output_dir.join(".tmp");

    let tarball = match &pkg.tarball {
        Some(tarball) => tarball,
        None => {
            return Err(NpmError::Validation(
                "Cannot download a Pkg without tarball information.".to_string(),
            ))
        }
    };

    let tarball_path = output_dir.join(format!("{}-{}.tar.gz", pkg.name, pkg.version));

    if tarball_path.is_file() {
        let tarball_data = fs::read(&tarball_path)?;

        if is_tarball_integrity_ok(&tarball_data, &tarball.checksum) {
            println!("Valid tarball exists on file system. Will use existing...");

            return Ok(());
        }

        println!("Found existing tarball but integrity check failed. Will remove existing...");
        fs::remove_file(&tarball_path)?;
    }

    println!("Downloading tarball from registry...");

    let response = reqwest::blocking::get(tarball.url.as_str())?.error_for_status();

    if let Err(error) = response {
        return Err(NpmError::Request(error));
    }

    let tarball_data = response.unwrap().bytes()?;

    if !is_tarball_integrity_ok(&tarball_data.to_vec(), &tarball.checksum) {
        return Err(NpmError::Validation(
            "Could not verify integrity of downloaded tarball.".into(),
        ));
    }

    println!("Integrity OK, storing on the file system...");

    if !output_dir.is_dir() {
        fs::create_dir(output_dir)?;
    }

    fs::write(tarball_path, tarball_data)?;

    Ok(())
}

fn is_tarball_integrity_ok(buffer: &Vec<u8>, checksum: &Vec<u8>) -> bool {
    let mut hash = Hash::new();

    hash.update(&buffer);
    hash.finalize().eq(checksum.as_slice())
}