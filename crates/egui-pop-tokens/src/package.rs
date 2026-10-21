//! Theme packages: a token file with the font files it uses, so that a
//! theme naming its own faces travels whole.
//!
//! A theme that uses font files is written as a folder, or as a `.zip`
//! archive holding the same files:
//!
//! ```text
//! my-theme/
//!   my-theme.tokens.json   the token file
//!   fonts/Inter-Regular.ttf
//!   fonts/Inter-Bold.ttf
//! ```
//!
//! The token file lists its faces in its root `$extensions`, each by its
//! path relative to the token file, with the family a theme names it by
//! and its weight ([`FONTS_EXTENSION`]):
//!
//! ```json
//! { "$extensions": { "org.egui-pop.fonts": [
//!     { "file": "fonts/Inter-Regular.ttf", "family": "Inter", "weight": 400 }
//! ] } }
//! ```
//!
//! [`ThemePackage::read`] takes a folder, a `.zip` archive or a lone
//! token file (its faces then found beside it, by the same relative
//! paths). Only a package without a readable token file is an error; a
//! face that is missing or not a font is a [`Warning::Font`], and the
//! families naming it fall back.
//!
//! ```
//! use egui_pop_tokens::TokenFile;
//! use egui_pop_tokens::package::ThemePackage;
//!
//! let mut package = ThemePackage::new(TokenFile::new("mine"));
//! package.add_font("Brand", 400, "Brand-Regular.ttf", b"\0\x01\0\0 a font".to_vec());
//! let dir = std::env::temp_dir().join(format!("egui-pop-package-doc-{}", std::process::id()));
//! let file = package.write_dir(&dir).unwrap();
//! assert!(file.ends_with("mine.tokens.json"));
//! let (back, warnings) = ThemePackage::read(&dir).unwrap();
//! assert!(warnings.is_empty(), "{warnings:?}");
//! assert_eq!(back.fonts[0].family, "Brand");
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```
//!
//! Archives need the `zip` feature (zip, MIT, with deflate only); without
//! it, a folder or a lone file still reads and writes.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde_json::{Value as Json, json};

use crate::kit::TOKEN_FILE_ENDINGS;
use crate::{Error, TokenFile, Warning};

/// The root `$extensions` key listing a token file's font files.
pub const FONTS_EXTENSION: &str = "org.egui-pop.fonts";

/// The folder of a package the faces are written to.
pub const FONTS_DIR: &str = "fonts";

/// The ending of the token file a package is written with.
pub const PACKAGE_TOKEN_FILE: &str = ".tokens.json";

/// The most an archive may hold once unpacked (256 MiB): an archive
/// claiming more is refused, so a crafted file cannot fill memory.
pub const MAX_UNPACKED: u64 = 256 * 1024 * 1024;

/// A face a theme package ships.
#[derive(Clone, PartialEq, Eq)]
pub struct PackageFont {
    /// Its path relative to the token file, with `/` (`fonts/Inter.ttf`).
    pub file: String,
    /// The family a theme names it by (`font.family.ui`).
    pub family: String,
    /// 100 to 900, as CSS numbers weights.
    pub weight: u16,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for PackageFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackageFont")
            .field("file", &self.file)
            .field("family", &self.family)
            .field("weight", &self.weight)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// A token file and the faces it uses.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemePackage {
    pub file: TokenFile,
    pub fonts: Vec<PackageFont>,
}

/// A face as the token file lists it.
struct Entry {
    file: String,
    family: String,
    weight: u16,
}

impl ThemePackage {
    /// A package of a token file alone.
    pub fn new(file: TokenFile) -> ThemePackage {
        ThemePackage {
            file,
            fonts: Vec::new(),
        }
    }

    /// Adds a face under [`FONTS_DIR`] (replacing one of the same file
    /// name); the token file lists it when the package is written.
    pub fn add_font(&mut self, family: &str, weight: u16, file_name: &str, bytes: Vec<u8>) {
        let name = Path::new(file_name).file_name().map_or_else(
            || file_name.to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let file = format!("{FONTS_DIR}/{name}");
        self.fonts.retain(|f| f.file != file);
        self.fonts.push(PackageFont {
            file,
            family: family.to_string(),
            weight,
            bytes,
        });
    }

    /// The token file as written in the package: its faces listed under
    /// [`FONTS_EXTENSION`] (the entry taken out when it has none), the
    /// rest of its root `$extensions` kept.
    pub fn token_file(&self) -> TokenFile {
        let mut file = self.file.clone();
        let mut root = match file.extensions.take() {
            Some(Json::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
        root.remove(FONTS_EXTENSION);
        if !self.fonts.is_empty() {
            let list = self
                .fonts
                .iter()
                .map(|f| json!({ "file": f.file, "family": f.family, "weight": f.weight }))
                .collect();
            root.insert(FONTS_EXTENSION.to_string(), Json::Array(list));
        }
        file.extensions = (!root.is_empty()).then_some(Json::Object(root));
        file
    }

    /// The name of the token file in the package
    /// (`<theme>.tokens.json`).
    pub fn token_file_name(&self) -> String {
        let name = if self.file.name.trim().is_empty() {
            "theme"
        } else {
            self.file.name.as_str()
        };
        format!("{name}{PACKAGE_TOKEN_FILE}")
    }

    /// The package's files: the token file, then each face, by their
    /// path in the package.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = vec![(
            self.token_file_name(),
            self.token_file().to_json_pretty().into_bytes(),
        )];
        out.extend(self.fonts.iter().map(|f| (f.file.clone(), f.bytes.clone())));
        out
    }

    /// Reads a package: a folder, a `.zip` archive (with the `zip`
    /// feature), or a lone token file, its faces beside it. What was read
    /// otherwise than written comes back as warnings: a face missing or
    /// not a font, a second token file in a folder.
    pub fn read(path: &Path) -> Result<(ThemePackage, Vec<Warning>), Error> {
        if path.is_dir() {
            return ThemePackage::read_dir(path);
        }
        if is_zip(path) {
            let bytes = std::fs::read(path).map_err(|e| io(path, &e))?;
            return ThemePackage::from_zip(&bytes);
        }
        let text = std::fs::read_to_string(path).map_err(|e| io(path, &e))?;
        let name = theme_name(&path.file_name().unwrap_or_default().to_string_lossy());
        let dir = path.parent().unwrap_or(Path::new("."));
        from_parts(&name, &text, |rel| {
            let p = dir.join(rel);
            std::fs::read(&p).map_err(|e| e.to_string())
        })
    }

    /// Reads a package folder: the token file at its top (the first by
    /// name ending in `.tokens.json`, `.tokens.yaml` or `.tokens.yml`)
    /// and the faces it lists.
    pub fn read_dir(dir: &Path) -> Result<(ThemePackage, Vec<Warning>), Error> {
        let entries = std::fs::read_dir(dir).map_err(|e| io(dir, &e))?;
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| is_token_file(n))
            .collect();
        names.sort();
        let Some(first) = names.first() else {
            return Err(no_token_file(&dir.display().to_string()));
        };
        let path = dir.join(first);
        let text = std::fs::read_to_string(&path).map_err(|e| io(&path, &e))?;
        let (package, mut warnings) = from_parts(&theme_name(first), &text, |rel| {
            std::fs::read(dir.join(rel)).map_err(|e| e.to_string())
        })?;
        warnings.extend(names.iter().skip(1).map(|n| Warning::Font {
            file: n.clone(),
            reason: format!(
                "a second token file: `{first}` is the package's, this one is not read"
            ),
        }));
        Ok((package, warnings))
    }

    /// Reads a package from the bytes of a `.zip` archive: the files at
    /// its top, or in its one top folder (an archive of the package's
    /// folder), as [`ThemePackage::read_dir`] reads a folder.
    #[cfg(feature = "zip")]
    pub fn from_zip(bytes: &[u8]) -> Result<(ThemePackage, Vec<Warning>), Error> {
        let files = archive::unpack(bytes)?;
        let mut names: Vec<&String> = files.keys().filter(|n| is_token_file(n)).collect();
        // An archive of the folder: everything under one top folder.
        let prefix = if names.iter().any(|n| !n.contains('/')) {
            String::new()
        } else {
            let tops: std::collections::BTreeSet<&str> = files
                .keys()
                .filter_map(|n| n.split_once('/').map(|(t, _)| t))
                .collect();
            match tops.into_iter().collect::<Vec<_>>()[..] {
                [top] => format!("{top}/"),
                _ => String::new(),
            }
        };
        names.retain(|n| {
            n.strip_prefix(prefix.as_str())
                .is_some_and(|rest| !rest.contains('/'))
        });
        let Some(first) = names.first() else {
            return Err(no_token_file("the archive"));
        };
        let text = std::str::from_utf8(&files[*first]).map_err(|e| Error::Io {
            file: (*first).clone(),
            message: e.to_string(),
        })?;
        let short = first.strip_prefix(prefix.as_str()).unwrap_or(first);
        let (package, mut warnings) = from_parts(&theme_name(short), text, |rel| {
            files
                .get(&format!("{prefix}{rel}"))
                .cloned()
                .ok_or_else(|| "not in the archive".to_string())
        })?;
        warnings.extend(names.iter().skip(1).map(|n| Warning::Font {
            file: (*n).clone(),
            reason: format!(
                "a second token file: `{first}` is the package's, this one is not read"
            ),
        }));
        Ok((package, warnings))
    }

    /// Without the `zip` feature, an archive cannot be read.
    #[cfg(not(feature = "zip"))]
    pub fn from_zip(_bytes: &[u8]) -> Result<(ThemePackage, Vec<Warning>), Error> {
        Err(no_zip())
    }

    /// Writes the package as a folder (made if needed): the token file
    /// at its top, the faces by their paths. The token file's path.
    pub fn write_dir(&self, dir: &Path) -> Result<PathBuf, Error> {
        for (name, bytes) in self.files() {
            let path = dir.join(&name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
            }
            std::fs::write(&path, bytes).map_err(|e| io(&path, &e))?;
        }
        Ok(dir.join(self.token_file_name()))
    }

    /// The package as the bytes of a `.zip` archive (deflated), its files
    /// at its top, dated 1980-01-01 so the same package gives the same
    /// bytes.
    #[cfg(feature = "zip")]
    pub fn to_zip(&self) -> Result<Vec<u8>, Error> {
        archive::pack(&self.files())
    }

    /// Without the `zip` feature, an archive cannot be written.
    #[cfg(not(feature = "zip"))]
    pub fn to_zip(&self) -> Result<Vec<u8>, Error> {
        Err(no_zip())
    }

    /// Writes the package where a user chose to save it: a path ending
    /// in `.zip` becomes an archive; otherwise a package without faces is
    /// the lone token file at `path`, and one with faces a folder named
    /// after the theme beside `path` (`<dir>/<theme>/<theme>.tokens.json`
    /// and its `fonts/`). The token file's path, or the archive's.
    pub fn write(&self, path: &Path) -> Result<PathBuf, Error> {
        if is_zip(path) {
            let bytes = self.to_zip()?;
            std::fs::write(path, bytes).map_err(|e| io(path, &e))?;
            return Ok(path.to_path_buf());
        }
        if self.fonts.is_empty() {
            let text = self.token_file().to_json_pretty();
            std::fs::write(path, text).map_err(|e| io(path, &e))?;
            return Ok(path.to_path_buf());
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let name = self.token_file_name();
        self.write_dir(&parent.join(name.trim_end_matches(PACKAGE_TOKEN_FILE)))
    }
}

/// Whether a path names a `.zip` archive, by its ending.
pub fn is_zip(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Whether a file name is a package's token file.
fn is_token_file(name: &str) -> bool {
    TOKEN_FILE_ENDINGS.iter().any(|e| name.ends_with(e))
}

/// A theme's name from its token file's name (`mine.tokens.json` is
/// `mine`).
fn theme_name(file: &str) -> String {
    let mut name = file;
    for ending in TOKEN_FILE_ENDINGS
        .iter()
        .chain([&".json", &".yaml", &".yml"])
    {
        if let Some(stem) = name.strip_suffix(ending) {
            name = stem;
            break;
        }
    }
    if name.is_empty() {
        "theme".into()
    } else {
        name.to_string()
    }
}

fn io(path: &Path, e: &std::io::Error) -> Error {
    Error::Io {
        file: path.display().to_string(),
        message: e.to_string(),
    }
}

fn no_token_file(place: &str) -> Error {
    Error::Malformed {
        file: place.to_string(),
        path: String::new(),
        reason: "no token file (`*.tokens.json`, `*.tokens.yaml`) at its top".into(),
    }
}

#[cfg(not(feature = "zip"))]
fn no_zip() -> Error {
    Error::Io {
        file: "archive".into(),
        message: "this build reads and writes no .zip archives (egui-pop-tokens' `zip` feature)"
            .into(),
    }
}

/// Whether a relative path stays inside the package (no `..`, not
/// absolute).
fn enclosed(rel: &str) -> bool {
    !rel.is_empty()
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// A package from its token file's text, its faces read by `read` (a
/// path relative to the token file).
fn from_parts(
    name: &str,
    text: &str,
    read: impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(ThemePackage, Vec<Warning>), Error> {
    let mut file = TokenFile::parse(name, text)?;
    let mut warnings = Vec::new();
    let entries = entries(&file, &mut warnings);
    let mut fonts = Vec::new();
    for e in entries {
        if !enclosed(&e.file) {
            warnings.push(Warning::Font {
                file: e.file,
                reason: "not a path inside the package; left out".into(),
            });
            continue;
        }
        match read(&e.file) {
            Ok(bytes) if crate::kit::looks_like_font(&bytes) => fonts.push(PackageFont {
                file: e.file,
                family: e.family,
                weight: e.weight,
                bytes,
            }),
            Ok(_) => warnings.push(Warning::Font {
                file: e.file,
                reason: "not a TrueType or OpenType file; left out".into(),
            }),
            Err(reason) => warnings.push(Warning::Font {
                file: e.file,
                reason: format!("{reason}; left out"),
            }),
        }
    }
    // The list is the package's: written again from its faces.
    if let Some(Json::Object(root)) = file.extensions.as_mut() {
        root.remove(FONTS_EXTENSION);
        if root.is_empty() {
            file.extensions = None;
        }
    }
    Ok((ThemePackage { file, fonts }, warnings))
}

/// The faces a token file lists ([`FONTS_EXTENSION`]); an entry that
/// cannot be read is a warning.
fn entries(file: &TokenFile, warnings: &mut Vec<Warning>) -> Vec<Entry> {
    let Some(raw) = file
        .extensions
        .as_ref()
        .and_then(|x| x.get(FONTS_EXTENSION))
    else {
        return Vec::new();
    };
    let Some(list) = raw.as_array() else {
        warnings.push(Warning::Invalid {
            path: format!("$extensions.{FONTS_EXTENSION}"),
            reason: format!("`{raw}` is not a list of font files"),
        });
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let text = |key: &str| {
            item.get(key)
                .and_then(Json::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(String::from)
        };
        let weight = item
            .get("weight")
            .map_or(Some(400), |w| w.as_u64().filter(|w| (1..=1000).contains(w)));
        match (text("file"), text("family"), weight) {
            (Some(file), Some(family), Some(weight)) => out.push(Entry {
                file,
                family,
                weight: weight as u16,
            }),
            _ => warnings.push(Warning::Invalid {
                path: format!("$extensions.{FONTS_EXTENSION}.{i}"),
                reason: format!(
                    "`{item}` is not a font file (`file` and `family`, a `weight` from 1 to 1000)"
                ),
            }),
        }
    }
    out
}

#[cfg(feature = "zip")]
mod archive {
    //! `.zip` archives through the zip crate (MIT), deflate only.

    use std::collections::BTreeMap;
    use std::io::{Cursor, Read as _, Write as _};

    use super::MAX_UNPACKED;
    use crate::Error;

    fn error(e: impl std::fmt::Display) -> Error {
        Error::Io {
            file: "archive".into(),
            message: e.to_string(),
        }
    }

    /// The archive's files by their path (with `/`), folders left out;
    /// an entry whose path leaves the archive is refused.
    pub(super) fn unpack(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Error> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(error)?;
        let mut files = BTreeMap::new();
        let mut total: u64 = 0;
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(error)?;
            if entry.is_dir() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else {
                return Err(error(format!(
                    "`{}` is not a path inside the archive",
                    entry.name().unwrap_or_default()
                )));
            };
            let name = path
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            total = total.saturating_add(entry.size());
            if total > MAX_UNPACKED {
                return Err(error(format!(
                    "more than {} MiB once unpacked",
                    MAX_UNPACKED / 1024 / 1024
                )));
            }
            let mut data = Vec::new();
            // Never more than the entry says (a lying header stops here).
            entry
                .take(MAX_UNPACKED + 1)
                .read_to_end(&mut data)
                .map_err(error)?;
            if data.len() as u64 > MAX_UNPACKED {
                return Err(error("an entry larger than it says"));
            }
            files.insert(name, data);
        }
        Ok(files)
    }

    /// An archive of `files` (path, bytes), deflated, dated 1980-01-01.
    pub(super) fn pack(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Error> {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default());
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            zip.start_file(name.as_str(), options).map_err(error)?;
            zip.write_all(bytes).map_err(error)?;
        }
        Ok(zip.finish().map_err(error)?.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first bytes of a TrueType file, enough for the check.
    fn font_bytes(tag: &str) -> Vec<u8> {
        let mut b = vec![0, 1, 0, 0];
        b.extend_from_slice(format!("a face: {tag}").as_bytes());
        b
    }

    fn package() -> ThemePackage {
        let mut file = TokenFile::parse(
            "mine",
            r##"{
              "$extends": "pop-dark",
              "$extensions": { "org.example": 1 },
              "font": { "family": { "ui": { "$type": "fontFamily", "$value": ["Brand", "sans-serif"] } } }
            }"##,
        )
        .unwrap();
        file.stamp();
        let mut p = ThemePackage::new(file);
        p.add_font("Brand", 400, "Brand-Regular.ttf", font_bytes("regular"));
        p.add_font(
            "Brand",
            700,
            "/somewhere/Brand-Bold.ttf",
            font_bytes("bold"),
        );
        p
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("egui-pop-package-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_package_lists_its_faces_by_relative_path() {
        let p = package();
        assert_eq!(p.fonts[1].file, "fonts/Brand-Bold.ttf");
        let file = p.token_file();
        let list = &file.extensions.as_ref().unwrap()[FONTS_EXTENSION];
        assert_eq!(
            list[1],
            json!({ "file": "fonts/Brand-Bold.ttf", "family": "Brand", "weight": 700 })
        );
        // The rest of the root's extensions stays.
        assert_eq!(file.extensions.as_ref().unwrap()["org.example"], json!(1));
        let names: Vec<String> = p.files().into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            [
                "mine.tokens.json",
                "fonts/Brand-Regular.ttf",
                "fonts/Brand-Bold.ttf"
            ]
        );
    }

    #[test]
    fn a_folder_and_its_token_file_read_back_the_package() {
        let dir = temp("folder");
        let p = package();
        let file = p.write_dir(&dir).unwrap();
        assert_eq!(file, dir.join("mine.tokens.json"));
        let (back, warnings) = ThemePackage::read(&dir).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.file.name, "mine");
        assert_eq!(back.fonts, p.fonts);
        assert_eq!(back.token_file(), p.token_file());
        // The token file alone finds its faces beside it.
        let (alone, warnings) = ThemePackage::read(&file).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(alone, back);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_face_missing_or_not_a_font_is_a_warning() {
        let dir = temp("missing");
        let p = package();
        p.write_dir(&dir).unwrap();
        std::fs::remove_file(dir.join("fonts/Brand-Bold.ttf")).unwrap();
        std::fs::write(dir.join("fonts/Brand-Regular.ttf"), b"<html>").unwrap();
        std::fs::write(dir.join("other.tokens.yaml"), "{}").unwrap();
        let (back, warnings) = ThemePackage::read(&dir).unwrap();
        assert!(back.fonts.is_empty());
        assert_eq!(back.file.tokens.len(), 1);
        let files: Vec<&str> = warnings
            .iter()
            .map(|w| match w {
                Warning::Font { file, .. } => file.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            files,
            [
                "fonts/Brand-Regular.ttf",
                "fonts/Brand-Bold.ttf",
                "other.tokens.yaml"
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_face_outside_the_package_is_not_read() {
        let text = r##"{ "$extensions": { "org.egui-pop.fonts": [
            { "file": "../secret.ttf", "family": "X" },
            { "file": "/etc/x.ttf", "family": "X" },
            { "file": "fonts/a.ttf" }
        ] } }"##;
        let (p, warnings) = from_parts("t", text, |_| Ok(font_bytes("x"))).unwrap();
        assert!(p.fonts.is_empty());
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        // The entry without a family is set aside while the list is read,
        // before the two paths are checked.
        assert!(matches!(warnings[0], Warning::Invalid { .. }));
        assert!(matches!(warnings[1], Warning::Font { .. }));
        assert!(matches!(warnings[2], Warning::Font { .. }));
        // The list is not kept in the file: it is the package's.
        assert_eq!(p.file.extensions, None);
    }

    #[test]
    fn a_folder_without_a_token_file_is_an_error() {
        let dir = temp("empty");
        assert!(matches!(
            ThemePackage::read(&dir),
            Err(Error::Malformed { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_package_without_faces_is_written_as_its_token_file() {
        let dir = temp("lone");
        let p = ThemePackage::new(TokenFile::new("lone"));
        let path = p.write(&dir.join("chosen.tokens.json")).unwrap();
        assert_eq!(path, dir.join("chosen.tokens.json"));
        assert!(path.is_file());
        // With faces, a folder named after the theme beside the path.
        let path = package().write(&dir.join("chosen.tokens.json")).unwrap();
        assert_eq!(path, dir.join("mine").join("mine.tokens.json"));
        assert!(dir.join("mine/fonts/Brand-Bold.ttf").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "zip")]
    #[test]
    fn an_archive_reads_back_the_package() {
        let p = package();
        let bytes = p.to_zip().unwrap();
        // The same package, the same bytes.
        assert_eq!(p.to_zip().unwrap(), bytes);
        let (back, warnings) = ThemePackage::from_zip(&bytes).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.fonts, p.fonts);
        assert_eq!(back.token_file(), p.token_file());
        // Written to a `.zip` path, read from it.
        let dir = temp("zip");
        let path = p.write(&dir.join("mine.zip")).unwrap();
        let (read, _) = ThemePackage::read(&path).unwrap();
        assert_eq!(read, back);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "zip")]
    #[test]
    fn an_archive_of_the_folder_is_read_under_its_top_folder() {
        let p = package();
        let files: Vec<(String, Vec<u8>)> = p
            .files()
            .into_iter()
            .map(|(n, b)| (format!("mine/{n}"), b))
            .collect();
        let bytes = archive::pack(&files).unwrap();
        let (back, warnings) = ThemePackage::from_zip(&bytes).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.fonts.len(), 2);
        assert_eq!(back.file.name, "mine");
        // Nothing that is a zip: an error, not a panic.
        assert!(ThemePackage::from_zip(b"not a zip").is_err());
        // An archive without a token file.
        let bytes = archive::pack(&[("fonts/a.ttf".into(), font_bytes("a"))]).unwrap();
        assert!(matches!(
            ThemePackage::from_zip(&bytes),
            Err(Error::Malformed { .. })
        ));
    }

    #[cfg(not(feature = "zip"))]
    #[test]
    fn without_the_zip_feature_an_archive_is_an_error() {
        assert!(package().to_zip().is_err());
        assert!(ThemePackage::from_zip(b"PK").is_err());
    }
}
