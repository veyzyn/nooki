use std::{collections::HashMap, path::Path};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct PropertiesFile {
    lines: Vec<PropertyLine>,
}

#[derive(Debug, Clone)]
enum PropertyLine {
    Pair { key: String, value: String },
    Raw(String),
}

impl PropertiesFile {
    pub async fn read(path: &Path) -> Result<Self> {
        let text = match tokio::fs::read_to_string(path).await {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self::parse(&text))
    }

    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                    return PropertyLine::Raw(line.to_owned());
                }
                if let Some((key, value)) = line.split_once('=') {
                    PropertyLine::Pair {
                        key: key.trim().to_owned(),
                        value: value.to_owned(),
                    }
                } else {
                    PropertyLine::Raw(line.to_owned())
                }
            })
            .collect();
        Self { lines }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|line| match line {
            PropertyLine::Pair { key: found, value } if found == key => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn update(&mut self, values: &HashMap<&str, String>) {
        let mut remaining = values.clone();
        for line in &mut self.lines {
            if let PropertyLine::Pair { key, value } = line {
                if let Some(new_value) = remaining.remove(key.as_str()) {
                    *value = escape_value(&new_value);
                }
            }
        }
        for (key, value) in remaining {
            self.lines.push(PropertyLine::Pair {
                key: key.to_owned(),
                value: escape_value(&value),
            });
        }
    }

    pub async fn write_atomic(&self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or_else(|| {
            crate::error::Error::Validation("The properties file has no parent folder.".into())
        })?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = path.with_extension("properties.nooki-tmp");
        tokio::fs::write(&temporary, self.to_string()).await?;
        if tokio::fs::try_exists(path).await? {
            let backup = path.with_extension("properties.nooki-prev");
            let _ = tokio::fs::remove_file(&backup).await;
            tokio::fs::rename(path, &backup).await?;
            if let Err(error) = tokio::fs::rename(&temporary, path).await {
                let _ = tokio::fs::rename(&backup, path).await;
                return Err(error.into());
            }
            let _ = tokio::fs::remove_file(backup).await;
        } else {
            tokio::fs::rename(temporary, path).await?;
        }
        Ok(())
    }
}

impl std::fmt::Display for PropertiesFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for line in &self.lines {
            match line {
                PropertyLine::Pair { key, value } => writeln!(formatter, "{key}={value}")?,
                PropertyLine::Raw(value) => writeln!(formatter, "{value}")?,
            }
        }
        Ok(())
    }
}

fn escape_value(value: &str) -> String {
    value.replace('\r', "").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_known_values_and_preserves_unknown_lines() {
        let mut file = PropertiesFile::parse("# hello\nserver-port=25565\nunknown=yes\n");
        file.update(&HashMap::from([
            ("server-port", "25570".to_string()),
            ("motd", "Nooki server".to_string()),
        ]));
        let output = file.to_string();
        assert!(output.contains("# hello"));
        assert!(output.contains("unknown=yes"));
        assert!(output.contains("server-port=25570"));
        assert!(output.contains("motd=Nooki server"));
    }
}
