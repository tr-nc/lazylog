use serde::Serialize;
use std::io;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Devicectl,
    IdeviceSyslog,
    Adb,
}

impl Tool {
    fn definition(self) -> ToolDefinition {
        match self {
            Self::Devicectl => ToolDefinition {
                id: "devicectl",
                program: "xcrun",
                args: &["--find", "devicectl"],
                unavailable: "devicectl was not found in the selected Xcode. Install or select a current Xcode to use iOS modes",
            },
            Self::IdeviceSyslog => ToolDefinition {
                id: "idevicesyslog",
                program: "idevicesyslog",
                args: &["--version"],
                unavailable: "idevicesyslog is not installed. This optional backend is needed only when --ios-log-backend idevicesyslog is selected; on macOS install it with: brew install libimobiledevice",
            },
            Self::Adb => ToolDefinition {
                id: "adb",
                program: "adb",
                args: &["version"],
                unavailable: "adb was not found in PATH. Install Android SDK Platform-Tools; on macOS: brew install android-platform-tools",
            },
        }
    }

    pub(crate) fn probe(self) -> ToolStatus {
        let definition = self.definition();
        match Command::new(definition.program)
            .args(definition.args)
            .output()
        {
            Ok(output) if output.status.success() => ToolStatus {
                id: definition.id,
                available: true,
                error: None,
            },
            Ok(output) => {
                let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                ToolStatus {
                    id: definition.id,
                    available: false,
                    error: Some(if detail.is_empty() {
                        definition.unavailable.to_string()
                    } else {
                        format!("{}: {detail}", definition.unavailable)
                    }),
                }
            }
            Err(error) => ToolStatus {
                id: definition.id,
                available: false,
                error: Some(if error.kind() == io::ErrorKind::NotFound {
                    definition.unavailable.to_string()
                } else {
                    format!("{}: {error}", definition.unavailable)
                }),
            },
        }
    }

    pub(crate) fn require(self) -> io::Result<()> {
        let status = self.probe();
        if status.available {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Error: {}",
                    status
                        .error
                        .unwrap_or_else(|| format!("{} is unavailable", status.id))
                ),
            ))
        }
    }
}

struct ToolDefinition {
    id: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    unavailable: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ToolStatus {
    pub(crate) id: &'static str,
    pub(crate) available: bool,
    pub(crate) error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_actionable_unavailable_guidance() {
        for tool in [Tool::Devicectl, Tool::IdeviceSyslog, Tool::Adb] {
            let definition = tool.definition();
            assert!(!definition.id.is_empty());
            assert!(!definition.program.is_empty());
            assert!(!definition.unavailable.is_empty());
        }
    }
}
