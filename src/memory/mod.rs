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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{read_memory, write_memory, MEMORY_TEMPLATE};

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
}
