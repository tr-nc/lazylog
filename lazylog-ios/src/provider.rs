use anyhow::{Result, bail};
use lazylog_framework::provider::{LogProvider, ProviderDisconnectReason, ProviderStatus};
use serde_json::Value;
use std::fs;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IosDeviceInfo {
    pub identifier: String,
    pub name: String,
    pub model: String,
    pub transport: String,
}

fn connected_device_infos(document: &Value) -> Result<Vec<IosDeviceInfo>> {
    let devices = document
        .pointer("/result/devices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("devicectl returned no device list"))?;

    let mut connected: Vec<_> = devices
        .iter()
        .filter(|device| {
            device
                .pointer("/hardwareProperties/platform")
                .and_then(Value::as_str)
                == Some("iOS")
                && device
                    .pointer("/hardwareProperties/reality")
                    .and_then(Value::as_str)
                    == Some("physical")
                && device
                    .pointer("/connectionProperties/transportType")
                    .and_then(Value::as_str)
                    .is_some()
        })
        .filter_map(|device| {
            let identifier = device.get("identifier")?.as_str()?.to_string();
            let name = device
                .pointer("/deviceProperties/name")
                .and_then(Value::as_str)
                .unwrap_or("iPhone")
                .to_string();
            let model = device
                .pointer("/hardwareProperties/marketingName")
                .or_else(|| device.pointer("/hardwareProperties/productType"))
                .and_then(Value::as_str)
                .unwrap_or("iPhone")
                .to_string();
            let transport = device
                .pointer("/connectionProperties/transportType")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            Some((
                device_priority(device),
                IosDeviceInfo {
                    identifier,
                    name,
                    model,
                    transport,
                },
            ))
        })
        .collect();

    connected.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(connected.into_iter().map(|(_, device)| device).collect())
}

#[cfg(test)]
fn select_default_device(devices_json: &str) -> Result<String> {
    let document: Value = serde_json::from_str(devices_json)?;
    connected_device_infos(&document)?
        .into_iter()
        .next()
        .map(|device| device.identifier)
        .ok_or_else(|| anyhow::anyhow!("No connected physical iOS device was found"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DeviceSnapshot {
    connected: bool,
    udid: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DisconnectEvidence {
    usb_was_attached: bool,
    usb_is_attached: Option<bool>,
    device_connected: Option<bool>,
    app_running: Option<bool>,
}

fn classify_disconnect(evidence: DisconnectEvidence) -> ProviderDisconnectReason {
    if evidence.usb_was_attached && evidence.usb_is_attached == Some(false) {
        return ProviderDisconnectReason::UsbDisconnected;
    }

    match (evidence.device_connected, evidence.app_running) {
        (Some(false), _) => ProviderDisconnectReason::DeviceDisconnected,
        (Some(true), Some(false)) => ProviderDisconnectReason::TargetExited,
        (Some(true), Some(true)) => ProviderDisconnectReason::CaptureFailed,
        (None, _) if evidence.usb_is_attached == Some(true) => {
            ProviderDisconnectReason::DeviceDisconnected
        }
        _ => ProviderDisconnectReason::Unknown,
    }
}

fn device_snapshot(document: &Value, identifier: &str) -> Option<DeviceSnapshot> {
    let device = document
        .pointer("/result/devices")?
        .as_array()?
        .iter()
        .find(|device| device.get("identifier").and_then(Value::as_str) == Some(identifier))?;
    let connected = device
        .pointer("/connectionProperties/transportType")
        .and_then(Value::as_str)
        .is_some();
    let udid = device
        .pointer("/hardwareProperties/udid")
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(DeviceSnapshot { connected, udid })
}

fn run_devicectl_json(args: &[&str], output_stem: &str) -> Result<Value> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let output_path = std::env::temp_dir().join(format!(
        "lazylog-{output_stem}-{}-{unique}.json",
        std::process::id()
    ));

    let result = (|| {
        let output = StdCommand::new("xcrun")
            .arg("devicectl")
            .args(args)
            .arg("--json-output")
            .arg(&output_path)
            .arg("--quiet")
            .output()?;
        let document: Value = serde_json::from_str(&fs::read_to_string(&output_path)?)?;
        let outcome = document.pointer("/info/outcome").and_then(Value::as_str);

        if !output.status.success() || outcome == Some("failed") {
            bail!(
                "devicectl command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(document)
    })();

    let _ = fs::remove_file(output_path);
    result
}

fn query_device_snapshot(identifier: &str) -> Result<Option<DeviceSnapshot>> {
    let document = run_devicectl_json(&["list", "devices"], "device-status")?;
    Ok(device_snapshot(&document, identifier))
}

fn usb_device_is_attached(udid: &str) -> Option<bool> {
    let output = StdCommand::new("/usr/sbin/ioreg")
        .args(["-p", "IOUSB", "-l", "-w", "0"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let normalized_udid = udid.replace('-', "").to_ascii_uppercase();
    let usb_tree = String::from_utf8_lossy(&output.stdout)
        .replace('-', "")
        .to_ascii_uppercase();
    Some(usb_tree.contains(&normalized_udid))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IosAppState {
    Running,
    NotRunning,
    NotInstalled,
}

/// Report whether an installed App currently has a process on the device.
///
/// `Running` includes foreground, background, and suspended processes. It does
/// not imply that the App is visible or that Lazylog launched it.
pub fn app_state(device: &str, bundle_id: &str) -> Result<IosAppState> {
    let apps = run_devicectl_json(
        &[
            "device",
            "info",
            "apps",
            "--device",
            device,
            "--bundle-id",
            bundle_id,
        ],
        "app-status",
    )?;
    let Some(app_url) = apps
        .pointer("/result/apps")
        .and_then(Value::as_array)
        .and_then(|apps| apps.first())
        .and_then(|app| app.get("url"))
        .and_then(Value::as_str)
    else {
        return Ok(IosAppState::NotInstalled);
    };

    let processes = run_devicectl_json(
        &["device", "info", "processes", "--device", device],
        "process-status",
    )?;
    let is_running = processes
        .pointer("/result/runningProcesses")
        .and_then(Value::as_array)
        .is_some_and(|processes| {
            processes.iter().any(|process| {
                process
                    .get("executable")
                    .and_then(Value::as_str)
                    .is_some_and(|executable| executable.starts_with(app_url))
            })
        });
    Ok(if is_running {
        IosAppState::Running
    } else {
        IosAppState::NotRunning
    })
}

fn query_app_running(device: &str, bundle_id: &str) -> Result<bool> {
    Ok(matches!(
        app_state(device, bundle_id)?,
        IosAppState::Running
    ))
}

fn disconnect_reason(
    device: &str,
    bundle_id: &str,
    initial_device: Option<&DeviceSnapshot>,
    usb_was_attached: bool,
) -> ProviderDisconnectReason {
    let current_device = query_device_snapshot(device).ok().flatten();
    let usb_is_attached = initial_device
        .and_then(|snapshot| snapshot.udid.as_deref())
        .and_then(usb_device_is_attached);
    let device_connected = current_device.as_ref().map(|snapshot| snapshot.connected);
    let app_running = if device_connected == Some(true) {
        query_app_running(device, bundle_id).ok()
    } else {
        None
    };

    classify_disconnect(DisconnectEvidence {
        usb_was_attached,
        usb_is_attached,
        device_connected,
        app_running,
    })
}

fn device_priority(device: &Value) -> (u8, &str) {
    let transport = device
        .pointer("/connectionProperties/transportType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let wired = u8::from(transport == "wired");
    let last_connection = device
        .pointer("/connectionProperties/lastConnectionDate")
        .and_then(Value::as_str)
        .unwrap_or_default();
    (wired, last_connection)
}

/// Return the preferred currently connected physical iOS device.
///
/// A wired device wins over network devices. Within the same transport class,
/// the most recently connected device wins. Historical paired devices without
/// an active transport are ignored.
pub fn default_device_identifier() -> Result<String> {
    connected_devices()?
        .into_iter()
        .next()
        .map(|device| device.identifier)
        .ok_or_else(|| anyhow::anyhow!("No connected physical iOS device was found"))
}

/// Return all currently connected physical iOS devices in preferred order.
pub fn connected_devices() -> Result<Vec<IosDeviceInfo>> {
    let document = run_devicectl_json(&["list", "devices"], "devices")?;
    connected_device_infos(&document)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum IosLogSource {
    AppConsole { device: String, bundle_id: String },
}

#[derive(Debug, PartialEq, Eq)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
    inherit_stdin: bool,
}

impl IosLogSource {
    fn command_spec(&self) -> CommandSpec {
        match self {
            Self::AppConsole { device, bundle_id } => CommandSpec {
                // devicectl only forwards the launched app's console output
                // when it has a terminal. macOS script(1) supplies that PTY
                // while keeping the resulting stream pipeable into Lazylog.
                program: "/usr/bin/script".to_string(),
                args: vec![
                    "-q".to_string(),
                    "/dev/null".to_string(),
                    "xcrun".to_string(),
                    "devicectl".to_string(),
                    "device".to_string(),
                    "process".to_string(),
                    "launch".to_string(),
                    "--device".to_string(),
                    device.clone(),
                    "--terminate-existing".to_string(),
                    "--console".to_string(),
                    bundle_id.clone(),
                ],
                inherit_stdin: false,
            },
        }
    }
}

/// Log provider for iOS device logs.
///
/// Launches one installed app through Apple's devicectl and attaches its
/// standard streams.
pub struct IosLogProvider {
    source: IosLogSource,
    log_buffer: Arc<Mutex<Vec<String>>>,
    should_stop: Arc<Mutex<bool>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    child_process: Option<Arc<Mutex<Option<Child>>>>,
    status: Arc<Mutex<ProviderStatus>>,
}

impl IosLogProvider {
    pub fn new_app_console(device: impl Into<String>, bundle_id: impl Into<String>) -> Self {
        Self::with_source(IosLogSource::AppConsole {
            device: device.into(),
            bundle_id: bundle_id.into(),
        })
    }

    fn with_source(source: IosLogSource) -> Self {
        Self {
            source,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(Mutex::new(false)),
            thread_handle: None,
            child_process: None,
            status: Arc::new(Mutex::new(ProviderStatus::Connecting)),
        }
    }
}

impl LogProvider for IosLogProvider {
    fn start(&mut self) -> Result<()> {
        log::debug!("IosLogProvider: Starting");

        if let Ok(mut stop) = self.should_stop.lock() {
            *stop = false;
        }
        Self::set_status(&self.status, ProviderStatus::Connecting);

        let log_buffer = self.log_buffer.clone();
        let should_stop = self.should_stop.clone();
        let child_process = Arc::new(Mutex::new(None));
        self.child_process = Some(child_process.clone());
        let source = self.source.clone();
        let provider_status = self.status.clone();

        // spawn a thread to run the command-line tool
        let handle = thread::spawn(move || {
            // we need a tokio runtime in this thread
            let rt = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("Failed to create tokio runtime: {}", e);
                    Self::set_status(
                        &provider_status,
                        ProviderStatus::Disconnected(ProviderDisconnectReason::CaptureFailed),
                    );
                    return;
                }
            };

            rt.block_on(async {
                match Self::run_log_source(
                    source,
                    log_buffer,
                    should_stop,
                    child_process,
                    provider_status.clone(),
                )
                .await
                {
                    Ok(_) => log::debug!("iOS log source stopped normally"),
                    Err(e) => {
                        log::error!("iOS log source error: {}", e);
                        if !Self::current_status(&provider_status).is_disconnected() {
                            Self::set_status(
                                &provider_status,
                                ProviderStatus::Disconnected(
                                    ProviderDisconnectReason::CaptureFailed,
                                ),
                            );
                        }
                    }
                }
            });
        });

        self.thread_handle = Some(handle);

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        log::debug!("IosLogProvider: Stopping");

        // signal the thread to stop
        if let Ok(mut stop) = self.should_stop.lock() {
            *stop = true;
        }

        // kill the child process
        if let Some(child_mutex) = &self.child_process
            && let Ok(mut child_opt) = child_mutex.lock()
            && let Some(child) = child_opt.as_mut()
        {
            let _ = child.start_kill();
        }

        // wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        // drain the log buffer and return decoded strings
        let mut buffer = self.log_buffer.lock().unwrap();
        let raw_logs: Vec<String> = buffer.drain(..).collect();

        if !raw_logs.is_empty() {
            log::debug!("IosLogProvider: Polled {} log lines", raw_logs.len());
        }

        Ok(raw_logs)
    }

    fn status(&self) -> Option<ProviderStatus> {
        Some(Self::current_status(&self.status))
    }
}

impl IosLogProvider {
    fn current_status(status: &Arc<Mutex<ProviderStatus>>) -> ProviderStatus {
        status.lock().map_or(
            ProviderStatus::Disconnected(ProviderDisconnectReason::Unknown),
            |status| *status,
        )
    }

    fn set_status(status: &Arc<Mutex<ProviderStatus>>, next: ProviderStatus) {
        if let Ok(mut current) = status.lock() {
            *current = next;
        }
    }

    fn should_stop(should_stop: &Arc<Mutex<bool>>) -> bool {
        should_stop.lock().is_ok_and(|stop| *stop)
    }

    fn push_line(log_buffer: &Arc<Mutex<Vec<String>>>, line: String) {
        let line = line.trim_end_matches('\r').to_string();

        if let Ok(mut buffer) = log_buffer.lock() {
            buffer.push(line);
        }
    }

    async fn run_log_source(
        source: IosLogSource,
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
        child_process: Arc<Mutex<Option<Child>>>,
        provider_status: Arc<Mutex<ProviderStatus>>,
    ) -> Result<()> {
        let (device, bundle_id) = match &source {
            IosLogSource::AppConsole { device, bundle_id } => {
                (device.to_string(), bundle_id.to_string())
            }
        };
        let initial_device = query_device_snapshot(&device).ok().flatten();
        let usb_was_attached = initial_device
            .as_ref()
            .and_then(|snapshot| snapshot.udid.as_deref())
            .and_then(usb_device_is_attached)
            .unwrap_or(false);
        let spec = source.command_spec();

        if Self::should_stop(&should_stop) {
            log::debug!("Stop signal received before iOS log connection");
            return Ok(());
        }

        log::debug!("Starting iOS log command: {}", spec.program);

        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // script(1) reads stdin and would otherwise race crossterm for the
        // user's terminal keystrokes, making the entire Lazylog UI feel laggy.
        if !spec.inherit_stdin {
            command.stdin(Stdio::null());
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                log::error!("Failed to spawn {}: {}", spec.program, e);
                return Err(e.into());
            }
        };

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("{} stdout was not piped", spec.program))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("{} stderr was not piped", spec.program))?;
        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();
        let mut stdout_open = true;
        let mut stderr_open = true;

        if let Ok(mut child_opt) = child_process.lock() {
            *child_opt = Some(child);
        }
        Self::set_status(&provider_status, ProviderStatus::Connected);

        while stdout_open || stderr_open {
            if Self::should_stop(&should_stop) {
                log::debug!("Stop signal received, exiting iOS log stream");
                break;
            }

            tokio::select! {
                result = stdout_lines.next_line(), if stdout_open => {
                    match result {
                        Ok(Some(line)) => Self::push_line(&log_buffer, line),
                        Ok(None) => stdout_open = false,
                        Err(error) => {
                            log::error!("Error reading {} stdout: {}", spec.program, error);
                            stdout_open = false;
                        }
                    }
                }
                result = stderr_lines.next_line(), if stderr_open => {
                    match result {
                        Ok(Some(line)) => Self::push_line(&log_buffer, line),
                        Ok(None) => stderr_open = false,
                        Err(error) => {
                            log::error!("Error reading {} stderr: {}", spec.program, error);
                            stderr_open = false;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }

        let child_to_finish = child_process.lock().ok().and_then(|mut child| child.take());
        let status = if let Some(mut child) = child_to_finish {
            if Self::should_stop(&should_stop) {
                let _ = child.start_kill();
            }
            Some(child.wait().await?)
        } else {
            None
        };

        if Self::should_stop(&should_stop) {
            return Ok(());
        }

        let reason = disconnect_reason(
            &device,
            &bundle_id,
            initial_device.as_ref(),
            usb_was_attached,
        );
        Self::set_status(&provider_status, ProviderStatus::Disconnected(reason));

        if status.is_some_and(|status| !status.success()) {
            bail!("devicectl app-console process exited with {status:?}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_device_prefers_a_connected_wired_iphone() {
        let devices = r#"
        {
          "result": {
            "devices": [
              {
                "identifier": "stale-device",
                "hardwareProperties": {"platform": "iOS", "reality": "physical"},
                "connectionProperties": {"pairingState": "paired"}
              },
              {
                "identifier": "network-device",
                "hardwareProperties": {"platform": "iOS", "reality": "physical"},
                "connectionProperties": {
                  "transportType": "localNetwork",
                  "lastConnectionDate": "2026-09-15T08:30:00.000Z"
                }
              },
              {
                "identifier": "wired-device",
                "hardwareProperties": {"platform": "iOS", "reality": "physical"},
                "connectionProperties": {
                  "transportType": "wired",
                  "lastConnectionDate": "2026-09-15T08:20:00.000Z"
                }
              }
            ]
          }
        }
        "#;

        assert_eq!(select_default_device(devices).unwrap(), "wired-device");
    }

    #[test]
    fn default_device_rejects_historical_pairings() {
        let devices = r#"
        {
          "result": {
            "devices": [{
              "identifier": "stale-device",
              "hardwareProperties": {"platform": "iOS", "reality": "physical"},
              "connectionProperties": {"pairingState": "paired"}
            }]
          }
        }
        "#;

        assert!(
            select_default_device(devices)
                .unwrap_err()
                .to_string()
                .contains("No connected physical iOS device")
        );
    }

    #[test]
    fn connected_devices_expose_picker_metadata_in_preferred_order() {
        let document: Value = serde_json::from_str(
            r#"{
              "result": {
                "devices": [{
                  "identifier": "network-device",
                  "deviceProperties": {"name": "Test iPhone"},
                  "hardwareProperties": {
                    "platform": "iOS",
                    "reality": "physical",
                    "marketingName": "iPhone 16"
                  },
                  "connectionProperties": {
                    "transportType": "localNetwork",
                    "lastConnectionDate": "2026-09-15T08:30:00.000Z"
                  }
                }, {
                  "identifier": "wired-device",
                  "deviceProperties": {"name": "Debug iPhone"},
                  "hardwareProperties": {
                    "platform": "iOS",
                    "reality": "physical",
                    "marketingName": "iPhone 17"
                  },
                  "connectionProperties": {
                    "transportType": "wired",
                    "lastConnectionDate": "2026-09-15T08:20:00.000Z"
                  }
                }]
              }
            }"#,
        )
        .unwrap();

        assert_eq!(
            connected_device_infos(&document).unwrap(),
            vec![
                IosDeviceInfo {
                    identifier: "wired-device".to_string(),
                    name: "Debug iPhone".to_string(),
                    model: "iPhone 17".to_string(),
                    transport: "wired".to_string(),
                },
                IosDeviceInfo {
                    identifier: "network-device".to_string(),
                    name: "Test iPhone".to_string(),
                    model: "iPhone 16".to_string(),
                    transport: "localNetwork".to_string(),
                },
            ]
        );
    }

    #[test]
    fn device_snapshot_preserves_connection_and_usb_identity() {
        let document: Value = serde_json::from_str(
            r#"{
              "result": {
                "devices": [{
                  "identifier": "device-123",
                  "hardwareProperties": {"udid": "00008150-001D554E0EDB401C"},
                  "connectionProperties": {"transportType": "wired"}
                }]
              }
            }"#,
        )
        .unwrap();

        assert_eq!(
            device_snapshot(&document, "device-123"),
            Some(DeviceSnapshot {
                connected: true,
                udid: Some("00008150-001D554E0EDB401C".to_string()),
            })
        );
    }

    #[test]
    fn disconnect_classifier_prefers_physical_usb_evidence() {
        assert_eq!(
            classify_disconnect(DisconnectEvidence {
                usb_was_attached: true,
                usb_is_attached: Some(false),
                device_connected: Some(true),
                app_running: Some(false),
            }),
            ProviderDisconnectReason::UsbDisconnected
        );
    }

    #[test]
    fn disconnect_classifier_distinguishes_device_app_and_capture() {
        assert_eq!(
            classify_disconnect(DisconnectEvidence {
                device_connected: Some(false),
                ..DisconnectEvidence::default()
            }),
            ProviderDisconnectReason::DeviceDisconnected
        );
        assert_eq!(
            classify_disconnect(DisconnectEvidence {
                device_connected: Some(true),
                app_running: Some(false),
                ..DisconnectEvidence::default()
            }),
            ProviderDisconnectReason::TargetExited
        );
        assert_eq!(
            classify_disconnect(DisconnectEvidence {
                device_connected: Some(true),
                app_running: Some(true),
                ..DisconnectEvidence::default()
            }),
            ProviderDisconnectReason::CaptureFailed
        );
        assert_eq!(
            classify_disconnect(DisconnectEvidence::default()),
            ProviderDisconnectReason::Unknown
        );
    }

    #[test]
    fn app_console_source_builds_attached_devicectl_launch_with_isolated_stdin() {
        let provider = IosLogProvider::new_app_console("device-123", "com.example.app");

        assert_eq!(
            provider.source.command_spec(),
            CommandSpec {
                program: "/usr/bin/script".to_string(),
                args: vec![
                    "-q",
                    "/dev/null",
                    "xcrun",
                    "devicectl",
                    "device",
                    "process",
                    "launch",
                    "--device",
                    "device-123",
                    "--terminate-existing",
                    "--console",
                    "com.example.app",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                inherit_stdin: false,
            }
        );
    }
}
