//! User settings, stored as TOML in the user's app data folder.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::render::{Fit, Rotation};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
pub const APP_DIR: &str = "TRCC-LCD";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Picture or GIF to show. Nothing is sent while this is empty.
    pub image: Option<PathBuf>,
    pub fit: Fit,
    /// Degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// JPEG quality, 1 to 100.
    pub quality: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            image: None,
            fit: Fit::Fill,
            rotation: 0,
            quality: 90,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), Error> {
        Rotation::from_degrees(self.rotation)?;
        if !(1..=100).contains(&self.quality) {
            return Err(Error::InvalidQuality(self.quality));
        }
        Ok(())
    }

    #[must_use]
    pub fn rotation(&self) -> Rotation {
        Rotation::from_degrees(self.rotation).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Result<Self, Error> {
        let bad = |e: toml::de::Error| Error::Config(e.message().to_owned());
        let mut table: toml::Table = toml::from_str(text).map_err(bad)?;
        // Removed setting that files from older builds still contain.
        table.remove("keep_alive_secs");
        let cfg = Self::deserialize(toml::Value::Table(table)).map_err(bad)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Loads the file at `path`, or returns defaults when it does not exist.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut text = String::new();
        file.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_CONFIG_BYTES {
            return Err(Error::Config("settings file is too large".into()));
        }
        Self::parse(&text)
    }

    /// Writes to a temporary file first so a crash never leaves a half-written file.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        self.validate()?;
        let text = toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// `%APPDATA%\TRCC-LCD`, the per-user folder for settings.
pub fn app_dir() -> Result<PathBuf, Error> {
    std::env::var_os("APPDATA")
        .map(|d| PathBuf::from(d).join(APP_DIR))
        .ok_or_else(|| Error::Config("APPDATA is not set".into()))
}

pub fn default_path() -> Result<PathBuf, Error> {
    Ok(app_dir()?.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_file() {
        let dir = std::env::temp_dir().join(format!("trcc-cfg-{}", std::process::id()));
        let path = dir.join("config.toml");
        let cfg = Config {
            image: Some(PathBuf::from("picture.png")),
            fit: Fit::Fit,
            rotation: 180,
            quality: 75,
        };
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), cfg);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_keep_alive_key_is_ignored() {
        let cfg = Config::parse("quality = 80\nkeep_alive_secs = 0").unwrap();
        assert_eq!(cfg.quality, 80);
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(!text.contains("keep_alive"), "{text}");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = std::env::temp_dir().join("trcc-does-not-exist.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    #[test]
    fn partial_file_fills_defaults() {
        let cfg = Config::parse("fit = \"stretch\"").unwrap();
        assert_eq!(cfg.fit, Fit::Stretch);
        assert_eq!(cfg.quality, 90);
    }

    #[test]
    fn bad_values_are_refused() {
        assert!(Config::parse("rotation = 45").is_err());
        assert!(Config::parse("quality = 0").is_err());
        assert!(Config::parse("fit = \"zoom\"").is_err());
        assert!(Config::parse("unknown_key = 1").is_err());
        assert!(Config::parse("rotation = -1").is_err());
    }
}
