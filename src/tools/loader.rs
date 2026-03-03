use crate::tools::{
    dynamic::{load_all_tools, DynamicToolType},
    generator::validate_tool_def,
    http_tool::HttpTool,
    script_tool::ScriptTool,
    Tool, ToolRegistry,
};

pub fn load_and_register_tools(data_dir: &str, registry: &mut ToolRegistry) {
    let defs = load_all_tools(data_dir);
    let mut loaded = 0usize;

    for def in defs {
        if let Err(err) = validate_tool_def(&def) {
            tracing::warn!(tool = %def.name, error = %err, "skipping invalid dynamic tool");
            continue;
        }

        let tool_name = def.name.clone();
        let tool_kind = match &def.tool_type {
            DynamicToolType::Http { .. } => "http",
            DynamicToolType::Script { .. } => "script",
        };

        let tool: Result<Box<dyn Tool>, String> = match &def.tool_type {
            DynamicToolType::Http { .. } => {
                HttpTool::new(def).map(|tool| Box::new(tool) as Box<dyn Tool>)
            }
            DynamicToolType::Script { .. } => ScriptTool::new(def, data_dir.to_owned())
                .map(|tool| Box::new(tool) as Box<dyn Tool>),
        };

        match tool {
            Ok(tool) => {
                registry.register(tool);
                loaded += 1;
                tracing::info!(tool = %tool_name, kind = tool_kind, "loaded dynamic tool");
            }
            Err(err) => {
                tracing::warn!(tool = %tool_name, error = %err, "failed to build dynamic tool executor");
            }
        }
    }

    tracing::info!(loaded, "completed boot-time dynamic tool loading");
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::tempdir;

    use crate::tools::{
        dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
        loader::load_and_register_tools,
        ToolRegistry,
    };

    fn sample_http_tool(name: &str) -> DynamicToolDef {
        DynamicToolDef {
            name: name.to_owned(),
            description: "fetch weather for a city".to_owned(),
            parameters: vec![ToolParam {
                name: "location".to_owned(),
                param_type: "string".to_owned(),
                description: "city".to_owned(),
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
                description: "text input".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Script {
                interpreter: "python3".to_owned(),
                source:
                    "import json,sys\nparams=json.loads(sys.argv[1])\nprint(params['text'].upper())"
                        .to_owned(),
            },
        }
    }

    #[test]
    fn loads_http_tool_on_boot() {
        let dir = tempdir().expect("tempdir should be created");
        sample_http_tool("weather_lookup")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        let mut registry = ToolRegistry::new();
        load_and_register_tools(dir.path().to_str().expect("utf-8 path"), &mut registry);
        assert!(
            registry.get("weather_lookup").is_some(),
            "http tool should be registered"
        );
    }

    #[test]
    fn loads_script_tool_on_boot() {
        let dir = tempdir().expect("tempdir should be created");
        sample_script_tool("uppercase_text")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");

        let mut registry = ToolRegistry::new();
        load_and_register_tools(dir.path().to_str().expect("utf-8 path"), &mut registry);
        assert!(
            registry.get("uppercase_text").is_some(),
            "script tool should be registered"
        );
    }

    #[test]
    fn loads_multiple_tools() {
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

        let mut registry = ToolRegistry::new();
        load_and_register_tools(dir.path().to_str().expect("utf-8 path"), &mut registry);
        assert!(registry.get("weather_a").is_some());
        assert!(registry.get("weather_b").is_some());
        assert!(registry.get("uppercase_text").is_some());
    }

    #[test]
    fn skips_invalid_on_boot() {
        let dir = tempdir().expect("tempdir should be created");
        sample_http_tool("weather_ok")
            .save(dir.path().to_str().expect("utf-8 path"))
            .expect("save should succeed");
        let tools_dir = dir.path().join("tools");
        std::fs::create_dir_all(&tools_dir).expect("tools dir should be created");
        std::fs::write(tools_dir.join("broken.json"), "{bad")
            .expect("invalid file should be written");

        let mut registry = ToolRegistry::new();
        load_and_register_tools(dir.path().to_str().expect("utf-8 path"), &mut registry);
        assert!(registry.get("weather_ok").is_some());
        assert!(registry.get("broken").is_none());
    }
}
