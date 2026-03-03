use async_trait::async_trait;
use serde_json::json;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::process::Command;
use uuid::Uuid;

use crate::tools::{
    dynamic::{DynamicToolDef, DynamicToolType},
    Tool,
};

pub struct ScriptTool {
    def: DynamicToolDef,
    data_dir: String,
}

impl ScriptTool {
    pub fn new(def: DynamicToolDef, data_dir: String) -> Result<Self, String> {
        match def.tool_type {
            DynamicToolType::Script { .. } => Ok(Self { def, data_dir }),
            _ => Err("script tool requires Script dynamic tool type".to_owned()),
        }
    }
}

#[async_trait]
impl Tool for ScriptTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn parameters(&self) -> &str {
        "dynamic script parameters"
    }

    fn tool_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for param in &self.def.parameters {
            properties.insert(
                param.name.clone(),
                json!({
                    "type": param.param_type,
                    "description": param.description,
                }),
            );
            if param.required {
                required.push(param.name.clone());
            }
        }

        json!({
            "type": "function",
            "function": {
                "name": self.def.name.clone(),
                "strict": true,
                "description": self.def.description.clone(),
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, String> {
        let (interpreter, source) = match &self.def.tool_type {
            DynamicToolType::Script {
                interpreter,
                source,
            } => (interpreter.as_str(), source.as_str()),
            DynamicToolType::Http { .. } => {
                return Err("script tool requires Script dynamic tool type".to_owned());
            }
        };
        validate_script_source(source)?;

        let tmp_dir = Path::new(&self.data_dir).join("tmp");
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|err| format!("failed to create tmp directory: {err}"))?;

        let extension = if interpreter == "bash" { "sh" } else { "py" };
        let script_path = tmp_dir.join(format!(
            "{}_{}.{}",
            self.def.name,
            Uuid::new_v4(),
            extension
        ));
        std::fs::write(&script_path, source)
            .map_err(|err| format!("failed writing temporary script file: {err}"))?;
        let params_json = serde_json::to_string(&params)
            .map_err(|err| format!("failed to serialize params: {err}"))?;

        let mut command = Command::new(interpreter);
        command
            .arg(&script_path)
            .arg(params_json)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output_result = tokio::time::timeout(Duration::from_secs(30), command.output()).await;

        if let Err(err) = std::fs::remove_file(&script_path) {
            tracing::warn!(path = %script_path.display(), error = %err, "failed to clean up temporary script file");
        }

        match output_result {
            Err(_) => Err("script execution timeout after 30 seconds".to_owned()),
            Ok(Err(err)) => Err(format!("failed executing script: {err}")),
            Ok(Ok(output)) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_owned())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                    if !stderr.is_empty() {
                        Err(stderr)
                    } else {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                        if stdout.is_empty() {
                            Err("script failed with empty stderr".to_owned())
                        } else {
                            Err(stdout)
                        }
                    }
                }
            }
        }
    }
}

pub fn validate_script_source(source: &str) -> Result<(), String> {
    let lower = source.to_lowercase();
    let dangerous_patterns = [
        ("subprocess", lower.contains("subprocess")),
        ("shutil.rmtree", lower.contains("shutil.rmtree")),
        ("rm -rf", lower.contains("rm -rf")),
        ("eval(", lower.contains("eval(")),
        ("exec(", lower.contains("exec(")),
    ];
    for (pattern, present) in dangerous_patterns {
        if present {
            tracing::warn!(pattern, "blocked script due to dangerous pattern");
            return Err(format!("blocked dangerous script pattern: {pattern}"));
        }
    }

    if lower.contains("import os") && lower.contains("system(") {
        tracing::warn!("blocked script due to import os + system pattern");
        return Err("blocked dangerous script pattern: import os + system(".to_owned());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command as StdCommand;

    use serde_json::json;
    use tempfile::tempdir;

    use crate::tools::{
        dynamic::{DynamicToolDef, DynamicToolType, ToolParam},
        Tool,
    };

    use super::ScriptTool;

    fn has_python3() -> bool {
        StdCommand::new("python3")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn sample_python_tool(source: &str) -> DynamicToolDef {
        DynamicToolDef {
            name: "py_tool".to_owned(),
            description: "python tool".to_owned(),
            parameters: vec![ToolParam {
                name: "text".to_owned(),
                param_type: "string".to_owned(),
                description: "input".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Script {
                interpreter: "python3".to_owned(),
                source: source.to_owned(),
            },
        }
    }

    fn sample_bash_tool(source: &str) -> DynamicToolDef {
        DynamicToolDef {
            name: "bash_tool".to_owned(),
            description: "bash tool".to_owned(),
            parameters: vec![ToolParam {
                name: "text".to_owned(),
                param_type: "string".to_owned(),
                description: "input".to_owned(),
                required: true,
            }],
            tool_type: DynamicToolType::Script {
                interpreter: "bash".to_owned(),
                source: source.to_owned(),
            },
        }
    }

    #[tokio::test]
    async fn script_tool_runs_python() {
        if !has_python3() {
            eprintln!("skipping test: python3 not available");
            return;
        }

        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_python_tool("import sys\nprint('hello from python')"),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let output = tool.execute(json!({ "text": "ignored" })).await;
        assert!(output.expect("python script should run").contains("hello"));
    }

    #[tokio::test]
    async fn script_tool_runs_bash() {
        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_bash_tool("echo hello-from-bash"),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let output = tool.execute(json!({ "text": "ignored" })).await;
        assert!(output
            .expect("bash script should run")
            .contains("hello-from-bash"));
    }

    #[tokio::test]
    async fn script_tool_passes_params() {
        if !has_python3() {
            eprintln!("skipping test: python3 not available");
            return;
        }

        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_python_tool(
                "import json,sys\nparams=json.loads(sys.argv[1])\nprint(params['text'])",
            ),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let output = tool
            .execute(json!({ "text": "hello-param" }))
            .await
            .expect("script should run");
        assert_eq!(output.trim(), "hello-param");
    }

    #[tokio::test]
    async fn script_tool_timeout() {
        if !has_python3() {
            eprintln!("skipping test: python3 not available");
            return;
        }

        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_python_tool("import time\ntime.sleep(60)\nprint('done')"),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let err = tool
            .execute(json!({ "text": "ignored" }))
            .await
            .expect_err("script should timeout");
        assert!(
            err.to_lowercase().contains("timeout"),
            "expected timeout error, got: {err}"
        );
    }

    #[tokio::test]
    async fn script_tool_captures_stderr() {
        if !has_python3() {
            eprintln!("skipping test: python3 not available");
            return;
        }

        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_python_tool("import sys\nsys.stderr.write('boom\\n')\nsys.exit(1)"),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let err = tool
            .execute(json!({ "text": "ignored" }))
            .await
            .expect_err("script should fail");
        assert!(err.contains("boom"), "stderr should be surfaced: {err}");
    }

    #[tokio::test]
    async fn script_tool_rejects_dangerous() {
        if !has_python3() {
            eprintln!("skipping test: python3 not available");
            return;
        }

        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_python_tool("import subprocess\nsubprocess.call(['echo','bad'])"),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let err = tool
            .execute(json!({ "text": "ignored" }))
            .await
            .expect_err("dangerous script should be blocked");
        assert!(
            err.to_lowercase().contains("blocked"),
            "expected blocked error, got: {err}"
        );
    }

    #[tokio::test]
    async fn script_tool_cleans_up_temp() {
        if !has_python3() {
            eprintln!("skipping test: python3 not available");
            return;
        }

        let dir = tempdir().expect("tempdir should be created");
        let tool = ScriptTool::new(
            sample_python_tool("print('cleanup')"),
            dir.path().to_str().expect("utf-8 path").to_owned(),
        )
        .expect("tool should build");

        let _ = tool
            .execute(json!({ "text": "ignored" }))
            .await
            .expect("script should run");
        let tmp_dir = dir.path().join("tmp");
        let files = std::fs::read_dir(&tmp_dir)
            .map(|iter| iter.count())
            .unwrap_or(0);
        assert_eq!(files, 0, "expected no leftover temp scripts");
    }
}
