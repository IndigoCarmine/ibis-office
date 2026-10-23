// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Handler manifests: one TOML file per external editor, in the settings
//! directory's `embed-handlers/`.
//!
//! ```toml
//! id = "jp.ibis.chem"
//! name.ja = "化学構造式"
//! name.en = "Chemical Structure"
//! command = "/path/to/ibis-chem"
//! prog_ids = ["ChemDraw.Document.6.0"]
//! new_object = true
//! import_extensions = ["cdxml", "cdx", "mol"]
//! export_extensions = ["cdxml"]
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerManifest {
    /// Reverse-DNS identifier, unique among handlers.
    pub id: String,
    pub name: LocalizedName,
    /// The editor program. A bare name is looked up on `PATH`; a relative
    /// path with a directory part ("bin/editor") is resolved against the
    /// manifest's directory by [`HandlerManifest::load`].
    pub command: PathBuf,
    /// Extra arguments put before `--ibis-embed …`.
    #[serde(default)]
    pub args: Vec<String>,
    /// ProgIDs this editor opens. Each matches a ProgID equal to it or
    /// starting with it followed by `.` ("ChemDraw.Document" matches
    /// "ChemDraw.Document.6.0"), ignoring ASCII case as Windows does.
    pub prog_ids: Vec<String>,
    /// Whether Insert offers "New <name>".
    #[serde(default)]
    pub new_object: bool,
    /// File extensions (without the dot) it can import, for Insert ▸ File and drops.
    #[serde(default)]
    pub import_extensions: Vec<String>,
    /// File extensions it can export to.
    #[serde(default)]
    pub export_extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalizedName {
    pub ja: String,
    pub en: String,
}

impl LocalizedName {
    /// The name for `locale` ("ja…" gives Japanese, anything else English).
    pub fn get(&self, locale: &str) -> &str {
        if locale.starts_with("ja") { &self.ja } else { &self.en }
    }
}

impl HandlerManifest {
    pub fn parse(text: &str) -> Result<Self> {
        let m: HandlerManifest =
            toml::from_str(text).map_err(|e| Error::Invalid(format!("bad handler manifest: {e}")))?;
        if m.id.trim().is_empty() {
            return Err(Error::Invalid("handler manifest has an empty id".into()));
        }
        if m.command.as_os_str().is_empty() {
            return Err(Error::Invalid(format!("handler {} has an empty command", m.id)));
        }
        Ok(m)
    }

    /// Reads a manifest file; a relative `command` is taken relative to it.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut m = Self::parse(&text)
            .map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))?;
        if m.command.is_relative()
            && m.command.components().count() > 1
            && let Some(dir) = path.parent()
        {
            m.command = dir.join(&m.command);
        }
        Ok(m)
    }

    /// How well this handler matches `prog_id`: the length of the longest
    /// matching entry, or `None`.
    pub fn match_prog_id(&self, prog_id: &str) -> Option<usize> {
        self.prog_ids.iter().filter(|p| prog_id_matches(p, prog_id)).map(|p| p.len()).max()
    }

    pub fn matches_prog_id(&self, prog_id: &str) -> bool {
        self.match_prog_id(prog_id).is_some()
    }

    pub fn can_import(&self, extension: &str) -> bool {
        let ext = extension.trim_start_matches('.');
        self.import_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }

    pub fn can_export(&self, extension: &str) -> bool {
        let ext = extension.trim_start_matches('.');
        self.export_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

fn prog_id_matches(pattern: &str, prog_id: &str) -> bool {
    let (p, id) = (pattern.as_bytes(), prog_id.as_bytes());
    !p.is_empty()
        && id.len() >= p.len()
        && id[..p.len()].eq_ignore_ascii_case(p)
        && (id.len() == p.len() || id[p.len()] == b'.' || p.ends_with(b"."))
}

/// The handler for `prog_id`: the one with the longest matching entry; on a
/// tie, the earlier one.
pub fn find_handler<'a>(handlers: &'a [HandlerManifest], prog_id: &str) -> Option<&'a HandlerManifest> {
    let mut best: Option<(&HandlerManifest, usize)> = None;
    for h in handlers {
        if let Some(n) = h.match_prog_id(prog_id)
            && best.is_none_or(|(_, b)| n > b)
        {
            best = Some((h, n));
        }
    }
    best.map(|(h, _)| h)
}

/// Loads every `*.toml` in `dir`, sorted by file name. A missing directory
/// gives an empty list; a bad file is returned as an error next to its path
/// so the settings window can show it, and does not stop the others.
pub fn load_manifest_dir(dir: &Path) -> Vec<(PathBuf, Result<HandlerManifest>)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("toml")) && p.is_file())
        .collect();
    paths.sort();
    paths.into_iter().map(|p| { let m = HandlerManifest::load(&p); (p, m) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHEM: &str = r#"
id = "jp.ibis.chem"
name.ja = "化学構造式"
name.en = "Chemical Structure"
command = "/opt/ibis-chem"
prog_ids = ["ChemDraw.Document"]
new_object = true
import_extensions = ["cdxml", "cdx", "mol"]
export_extensions = ["cdxml"]
"#;

    #[test]
    fn parses_and_matches() {
        let m = HandlerManifest::parse(CHEM).unwrap();
        assert_eq!(m.name.get("ja-JP"), "化学構造式");
        assert_eq!(m.name.get("en"), "Chemical Structure");
        assert!(m.new_object);
        assert!(m.matches_prog_id("ChemDraw.Document.6.0"));
        assert!(m.matches_prog_id("chemdraw.document"));
        assert!(!m.matches_prog_id("ChemDraw.DocumentX"));
        assert!(!m.matches_prog_id("ChemDraw"));
        assert!(m.can_import(".CDX") && !m.can_import("png"));
        assert!(m.can_export("cdxml") && !m.can_export("cdx"));
    }

    #[test]
    fn defaults_and_errors() {
        let m = HandlerManifest::parse(
            "id = \"x\"\nname = { ja = \"エックス\", en = \"X\" }\ncommand = \"x\"\nprog_ids = [\"X.1\"]",
        )
        .unwrap();
        assert!(!m.new_object && m.import_extensions.is_empty() && m.args.is_empty());
        assert!(HandlerManifest::parse("id = \"x\"").is_err());
        assert!(HandlerManifest::parse(&format!("{CHEM}\nunknown = 1")).is_err());
    }

    #[test]
    fn longest_match_wins() {
        let general = HandlerManifest::parse(&CHEM.replace("jp.ibis.chem", "general")).unwrap();
        let specific = HandlerManifest::parse(
            &CHEM.replace("jp.ibis.chem", "specific").replace("\"ChemDraw.Document\"", "\"ChemDraw.Document.6.0\""),
        )
        .unwrap();
        let hs = [general, specific];
        assert_eq!(find_handler(&hs, "ChemDraw.Document.6.0").unwrap().id, "specific");
        assert_eq!(find_handler(&hs, "ChemDraw.Document.5.0").unwrap().id, "general");
        assert!(find_handler(&hs, "Equation.3").is_none());
    }
}
