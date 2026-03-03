use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DynamicToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
    pub tool_type: DynamicToolType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type")]
pub enum DynamicToolType {
    Http {
        url_template: String,
        method: String,
        headers: HashMap<String, String>,
        body_template: Option<String>,
        response_jq: Option<String>,
    },
    Script {
        interpreter: String,
        source: String,
    },
}

impl DynamicToolDef {
    pub fn save(&self, data_dir: &str) -> Result<(), String> {
        validate_tool_name(&self.name)?;

        let tools_dir = Path::new(data_dir).join("tools");
        std::fs::create_dir_all(&tools_dir)
            .map_err(|err| format!("failed to create tools directory: {err}"))?;
        let path = tools_dir.join(format!("{}.json", self.name));
        let body = serde_json::to_string_pretty(self)
            .map_err(|err| format!("failed to serialize tool definition: {err}"))?;
        std::fs::write(&path, body)
            .map_err(|err| format!("failed to write tool definition {}: {err}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let body = std::fs::read_to_string(path)
            .map_err(|err| format!("failed reading tool definition {}: {err}", path.display()))?;
        let parsed = serde_json::from_str::<Self>(&body)
            .map_err(|err| format!("failed parsing tool definition {}: {err}", path.display()))?;
        Ok(parsed)
    }
}

pub fn load_all_tools(data_dir: &str) -> Vec<DynamicToolDef> {
    let tools_dir = Path::new(data_dir).join("tools");
    let entries = match std::fs::read_dir(&tools_dir) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::debug!(
                dir = %tools_dir.display(),
                error = %err,
                "tools directory missing or unreadable; skipping dynamic tool load"
            );
            return Vec::new();
        }
    };

    let mut tools = Vec::new();
    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(err) => {
                tracing::warn!(error = %err, "failed to read tools directory entry");
                continue;
            }
        };

        let path = entry.path();
        let is_json = path.extension().and_then(|ext| ext.to_str()) == Some("json");
        if !is_json {
            continue;
        }

        match DynamicToolDef::load(&path) {
            Ok(tool) => tools.push(tool),
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "skipping invalid dynamic tool file")
            }
        }
    }
    tools
}

pub fn delete_tool(data_dir: &str, name: &str) -> Result<(), String> {
    validate_tool_name(name)?;
    let path = Path::new(data_dir)
        .join("tools")
        .join(format!("{name}.json"));
    std::fs::remove_file(&path)
        .map_err(|err| format!("failed to delete tool definition {}: {err}", path.display()))
}

fn validate_tool_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("invalid tool name: empty".to_owned());
    }
    if name.contains('/') || name.contains('\\') || name.contains('.') {
        return Err(format!(
            "invalid tool name '{name}': only simple names are allowed"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::Path};

    use tempfile::tempdir;

    use super::{delete_tool, load_all_tools, DynamicToolDef, DynamicToolType, ToolParam};

    fn sample_http_tool(name: &str) -> DynamicToolDef {
        DynamicToolDef {
            name: name.to_owned(),
            description: "fetch weather for a city".to_owned(),
            parameters: vec![ToolParam {
                name: "location".to_owned(),
                param_type: "string".to_owned(),
                description: "city name".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Http {
                url_template: "https://wttr.in/{{location}}?format=j1".to_owned(),
                method: "GET".to_owned(),
                headers: HashMap::new(),
                body_template: None,
                response_jq: Some(".current_condition[0].temp_F".to_owned()),
            },
        }
    }

    fn sample_script_tool(name: &str) -> DynamicToolDef {
        DynamicToolDef {
            name: name.to_owned(),
            description: "uppercase text".to_owned(),
            parameters: vec![ToolParam {
                name: "text".to_owned(),
                param_type: "string".to_owned(),
                description: "input text".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Script {
                interpreter: "python3".to_owned(),
                source: "import json,sys\np=json.loads(sys.argv[1])\nprint(p['text'].upper())"
                    .to_owned(),
            },
        }
    }

    #[test]
    fn save_and_load_http_tool() {
        let dir = tempdir().expect("tempdir should be created");
        let tool = sample_http_tool("weather_lookup");
        tool.save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        let file_path = dir.path().join("tools/weather_lookup.json");
        let loaded = DynamicToolDef::load(&file_path).expect("load should succeed");
        assert_eq!(loaded, tool);
    }

    #[test]
    fn save_and_load_script_tool() {
        let dir = tempdir().expect("tempdir should be created");
        let tool = sample_script_tool("uppercase_text");
        tool.save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        let file_path = dir.path().join("tools/uppercase_text.json");
        let loaded = DynamicToolDef::load(&file_path).expect("load should succeed");
        assert_eq!(loaded, tool);
    }

    #[test]
    fn load_all_finds_tools() {
        let dir = tempdir().expect("tempdir should be created");
        sample_http_tool("weather_a")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");
        sample_http_tool("weather_b")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");
        sample_script_tool("uppercase_text")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        let all = load_all_tools(dir.path().to_str().expect("utf-8 path"));
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|tool| tool.name == "weather_a"));
        assert!(all.iter().any(|tool| tool.name == "weather_b"));
        assert!(all.iter().any(|tool| tool.name == "uppercase_text"));
    }

    #[test]
    fn load_all_skips_invalid() {
        let dir = tempdir().expect("tempdir should be created");
        sample_http_tool("weather_valid")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        let tools_dir = dir.path().join("tools");
        fs::create_dir_all(&tools_dir).expect("tools dir should be created");
        fs::write(tools_dir.join("broken.json"), "{not-json")
            .expect("invalid json should be written");

        let all = load_all_tools(dir.path().to_str().expect("utf-8 path"));
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "weather_valid");
    }

    #[test]
    fn delete_tool_removes_file() {
        let dir = tempdir().expect("tempdir should be created");
        let tool = sample_http_tool("weather_delete");
        tool.save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        delete_tool(dir.path().to_str().expect("utf-8 path"), "weather_delete")
            .expect("delete should succeed");
        assert!(!Path::new(&format!(
            "{}/tools/weather_delete.json",
            dir.path().to_str().expect("utf-8 path")
        ))
        .exists());
    }

    #[test]
    fn tool_name_sanitization() {
        let dir = tempdir().expect("tempdir should be created");
        let mut tool = sample_http_tool("weather/../../bad");
        let err = tool
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect_err("invalid name should fail");
        assert!(
            err.contains("name"),
            "expected invalid name error message, got: {err}"
        );

        tool.name = "another.bad".to_owned();
        let err = tool
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect_err("invalid name should fail");
        assert!(
            err.contains("name"),
            "expected invalid name error message, got: {err}"
        );
    }
}
