use crate::pkg::contents::PkgContents;
use crate::pkg::entries::PkgEntries;
use crate::pkg::tarball::PkgTarball;
use crate::pkg::Pkg;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE_64_STANDARD, Engine as _};
use json::JsonValue;
use std::path::PathBuf;
use std::rc::Rc;
use url::Url;

pub fn load_from_dir(pkg_dir: PathBuf) -> Result<Pkg> {
    let pkg_json = Pkg::parse_config_in_dir(&pkg_dir)
        .with_context(|| "Failed to load and parse package.json config.")?;

    let pkg_registry_url = Pkg::get_registry_url(&pkg_dir)
        .with_context(|| "Failed to determine package registry URL.")?;

    println!("Will use {} as registry.", &pkg_registry_url);

    let pkg_contents = PkgContents::new(pkg_dir.to_owned(), &pkg_json, None)
        .with_context(|| "Failed to create package contents from file system.")?;

    let pkg_contents = Rc::new(pkg_contents);

    let pkg_entries = PkgEntries::new(&pkg_json, Rc::clone(&pkg_contents))
        .with_context(|| "Failed to create package entries.")?;

    Ok(Pkg::new(
        pkg_dir,
        pkg_json,
        pkg_registry_url,
        pkg_contents,
        pkg_entries,
    ))
}

pub fn fetch_from_server(local_pkg: &Pkg) -> Result<Pkg> {
    let pkg_dir = &local_pkg.dir;
    let pkg_dir_tmp = pkg_dir.join(".tmp");
    let pkg_registry_url = local_pkg.registry_url.to_owned();

    let tarball = fetch_last_published_tarball_of(&pkg_dir_tmp, local_pkg)
        .with_context(|| "Failed to fetch last published tarball from registry.")?;

    let pkg = download_and_unpack_pkg_tarball(pkg_dir.to_owned(), pkg_registry_url, tarball)
        .with_context(|| "Failed to download and unpack package tarball from registry.")?;

    Ok(pkg)
}

fn fetch_last_published_tarball_of(pkg_dir: &PathBuf, local_pkg: &Pkg) -> Result<PkgTarball> {
    let pkg_data_latest = fetch_latest_pkg_info_for(local_pkg)
        .with_context(|| "Failed to request package information from registry.")?;

    let pkg_version_latest = &pkg_data_latest["dist-tags"]["latest"];

    if !pkg_version_latest.is_string() {
        bail!("Unexpected latest dist-tag value for latest package.");
    }

    let pkg_version_latest = pkg_version_latest.to_string();
    let pkg_tarball_name = format!("{}-{}.tar.gz", local_pkg.name, pkg_version_latest);

    let pkg_dist = &pkg_data_latest["versions"][&pkg_version_latest]["dist"];
    let pkg_tarball = get_pkg_tarball_from_dist(pkg_tarball_name, pkg_dir, pkg_dist)
        .with_context(|| "Failed to extract tarball info from latest version dist response.")?;

    Ok(pkg_tarball)
}

fn download_and_unpack_pkg_tarball(
    pkg_dir: PathBuf,
    pkg_registry_url: Url,
    mut pkg_tarball: PkgTarball,
) -> Result<Pkg> {
    pkg_tarball
        .download_if_needed()
        .with_context(|| "Failed to download tarball or load from local cache.")?;

    let pkg_json = get_pkg_json_from_tarball(&mut pkg_tarball)?;

    let pkg_contents = PkgContents::new(pkg_dir.to_owned(), &pkg_json, Some(pkg_tarball))
        .with_context(|| "Failed to create package contents with tarball.")?;

    let pkg_contents = Rc::new(pkg_contents);

    let pkg_entries = PkgEntries::new(&pkg_json, Rc::clone(&pkg_contents))
        .with_context(|| "Failed to create package entries.")?;

    Ok(Pkg::new(
        pkg_dir,
        pkg_json,
        pkg_registry_url,
        pkg_contents,
        pkg_entries,
    ))
}

fn get_pkg_json_from_tarball(pkg_tarball: &mut PkgTarball) -> Result<JsonValue> {
    let path = PathBuf::from("package.json");
    let data = pkg_tarball.load_file_by_path(&path)?;
    let data = String::from_utf8(data.unwrap())?;

    Pkg::parse_config_as_json(data)
}

fn fetch_latest_pkg_info_for(pkg: &Pkg) -> Result<JsonValue> {
    let request_url = &pkg.registry_url.join(&pkg.name)?;
    let response = reqwest::blocking::get(request_url.to_string())?.error_for_status();

    if let Err(error) = response {
        bail!(error);
    }

    let response_body = response.unwrap().text()?;
    let response_body = json::parse(&response_body)?;

    Ok(response_body)
}

fn get_pkg_tarball_from_dist(name: String, dir: &PathBuf, dist: &JsonValue) -> Result<PkgTarball> {
    let tarball_url = &dist["tarball"];
    let tarball_checksum = &dist["integrity"];

    if !tarball_url.is_string() {
        bail!("Couldn't find tarball URL for latest package.");
    } else if !tarball_checksum.is_string() {
        bail!("Couldn't find tarball checksum for latest package.");
    }

    let tarball_checksum = tarball_checksum.to_string();
    let tarball_integrity_parts: Vec<&str> = tarball_checksum.split('-').collect();

    if tarball_integrity_parts.len().ne(&2) {
        bail!("Unexpected integrity string format for latest package.");
    }

    let tarball_hash_algorithm = tarball_integrity_parts.first().unwrap();
    let tarball_hash_integrity = tarball_integrity_parts.last().unwrap();

    if tarball_hash_algorithm.ne(&"sha512") {
        bail!("Package integrity can only be verified with SHA-512.");
    }

    let tarball_url = Url::parse(tarball_url.as_str().unwrap())?;
    let tarball_checksum = BASE_64_STANDARD.decode(tarball_hash_integrity)?;

    PkgTarball::new(name, dir.to_owned(), tarball_url, tarball_checksum)
}
