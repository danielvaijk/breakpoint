use crate::fs::file::FileExt;
use crate::fs::path::path_matches_a_pattern_in;
use crate::pkg::contents::PkgContents;
use anyhow::{bail, Context, Result};
use json::JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub struct PkgEntry {
    pub name: String,
    pub path: PathBuf,
    pub ext: FileExt,
    contents: Rc<PkgContents>,
}

pub struct PkgEntries {
    pub main: HashMap<String, PkgEntry>,
    pub bin: HashMap<String, PkgEntry>,
    pub browser: HashMap<String, PkgEntry>,
    pub exports: HashMap<String, PkgEntry>,
}

impl PkgEntry {
    pub fn new(name: String, path: PathBuf, contents: Rc<PkgContents>) -> Result<Self> {
        let pkg_file_path = contents.pkg_dir.join(&path);

        let ext = FileExt::from(&path)
            .with_context(|| format!("Failed to get file extension for entry '{name}'."))?;

        if let FileExt::OTHER(other) = ext {
            bail!("Invalid entry file extension '{other}'.");
        }

        let entry = Self {
            contents: Rc::clone(&contents),
            path: path.to_owned(),
            name,
            ext,
        };

        if contents.is_tarball() {
            return Ok(entry);
        }

        if !pkg_file_path.try_exists()? {
            bail!("Entry '{}' does not exist.", path.display());
        } else if !path_matches_a_pattern_in(&pkg_file_path, &contents.include_patterns) {
            bail!(
                "Entry '{}' exists but is not included in 'files'.",
                path.display()
            );
        }

        Ok(entry)
    }

    pub fn load_file(&self) -> Result<Option<Vec<u8>>> {
        self.contents.load_file(&self.path)
    }
}

impl PkgEntries {
    pub fn new(pkg_json: &JsonValue, pkg_contents: Rc<PkgContents>) -> Result<PkgEntries> {
        let main = Self::resolve_main_entry(pkg_json, Rc::clone(&pkg_contents))?;
        let browser = Self::resolve_browser_entries(pkg_json, Rc::clone(&pkg_contents))?;
        let bin = Self::resolve_bin_entries(pkg_json, Rc::clone(&pkg_contents))?;
        let exports = Self::resolve_exports_entries(pkg_json, Rc::clone(&pkg_contents))?;

        Ok(Self {
            main,
            bin,
            browser,
            exports,
        })
    }

    fn resolve_main_entry(
        pkg_json: &JsonValue,
        pkg_contents: Rc<PkgContents>,
    ) -> Result<HashMap<String, PkgEntry>> {
        let name = String::from("main");

        let entry_path = &pkg_json[&name];
        let entry_path = if entry_path.is_string() {
            entry_path.to_string()
        } else {
            "index.js".to_string()
        };

        let entry = PkgEntry::new(name.to_owned(), entry_path.into(), Rc::clone(&pkg_contents))?;
        let entry = (name, entry);

        Ok(HashMap::from([entry]))
    }

    fn resolve_browser_entries(
        pkg_json: &JsonValue,
        pkg_contents: Rc<PkgContents>,
    ) -> Result<HashMap<String, PkgEntry>> {
        Self::resolve_string_or_object_entries("browser".into(), pkg_json, pkg_contents)
    }

    fn resolve_bin_entries(
        pkg_json: &JsonValue,
        pkg_contents: Rc<PkgContents>,
    ) -> Result<HashMap<String, PkgEntry>> {
        Self::resolve_string_or_object_entries("bin".into(), pkg_json, pkg_contents)
    }

    fn resolve_exports_entries(
        pkg_json: &JsonValue,
        pkg_contents: Rc<PkgContents>,
    ) -> Result<HashMap<String, PkgEntry>> {
        Self::resolve_string_or_object_entries("exports".into(), pkg_json, pkg_contents)
    }

    fn resolve_string_or_object_entries(
        field_name: String,
        pkg_json: &JsonValue,
        pkg_contents: Rc<PkgContents>,
    ) -> Result<HashMap<String, PkgEntry>> {
        let property = &pkg_json[&field_name];
        let mut entries: HashMap<String, PkgEntry> = HashMap::new();

        if property.is_string() {
            entries.insert(
                field_name.to_owned(),
                PkgEntry::new(
                    field_name,
                    property.to_string().into(),
                    Rc::clone(&pkg_contents),
                )?,
            );

            return Ok(entries);
        } else if !property.is_object() {
            return Ok(entries);
        }

        for (entry_name, entry_value) in property.entries() {
            // Ignore exclusion entries — if property is browser.
            if entry_value.is_boolean() {
                continue;
            }

            if entry_value.is_string() {
                let entry = PkgEntry::new(
                    entry_name.into(),
                    entry_value.to_string().into(),
                    Rc::clone(&pkg_contents),
                )?;

                entries.insert(entry_name.into(), entry);
                continue;
            }

            if !entry_value.is_object() {
                bail!("Expected '{field_name}' field to be an object.");
            }

            for (sub_entry_name, sub_entry_path) in entry_value.entries() {
                if !sub_entry_path.is_string() {
                    bail!("Expected '{sub_entry_name}' in '{field_name}' field to be a string.");
                }

                let entry = PkgEntry::new(
                    sub_entry_name.into(),
                    sub_entry_path.to_string().into(),
                    Rc::clone(&pkg_contents),
                )?;

                entries.insert(sub_entry_name.into(), entry);
            }
        }

        Ok(entries)
    }
}
