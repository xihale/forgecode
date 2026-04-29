use std::path::Path;

use toml_edit::DocumentMut;

use crate::ForgeConfig;

/// Writes a [`ForgeConfig`] to the user configuration file on disk.
pub struct ConfigWriter {
    config: ForgeConfig,
}

impl ConfigWriter {
    /// Creates a new `ConfigWriter` for the given configuration.
    pub fn new(config: ForgeConfig) -> Self {
        Self { config }
    }

    /// Serializes and writes the configuration to `path`, creating all parent
    /// directories recursively if they do not already exist.
    ///
    /// The output includes a leading `$schema` key pointing to the Forge
    /// configuration JSON schema, which enables editor validation and
    /// auto-complete.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or the file
    /// cannot be written.
    pub fn write(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config_toml = toml_edit::ser::to_string_pretty(&self.config)?;
        let config_toml = preserve_user_config_fields(path, config_toml)?;
        let contents =
            format!("\"$schema\" = \"https://forgecode.dev/schema.json\"\n\n{config_toml}");

        std::fs::write(path, contents)?;

        Ok(())
    }
}

/// Preserves user-only configuration keys that are not yet represented by the
/// typed [`ForgeConfig`] model during full-file rewrites.
fn preserve_user_config_fields(path: &Path, config_toml: String) -> crate::Result<String> {
    let existing_toml = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(config_toml),
        Err(err) => return Err(err.into()),
    };

    let existing = match existing_toml.parse::<DocumentMut>() {
        Ok(document) => document,
        Err(_) => return Ok(config_toml),
    };
    let mut updated = match config_toml.parse::<DocumentMut>() {
        Ok(document) => document,
        Err(_) => return Ok(config_toml),
    };

    for key in ["terminal_context"] {
        if !updated.contains_key(key)
            && let Some(value) = existing.get(key).cloned()
        {
            updated.insert(key, value);
        }
    }

    Ok(updated.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writer_preserves_terminal_context_on_round_trip() {
        let fixture = std::env::temp_dir().join(format!(
            "forge-config-writer-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&fixture).unwrap();
        let path = fixture.join(".forge.toml");
        std::fs::write(&path, "terminal_context = false\nshow_banner = false\n").unwrap();
        let config = ForgeConfig { show_banner: false, ..Default::default() };

        ConfigWriter::new(config).write(&path).unwrap();
        let actual = std::fs::read_to_string(&path).unwrap();

        assert!(actual.contains("terminal_context = false"));
        assert!(actual.contains("show_banner = false"));
        std::fs::remove_dir_all(&fixture).unwrap();
    }
}

