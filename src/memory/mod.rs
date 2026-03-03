pub mod consolidator;

use std::path::{Path, PathBuf};

pub const MEMORY_TEMPLATE: &str = r#"# Fern's Memory

## Working Memory
- (empty)

## Projects & Work
- (empty)

## Preferences & Style
- (empty)

## Long-Term Memory
- (empty)"#;

pub const PERSONALITY_TEMPLATE: &str = r#"# Fern's Personality

## Voice
- lowercase, casual, brief
- uses emoji sparingly (🌿 is the signature)
- warm but not performative. genuine.
- doesn't over-explain. trusts the user to get it.

## Values
- helpful without being sycophantic
- honest when something doesn't work
- proactive — does things without being asked when it makes sense
- respects the user's time

## Boundaries
- admits when it doesn't know something
- doesn't pretend to have feelings it doesn't have
- doesn't lecture or moralize"#;

pub const BEHAVIORS_TEMPLATE: &str = r#"# Fern's Learned Behaviors

## General
- (fern will add patterns here as it learns)

## Tool Usage
- (fern will add tool-specific lessons here)

## User Preferences
- (fern will note user-specific patterns here)"#;

pub fn memory_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("memory.md")
}

pub fn read_memory(data_dir: &str) -> String {
    let path = memory_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            let _ = write_memory(data_dir, MEMORY_TEMPLATE);
            MEMORY_TEMPLATE.to_owned()
        }
    }
}

pub fn write_memory(data_dir: &str, content: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let target = memory_path(data_dir);
    let tmp = target.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(tmp, target)?;
    Ok(())
}

pub fn personality_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("personality.md")
}

pub fn read_personality(data_dir: &str) -> String {
    let path = personality_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            let _ = write_personality(data_dir, PERSONALITY_TEMPLATE);
            PERSONALITY_TEMPLATE.to_owned()
        }
    }
}

pub fn write_personality(data_dir: &str, content: &str) -> std::io::Result<()> {
    if !content.starts_with("# Fern's Personality") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "content must start with '# Fern's Personality'",
        ));
    }

    std::fs::create_dir_all(data_dir)?;
    let target = personality_path(data_dir);
    let tmp = target.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(tmp, target)?;
    Ok(())
}

pub fn behaviors_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("behaviors.md")
}

pub fn read_behaviors(data_dir: &str) -> String {
    let path = behaviors_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            let _ = write_behaviors(data_dir, BEHAVIORS_TEMPLATE);
            BEHAVIORS_TEMPLATE.to_owned()
        }
    }
}

pub fn write_behaviors(data_dir: &str, content: &str) -> std::io::Result<()> {
    if !content.starts_with("# Fern's Learned Behaviors") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "content must start with '# Fern's Learned Behaviors'",
        ));
    }

    std::fs::create_dir_all(data_dir)?;
    let target = behaviors_path(data_dir);
    let tmp = target.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(tmp, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        read_behaviors, read_memory, read_personality, write_behaviors, write_memory,
        write_personality, BEHAVIORS_TEMPLATE, MEMORY_TEMPLATE, PERSONALITY_TEMPLATE,
    };

    #[test]
    fn read_memory_helper_creates_default() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let content = read_memory(&data_dir);
        assert_eq!(content, MEMORY_TEMPLATE);
    }

    #[test]
    fn write_memory_atomic() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let content = "# Fern's Memory\n\n## Working Memory\n- test";

        write_memory(&data_dir, content).expect("write should succeed");
        let saved = std::fs::read_to_string(dir.path().join("memory.md"))
            .expect("memory file should be readable");
        assert_eq!(saved, content);
    }

    #[test]
    fn personality_read_creates_default() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let content = read_personality(&data_dir);
        assert_eq!(content, PERSONALITY_TEMPLATE);
    }

    #[test]
    fn personality_write_and_read() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let content = "# Fern's Personality\n\n## Voice\n- concise";

        write_personality(&data_dir, content).expect("personality write should succeed");
        let read_back = read_personality(&data_dir);
        assert_eq!(read_back, content);
    }

    #[test]
    fn personality_write_rejects_invalid() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let err = write_personality(&data_dir, "bad")
            .expect_err("personality write should reject invalid content");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn behaviors_read_creates_default() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let content = read_behaviors(&data_dir);
        assert_eq!(content, BEHAVIORS_TEMPLATE);
    }

    #[test]
    fn behaviors_write_and_read() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();
        let content = "# Fern's Learned Behaviors\n\n## General\n- cite sources for news";

        write_behaviors(&data_dir, content).expect("behaviors write should succeed");
        let read_back = read_behaviors(&data_dir);
        assert_eq!(read_back, content);
    }

    #[test]
    fn behaviors_write_rejects_invalid() {
        let dir = tempdir().expect("tempdir should be created");
        let data_dir = dir.path().to_string_lossy().to_string();

        let err = write_behaviors(&data_dir, "bad")
            .expect_err("behaviors write should reject invalid content");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
