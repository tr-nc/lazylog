use crate::catalog::{PROVIDERS, ProviderKind, TARGET_APPS, TargetApp};
use crate::tooling::{Tool, ToolStatus};
use lazylog_android::{
    AndroidAppState, AndroidDeviceInfo, app_states as android_app_states,
    connected_devices as connected_android_devices,
};
use lazylog_ios::{
    IosAppState, IosDeviceInfo, app_states, connected_devices as connected_ios_devices,
    ensure_device_control_ready,
};
use serde::Serialize;
use std::io::{self, Write};

const AGENT_USAGE: &str = r#"Usage: lazylog agent <COMMAND>

Commands:
  inspect                 Inspect devices, Apps, and log backends without changing them

Options:
  --help, -h              Print this help message

Discover the exact inspect contract:
  lazylog agent inspect --help

Compatibility:
  Existing capture syntax remains supported:
  lazylog --agent <PROVIDER> [OPTIONS]
"#;

const INSPECT_USAGE: &str = r#"Usage: lazylog agent inspect --json

Read-only preflight for agent-operated log capture. This command does not launch or terminate an
App and does not start a capture.

Options:
  --json                  Write the versioned inspection document to stdout
  --help, -h              Print this help message

The JSON includes explicit device counts, per-device App installation/process state, backend
availability, and exact per-device next commands. Process presence does not imply foreground state
or an active Lazylog capture.

Example:
  lazylog agent inspect --json
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentCommand {
    Help,
    InspectHelp,
    InspectJson,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpContext {
    Agent,
    Inspect,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsageError {
    message: String,
    context: HelpContext,
}

impl UsageError {
    fn new(message: impl Into<String>, context: HelpContext) -> Self {
        Self {
            message: message.into(),
            context,
        }
    }

    pub(crate) fn render(&self) -> String {
        let usage = match self.context {
            HelpContext::Agent => AGENT_USAGE,
            HelpContext::Inspect => INSPECT_USAGE,
        };
        format!("Error: {}\n\n{usage}", self.message)
    }
}

pub(crate) fn parse(args: &[String]) -> Result<AgentCommand, UsageError> {
    let Some(action) = args.first().map(String::as_str) else {
        return Err(UsageError::new(
            "Missing command after 'agent'; expected inspect",
            HelpContext::Agent,
        ));
    };

    match action {
        "--help" | "-h" if args.len() == 1 => Ok(AgentCommand::Help),
        "inspect" => parse_inspect(&args[1..]),
        unknown => Err(UsageError::new(
            format!("Unknown agent command '{unknown}'; expected inspect"),
            HelpContext::Agent,
        )),
    }
}

fn parse_inspect(args: &[String]) -> Result<AgentCommand, UsageError> {
    if matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(AgentCommand::InspectHelp);
    }

    let mut json = false;
    for argument in args {
        match argument.as_str() {
            "--json" if json => {
                return Err(UsageError::new(
                    "--json provided multiple times",
                    HelpContext::Inspect,
                ));
            }
            "--json" => json = true,
            "--help" | "-h" => {
                return Err(UsageError::new(
                    "--help cannot be combined with other inspect options",
                    HelpContext::Inspect,
                ));
            }
            unknown => {
                return Err(UsageError::new(
                    format!("Unknown inspect option '{unknown}'; expected --json"),
                    HelpContext::Inspect,
                ));
            }
        }
    }

    if !json {
        return Err(UsageError::new(
            "Missing required option --json",
            HelpContext::Inspect,
        ));
    }

    Ok(AgentCommand::InspectJson)
}

pub(crate) fn run(command: AgentCommand) -> io::Result<()> {
    match command {
        AgentCommand::Help => print_help(AGENT_USAGE),
        AgentCommand::InspectHelp => print_help(INSPECT_USAGE),
        AgentCommand::InspectJson => {
            let report = build_report(&SystemEnvironment);
            let mut stdout = io::stdout().lock();
            serde_json::to_writer_pretty(&mut stdout, &report).map_err(io::Error::other)?;
            writeln!(stdout)
        }
    }
}

fn print_help(help: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(help.as_bytes())
}

trait InspectionEnvironment {
    fn tool_status(&self, tool: Tool) -> ToolStatus;
    fn ios_devices(&self) -> Result<Vec<IosDeviceInfo>, String>;
    fn ios_control_ready(&self, device: &str) -> Result<(), String>;
    fn ios_app_states(&self, device: &str, apps: &[TargetApp]) -> Result<Vec<IosAppState>, String>;
    fn android_devices(&self) -> Result<Vec<AndroidDeviceInfo>, String>;
    fn android_app_states(
        &self,
        device: &str,
        apps: &[TargetApp],
    ) -> Result<Vec<AndroidAppState>, String>;
    fn dyeh_available(&self) -> bool;
}

struct SystemEnvironment;

impl InspectionEnvironment for SystemEnvironment {
    fn tool_status(&self, tool: Tool) -> ToolStatus {
        tool.probe()
    }

    fn ios_devices(&self) -> Result<Vec<IosDeviceInfo>, String> {
        connected_ios_devices().map_err(|error| error.to_string())
    }

    fn ios_control_ready(&self, device: &str) -> Result<(), String> {
        ensure_device_control_ready(device).map_err(|error| error.to_string())
    }

    fn ios_app_states(&self, device: &str, apps: &[TargetApp]) -> Result<Vec<IosAppState>, String> {
        let bundle_ids: Vec<_> = apps.iter().map(|app| app.ios_bundle_id()).collect();
        app_states(device, &bundle_ids).map_err(|error| error.to_string())
    }

    fn android_devices(&self) -> Result<Vec<AndroidDeviceInfo>, String> {
        connected_android_devices().map_err(|error| error.to_string())
    }

    fn android_app_states(
        &self,
        device: &str,
        apps: &[TargetApp],
    ) -> Result<Vec<AndroidAppState>, String> {
        let package_names: Vec<_> = apps.iter().map(|app| app.android_package()).collect();
        android_app_states(device, &package_names).map_err(|error| error.to_string())
    }

    fn dyeh_available(&self) -> bool {
        dirs::home_dir().is_some()
    }
}

#[derive(Serialize)]
struct InspectReport {
    schema_version: u32,
    command: &'static str,
    read_only: bool,
    tools: Vec<ToolStatus>,
    providers: Vec<ProviderReport>,
    ios: IosReport,
    android: AndroidReport,
    capture_session_detection: CaptureSessionDetection,
    diagnostics: Vec<Diagnostic>,
    next_commands: Vec<NextCommand>,
}

#[derive(Serialize)]
struct ProviderReport {
    id: &'static str,
    display_name: &'static str,
    capture_usage: &'static str,
    available: bool,
    ready: bool,
}

#[derive(Serialize)]
struct IosReport {
    device_count: usize,
    log_backends: Vec<BackendReport>,
    devices: Vec<IosDeviceReport>,
}

#[derive(Serialize)]
struct BackendReport {
    id: &'static str,
    mode: &'static str,
    default: bool,
    available: bool,
    controls_device: &'static str,
}

#[derive(Serialize)]
struct IosDeviceReport {
    id: String,
    name: String,
    model: String,
    transport: String,
    connected: bool,
    default: bool,
    control_ready: bool,
    control_error: Option<String>,
    apps: Vec<AppReport>,
}

#[derive(Serialize)]
struct AppReport {
    alias: &'static str,
    display_name: &'static str,
    bundle_id: Option<&'static str>,
    package_name: Option<&'static str>,
    state: &'static str,
    installed: Option<bool>,
    process_present: Option<bool>,
    capture_active: Option<bool>,
    error: Option<String>,
}

#[derive(Serialize)]
struct AndroidReport {
    device_count: usize,
    devices: Vec<AndroidDeviceReport>,
}

#[derive(Serialize)]
struct AndroidDeviceReport {
    serial: String,
    name: String,
    product: Option<String>,
    connected: bool,
    default: bool,
    apps: Vec<AppReport>,
}

#[derive(Serialize)]
struct CaptureSessionDetection {
    supported: bool,
    reason: &'static str,
}

#[derive(Serialize)]
struct Diagnostic {
    level: &'static str,
    component: String,
    message: String,
    retry_command: Option<&'static str>,
}

#[derive(Serialize)]
struct NextCommand {
    purpose: String,
    command: String,
    target: String,
    side_effect: &'static str,
}

fn build_report(environment: &impl InspectionEnvironment) -> InspectReport {
    let devicectl = environment.tool_status(Tool::Devicectl);
    let idevicesyslog = environment.tool_status(Tool::IdeviceSyslog);
    let adb = environment.tool_status(Tool::Adb);
    let mut diagnostics = Vec::new();

    let ios_devices = if devicectl.available {
        match environment.ios_devices() {
            Ok(devices) => devices
                .into_iter()
                .enumerate()
                .map(|(index, device)| {
                    inspect_ios_device(environment, device, index == 0, &mut diagnostics)
                })
                .collect(),
            Err(error) => {
                diagnostics.push(Diagnostic {
                    level: "error",
                    component: "ios.devices".to_string(),
                    message: error,
                    retry_command: Some("lazylog agent inspect --json"),
                });
                Vec::new()
            }
        }
    } else {
        if let Some(error) = devicectl.error.clone() {
            diagnostics.push(Diagnostic {
                level: "warning",
                component: "tool.devicectl".to_string(),
                message: error,
                retry_command: Some("lazylog agent inspect --json"),
            });
        }
        Vec::new()
    };

    let android_devices = if adb.available {
        match environment.android_devices() {
            Ok(devices) => devices
                .into_iter()
                .enumerate()
                .map(|(index, device)| {
                    inspect_android_device(environment, device, index == 0, &mut diagnostics)
                })
                .collect(),
            Err(error) => {
                diagnostics.push(Diagnostic {
                    level: "error",
                    component: "android.devices".to_string(),
                    message: error,
                    retry_command: Some("lazylog agent inspect --json"),
                });
                Vec::new()
            }
        }
    } else {
        if let Some(error) = adb.error.clone() {
            diagnostics.push(Diagnostic {
                level: "warning",
                component: "tool.adb".to_string(),
                message: error,
                retry_command: Some("lazylog agent inspect --json"),
            });
        }
        Vec::new()
    };

    let dyeh_available = environment.dyeh_available();
    let providers = PROVIDERS
        .into_iter()
        .map(|provider| {
            provider_report(
                provider,
                &devicectl,
                &adb,
                &ios_devices,
                &android_devices,
                dyeh_available,
            )
        })
        .collect();
    let log_backends = vec![
        BackendReport {
            id: "devicectl",
            mode: "app-console stdout",
            default: true,
            available: devicectl.available,
            controls_device: "devicectl",
        },
        BackendReport {
            id: "idevicesyslog",
            mode: "device-syslog",
            default: false,
            available: idevicesyslog.available,
            controls_device: "devicectl",
        },
    ];
    let next_commands = next_commands(
        &ios_devices,
        &android_devices,
        idevicesyslog.available,
        dyeh_available,
    );

    InspectReport {
        schema_version: 1,
        command: "lazylog agent inspect --json",
        read_only: true,
        tools: vec![devicectl, idevicesyslog, adb],
        providers,
        ios: IosReport {
            device_count: ios_devices.len(),
            log_backends,
            devices: ios_devices,
        },
        android: AndroidReport {
            device_count: android_devices.len(),
            devices: android_devices,
        },
        capture_session_detection: CaptureSessionDetection {
            supported: false,
            reason: "Lazylog capture sessions are not registered across processes; process_present does not imply capture_active",
        },
        diagnostics,
        next_commands,
    }
}

fn inspect_ios_device(
    environment: &impl InspectionEnvironment,
    device: IosDeviceInfo,
    is_default: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> IosDeviceReport {
    let control_result = environment.ios_control_ready(&device.identifier);
    let control_ready = control_result.is_ok();
    let control_error = control_result.err();
    if let Some(error) = &control_error {
        diagnostics.push(Diagnostic {
            level: "error",
            component: format!("ios.device.{}.control", device.identifier),
            message: error.clone(),
            retry_command: Some("lazylog agent inspect --json"),
        });
    }

    let apps = if control_ready {
        match environment.ios_app_states(&device.identifier, &TARGET_APPS) {
            Ok(states) if states.len() == TARGET_APPS.len() => TARGET_APPS
                .into_iter()
                .zip(states)
                .map(|(app, state)| ios_app_report(app, state))
                .collect(),
            Ok(_) => {
                failed_ios_app_reports("App status query returned an unexpected number of states")
            }
            Err(error) => {
                diagnostics.push(Diagnostic {
                    level: "error",
                    component: format!("ios.device.{}.apps", device.identifier),
                    message: error.clone(),
                    retry_command: Some("lazylog agent inspect --json"),
                });
                failed_ios_app_reports(&error)
            }
        }
    } else {
        failed_ios_app_reports("Device control is unavailable, so App state was not queried")
    };

    IosDeviceReport {
        id: device.identifier,
        name: device.name,
        model: device.model,
        transport: device.transport,
        connected: true,
        default: is_default,
        control_ready,
        control_error,
        apps,
    }
}

fn ios_app_report(app: TargetApp, state: IosAppState) -> AppReport {
    let (state, installed, process_present) = match state {
        IosAppState::ProcessPresent => ("process_present", Some(true), Some(true)),
        IosAppState::NoProcess => ("no_process", Some(true), Some(false)),
        IosAppState::NotInstalled => ("not_installed", Some(false), Some(false)),
    };
    AppReport {
        alias: app.alias(),
        display_name: app.ios_display_name(),
        bundle_id: Some(app.ios_bundle_id()),
        package_name: None,
        state,
        installed,
        process_present,
        capture_active: None,
        error: None,
    }
}

fn failed_ios_app_reports(error: &str) -> Vec<AppReport> {
    TARGET_APPS
        .into_iter()
        .map(|app| AppReport {
            alias: app.alias(),
            display_name: app.ios_display_name(),
            bundle_id: Some(app.ios_bundle_id()),
            package_name: None,
            state: "detection_failed",
            installed: None,
            process_present: None,
            capture_active: None,
            error: Some(error.to_string()),
        })
        .collect()
}

fn inspect_android_device(
    environment: &impl InspectionEnvironment,
    device: AndroidDeviceInfo,
    is_default: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> AndroidDeviceReport {
    let apps = match environment.android_app_states(&device.serial, &TARGET_APPS) {
        Ok(states) if states.len() == TARGET_APPS.len() => TARGET_APPS
            .into_iter()
            .zip(states)
            .map(|(app, state)| android_app_report(app, state))
            .collect(),
        Ok(_) => {
            failed_android_app_reports("App status query returned an unexpected number of states")
        }
        Err(error) => {
            diagnostics.push(Diagnostic {
                level: "error",
                component: format!("android.device.{}.apps", device.serial),
                message: error.clone(),
                retry_command: Some("lazylog agent inspect --json"),
            });
            failed_android_app_reports(&error)
        }
    };

    AndroidDeviceReport {
        serial: device.serial,
        name: device.name,
        product: device.product,
        connected: true,
        default: is_default,
        apps,
    }
}

fn android_app_report(app: TargetApp, state: AndroidAppState) -> AppReport {
    let (state, installed, process_present) = match state {
        AndroidAppState::ProcessPresent => ("process_present", Some(true), Some(true)),
        AndroidAppState::NoProcess => ("no_process", Some(true), Some(false)),
        AndroidAppState::NotInstalled => ("not_installed", Some(false), Some(false)),
    };
    AppReport {
        alias: app.alias(),
        display_name: app.android_display_name(),
        bundle_id: None,
        package_name: Some(app.android_package()),
        state,
        installed,
        process_present,
        capture_active: None,
        error: None,
    }
}

fn failed_android_app_reports(error: &str) -> Vec<AppReport> {
    TARGET_APPS
        .into_iter()
        .map(|app| AppReport {
            alias: app.alias(),
            display_name: app.android_display_name(),
            bundle_id: None,
            package_name: Some(app.android_package()),
            state: "detection_failed",
            installed: None,
            process_present: None,
            capture_active: None,
            error: Some(error.to_string()),
        })
        .collect()
}

fn provider_report(
    provider: ProviderKind,
    devicectl: &ToolStatus,
    adb: &ToolStatus,
    ios_devices: &[IosDeviceReport],
    android_devices: &[AndroidDeviceReport],
    dyeh_available: bool,
) -> ProviderReport {
    match provider {
        ProviderKind::Ios => ProviderReport {
            id: "ios",
            display_name: provider.display_name(),
            capture_usage: "lazylog --agent --ios --ios-device <id> --ios-app <effectcam|douyin> [--ios-log-backend <devicectl|idevicesyslog>] [--duration <seconds>]",
            available: devicectl.available,
            ready: ios_devices.iter().any(|device| device.control_ready),
        },
        ProviderKind::Android => ProviderReport {
            id: "android",
            display_name: provider.display_name(),
            capture_usage: "lazylog --agent --android --android-device <serial> [--duration <seconds>]",
            available: adb.available,
            ready: !android_devices.is_empty(),
        },
        ProviderKind::DyehPreview => ProviderReport {
            id: "dyeh-preview",
            display_name: provider.display_name(),
            capture_usage: "lazylog --agent --dyeh-preview [--duration <seconds>]",
            available: dyeh_available,
            ready: dyeh_available,
        },
        ProviderKind::DyehEditor => ProviderReport {
            id: "dyeh-editor",
            display_name: provider.display_name(),
            capture_usage: "lazylog --agent --dyeh-editor [--duration <seconds>]",
            available: dyeh_available,
            ready: dyeh_available,
        },
    }
}

fn next_commands(
    ios_devices: &[IosDeviceReport],
    android_devices: &[AndroidDeviceReport],
    idevicesyslog_available: bool,
    dyeh_available: bool,
) -> Vec<NextCommand> {
    let mut commands = Vec::new();
    for device in ios_devices.iter().filter(|device| device.control_ready) {
        for app in device.apps.iter().filter(|app| app.installed == Some(true)) {
            commands.push(NextCommand {
                purpose: format!(
                    "Capture {} on {} with the default iOS backend",
                    app.alias, device.name
                ),
                command: format!(
                    "lazylog --agent --ios --ios-device {} --ios-app {} --ios-log-backend devicectl --duration 30",
                    shell_quote(&device.id),
                    app.alias
                ),
                target: format!("ios:{}:{}", device.id, app.alias),
                side_effect: "Terminates any existing target App process and relaunches it",
            });
            if idevicesyslog_available {
                commands.push(NextCommand {
                    purpose: format!(
                        "Capture {} on {} with the optional device-syslog backend",
                        app.alias, device.name
                    ),
                    command: format!(
                        "lazylog --agent --ios --ios-device {} --ios-app {} --ios-log-backend idevicesyslog --duration 30",
                        shell_quote(&device.id),
                        app.alias
                    ),
                    target: format!("ios:{}:{}", device.id, app.alias),
                    side_effect: "Terminates any existing target App process and relaunches it",
                });
            }
        }
    }
    for device in android_devices {
        commands.push(NextCommand {
            purpose: format!("Capture Android effect logs from {}", device.name),
            command: format!(
                "lazylog --agent --android --android-device {} --duration 30",
                shell_quote(&device.serial)
            ),
            target: format!("android:{}", device.serial),
            side_effect: "Starts adb logcat capture without launching an App",
        });
    }
    if dyeh_available {
        commands.push(NextCommand {
            purpose: "Capture DYEH preview logs".to_string(),
            command: "lazylog --agent --dyeh-preview --duration 30".to_string(),
            target: "dyeh-preview".to_string(),
            side_effect: "Reads the local DYEH log directory",
        });
        commands.push(NextCommand {
            purpose: "Capture DYEH editor logs".to_string(),
            command: "lazylog --agent --dyeh-editor --duration 30".to_string(),
            target: "dyeh-editor".to_string(),
            side_effect: "Reads the local DYEH log directory",
        });
    }
    commands
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._:".contains(&byte))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parser_exposes_scoped_help_and_json_inspection() {
        assert_eq!(parse(&args(&["--help"])).unwrap(), AgentCommand::Help);
        assert_eq!(
            parse(&args(&["inspect", "--help"])).unwrap(),
            AgentCommand::InspectHelp
        );
        assert_eq!(
            parse(&args(&["inspect", "--json"])).unwrap(),
            AgentCommand::InspectJson
        );
    }

    #[test]
    fn missing_json_recovers_at_inspect_context() {
        let rendered = parse(&args(&["inspect"])).unwrap_err().render();

        assert!(rendered.starts_with("Error: Missing required option --json"));
        assert!(rendered.contains("Usage: lazylog agent inspect --json"));
        assert!(!rendered.contains("--dyeh-preview"));
    }

    #[test]
    fn unknown_action_recovers_at_agent_context() {
        let rendered = parse(&args(&["capture"])).unwrap_err().render();

        assert!(rendered.starts_with("Error: Unknown agent command 'capture'"));
        assert!(rendered.contains("Usage: lazylog agent <COMMAND>"));
        assert!(rendered.contains("lazylog --agent <PROVIDER>"));
        assert!(!rendered.contains("--json                  Write"));
    }

    struct FakeEnvironment;

    impl InspectionEnvironment for FakeEnvironment {
        fn tool_status(&self, tool: Tool) -> ToolStatus {
            let id = match tool {
                Tool::Devicectl => "devicectl",
                Tool::IdeviceSyslog => "idevicesyslog",
                Tool::Adb => "adb",
            };
            ToolStatus {
                id,
                available: true,
                error: None,
            }
        }

        fn ios_devices(&self) -> Result<Vec<IosDeviceInfo>, String> {
            Ok(vec![IosDeviceInfo {
                identifier: "ios-device".to_string(),
                name: "Test iPhone".to_string(),
                model: "iPhone".to_string(),
                transport: "wired".to_string(),
            }])
        }

        fn ios_control_ready(&self, _device: &str) -> Result<(), String> {
            Ok(())
        }

        fn ios_app_states(
            &self,
            _device: &str,
            _apps: &[TargetApp],
        ) -> Result<Vec<IosAppState>, String> {
            Ok(vec![IosAppState::ProcessPresent, IosAppState::NotInstalled])
        }

        fn android_devices(&self) -> Result<Vec<AndroidDeviceInfo>, String> {
            Ok(vec![AndroidDeviceInfo {
                serial: "android-device".to_string(),
                name: "Test Android".to_string(),
                product: None,
            }])
        }

        fn android_app_states(
            &self,
            _device: &str,
            _apps: &[TargetApp],
        ) -> Result<Vec<AndroidAppState>, String> {
            Ok(vec![
                AndroidAppState::NoProcess,
                AndroidAppState::ProcessPresent,
            ])
        }

        fn dyeh_available(&self) -> bool {
            true
        }
    }

    #[test]
    fn report_distinguishes_process_presence_from_capture_state() {
        let report = build_report(&FakeEnvironment);
        let json = serde_json::to_value(report).unwrap();
        let effectcam = &json["ios"]["devices"][0]["apps"][0];

        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["ios"]["device_count"], 1);
        assert_eq!(json["android"]["device_count"], 1);
        assert_eq!(effectcam["installed"], true);
        assert_eq!(effectcam["process_present"], true);
        assert!(effectcam["capture_active"].is_null());
        assert_eq!(json["capture_session_detection"]["supported"], false);
        assert!(
            json["next_commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|command| command["command"]
                    .as_str()
                    .unwrap()
                    .contains("--ios-device ios-device"))
        );
        assert!(
            json["next_commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|command| command["command"]
                    .as_str()
                    .unwrap()
                    .contains("--android-device android-device"))
        );
        assert!(
            json["next_commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|command| command["command"]
                    .as_str()
                    .unwrap()
                    .contains("--ios-app effectcam"))
        );
        assert!(
            !json["next_commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|command| command["command"]
                    .as_str()
                    .unwrap()
                    .contains("--ios-app douyin"))
        );
        assert_eq!(
            json["android"]["devices"][0]["apps"][1]["process_present"],
            true
        );
        assert_eq!(
            json["android"]["devices"][0]["apps"][1]["package_name"],
            TargetApp::DOUYIN_ANDROID_PACKAGE
        );
    }

    #[test]
    fn generated_commands_shell_quote_unsafe_device_ids() {
        assert_eq!(shell_quote("safe-device:1"), "safe-device:1");
        assert_eq!(shell_quote("device with space"), "'device with space'");
        assert_eq!(shell_quote("device'quote"), "'device'\\''quote'");
    }
}
