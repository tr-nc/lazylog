use anyhow::{Result, bail};
use lazylog_framework::provider::{LogProvider, ProviderDisconnectReason, ProviderStatus};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;

const DEVICECTL_QUERY_TIMEOUT_SECONDS: &str = "5";
const DIAGNOSTIC_TAIL_LINES: usize = 20;

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
            let is_ios = device
                .pointer("/hardwareProperties/platform")
                .and_then(Value::as_str)
                == Some("iOS");
            let has_active_transport = device
                .pointer("/connectionProperties/transportType")
                .and_then(Value::as_str)
                .is_some();
            let is_physical = match device
                .pointer("/hardwareProperties/reality")
                .and_then(Value::as_str)
            {
                Some("physical") => true,
                Some(_) => false,
                // FYI: CoreDevice can omit `reality` for older iOS devices
                // (observed on iOS 18.6.2). A hardware UDID is the fallback
                // physical-device evidence; explicit nonphysical values above
                // are still rejected.
                None => device
                    .pointer("/hardwareProperties/udid")
                    .and_then(Value::as_str)
                    .is_some_and(|udid| !udid.is_empty()),
            };

            is_ios && has_active_transport && is_physical
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

fn devicectl_json_command(args: &[&str], output_path: &Path) -> StdCommand {
    let mut command = StdCommand::new("xcrun");
    command
        .arg("devicectl")
        .args(args)
        .args(["--timeout", DEVICECTL_QUERY_TIMEOUT_SECONDS])
        .arg("--json-output")
        .arg(output_path)
        .arg("--quiet");
    command
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
        let output = devicectl_json_command(args, &output_path).output()?;
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
    ProcessPresent,
    NoProcess,
    NotInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IosAppAvailability {
    Installed,
    NotInstalled,
}

fn installed_app_url<'a>(document: &'a Value, bundle_id: &str) -> Option<&'a str> {
    document
        .pointer("/result/apps")
        .and_then(Value::as_array)
        .and_then(|apps| {
            apps.iter()
                .find(|app| app.get("bundleIdentifier").and_then(Value::as_str) == Some(bundle_id))
        })
        .and_then(|app| app.get("url"))
        .and_then(Value::as_str)
}

fn app_availabilities_from_document(
    apps: &Value,
    bundle_ids: &[&str],
) -> Result<Vec<IosAppAvailability>> {
    apps.pointer("/result/apps")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("devicectl returned no installed App list"))?;

    Ok(bundle_ids
        .iter()
        .map(|bundle_id| {
            if installed_app_url(apps, bundle_id).is_some() {
                IosAppAvailability::Installed
            } else {
                IosAppAvailability::NotInstalled
            }
        })
        .collect())
}

/// Report installation availability for multiple Apps.
pub fn app_availabilities(device: &str, bundle_ids: &[&str]) -> Result<Vec<IosAppAvailability>> {
    let apps = run_devicectl_json(
        &["device", "info", "apps", "--device", device],
        "app-availability",
    )?;
    app_availabilities_from_document(&apps, bundle_ids)
}

fn app_states_from_documents(
    apps: &Value,
    processes: &Value,
    bundle_ids: &[&str],
) -> Result<Vec<IosAppState>> {
    apps.pointer("/result/apps")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("devicectl returned no installed App list"))?;
    let processes = processes
        .pointer("/result/runningProcesses")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("devicectl returned no process list"))?;

    Ok(bundle_ids
        .iter()
        .map(|bundle_id| {
            let Some(app_url) = installed_app_url(apps, bundle_id) else {
                return IosAppState::NotInstalled;
            };
            let main_process_present = processes.iter().any(|process| {
                process
                    .get("executable")
                    .and_then(Value::as_str)
                    .and_then(|executable| executable.strip_prefix(app_url))
                    .is_some_and(|relative_path| {
                        !relative_path.is_empty() && !relative_path.contains('/')
                    })
            });
            if main_process_present {
                IosAppState::ProcessPresent
            } else {
                IosAppState::NoProcess
            }
        })
        .collect())
}

/// Report whether an installed App currently has a process on the device.
///
/// `ProcessPresent` includes foreground, background, and suspended processes.
/// It does not imply that the App is visible or that Lazylog launched it.
pub fn app_state(device: &str, bundle_id: &str) -> Result<IosAppState> {
    app_states(device, &[bundle_id])?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No bundle ID was provided"))
}

/// Report process presence for multiple Apps using one coherent device snapshot.
pub fn app_states(device: &str, bundle_ids: &[&str]) -> Result<Vec<IosAppState>> {
    let apps = run_devicectl_json(
        &["device", "info", "apps", "--device", device],
        "app-status",
    )?;
    let processes = run_devicectl_json(
        &["device", "info", "processes", "--device", device],
        "process-status",
    )?;
    app_states_from_documents(&apps, &processes, bundle_ids)
}

fn validate_control_snapshot(document: &Value) -> Result<()> {
    document
        .pointer("/result/runningProcesses")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("devicectl returned no running process snapshot"))?;
    Ok(())
}

/// Verify that a detected device also accepts a bounded, read-only CoreDevice
/// request before Lazylog changes the target App's process state.
pub fn ensure_device_control_ready(device: &str) -> Result<()> {
    let result = (|| {
        let processes = run_devicectl_json(
            &["device", "info", "processes", "--device", device],
            "control-readiness",
        )?;
        validate_control_snapshot(&processes)
    })();

    result.map_err(|error| {
        anyhow::anyhow!(
            "iOS device '{device}' was detected, but its devicectl control channel is unavailable: \
             {error}. Close any other devicectl console or stale Lazylog session, then retry"
        )
    })
}

fn query_app_running(device: &str, bundle_id: &str) -> Result<bool> {
    Ok(matches!(
        app_state(device, bundle_id)?,
        IosAppState::ProcessPresent
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
    DeviceSyslog { device: String, bundle_id: String },
}

#[derive(Debug, PartialEq, Eq)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
    stdio: CommandStdio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandStdio {
    Pipes,
    PtyStdout,
}

struct SpawnedLogCommand {
    child: Child,
    stdout: Box<dyn AsyncRead + Unpin + Send>,
    stderr: Box<dyn AsyncRead + Unpin + Send>,
}

struct PtyPair {
    master: File,
    slave: File,
}

fn set_close_on_exec(file: &File) -> Result<()> {
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    if result == -1 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

fn open_pty() -> Result<PtyPair> {
    let mut master_fd = -1;
    let mut slave_fd = -1;
    let result = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error().into());
    }

    let master = File::from(unsafe { OwnedFd::from_raw_fd(master_fd) });
    let slave = File::from(unsafe { OwnedFd::from_raw_fd(slave_fd) });
    set_close_on_exec(&master)?;
    set_close_on_exec(&slave)?;
    Ok(PtyPair { master, slave })
}

fn spawn_log_command(spec: &CommandSpec) -> Result<SpawnedLogCommand> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).stderr(Stdio::piped());

    let pty_master = match spec.stdio {
        CommandStdio::Pipes => {
            command.stdin(Stdio::null()).stdout(Stdio::piped());
            None
        }
        CommandStdio::PtyStdout => {
            let pty = open_pty()?;
            command
                .stdin(Stdio::from(pty.slave.try_clone()?))
                .stdout(Stdio::from(pty.slave));
            Some(pty.master)
        }
    };

    let mut child = command.spawn()?;
    let stdout: Box<dyn AsyncRead + Unpin + Send> = match pty_master {
        Some(master) => Box::new(tokio::fs::File::from_std(master)),
        None => Box::new(
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("{} stdout was not piped", spec.program))?,
        ),
    };
    let stderr = Box::new(
        child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("{} stderr was not piped", spec.program))?,
    );

    Ok(SpawnedLogCommand {
        child,
        stdout,
        stderr,
    })
}

impl IosLogSource {
    fn device_and_bundle_id(&self) -> (&str, &str) {
        match self {
            Self::AppConsole { device, bundle_id } | Self::DeviceSyslog { device, bundle_id } => {
                (device, bundle_id)
            }
        }
    }

    fn control_command_spec(&self) -> Option<CommandSpec> {
        match self {
            Self::AppConsole { .. } => None,
            Self::DeviceSyslog { device, bundle_id } => Some(CommandSpec {
                program: "xcrun".to_string(),
                args: vec![
                    "devicectl".to_string(),
                    "device".to_string(),
                    "process".to_string(),
                    "launch".to_string(),
                    "--device".to_string(),
                    device.clone(),
                    "--terminate-existing".to_string(),
                    bundle_id.clone(),
                ],
                stdio: CommandStdio::Pipes,
            }),
        }
    }

    fn log_command_spec(&self, udid: Option<&str>) -> Result<CommandSpec> {
        match self {
            Self::AppConsole { device, bundle_id } => Ok(CommandSpec {
                // devicectl only forwards the launched app's console output
                // when stdin/stdout are attached to a terminal. Lazylog owns
                // that PTY so devicectl stderr remains a separate diagnostics
                // channel instead of duplicating App records into the stream.
                program: "xcrun".to_string(),
                args: vec![
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
                stdio: CommandStdio::PtyStdout,
            }),
            Self::DeviceSyslog { .. } => {
                let udid = udid.ok_or_else(|| {
                    anyhow::anyhow!(
                        "devicectl did not report the selected device UDID required by idevicesyslog"
                    )
                })?;
                Ok(CommandSpec {
                    program: "idevicesyslog".to_string(),
                    args: vec!["-u".to_string(), udid.to_string()],
                    stdio: CommandStdio::Pipes,
                })
            }
        }
    }

    fn monitors_target_app(&self) -> bool {
        matches!(self, Self::DeviceSyslog { .. })
    }

    fn stderr_is_log_data(&self) -> bool {
        matches!(self, Self::DeviceSyslog { .. })
    }
}

/// Log provider for iOS device logs.
///
/// Launches one installed app through Apple's devicectl and captures its
/// canonical stdout console stream.
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

    /// Launch the target App with devicectl, then capture the selected device's
    /// syslog with idevicesyslog.
    pub fn new_device_syslog(device: impl Into<String>, bundle_id: impl Into<String>) -> Self {
        Self::with_source(IosLogSource::DeviceSyslog {
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

    fn push_diagnostic(tail: &mut VecDeque<String>, line: String) {
        if tail.len() == DIAGNOSTIC_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line.trim_end_matches('\r').to_string());
    }

    async fn run_control_command(spec: &CommandSpec) -> Result<()> {
        let output = Command::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        if !output.status.success() {
            bail!(
                "iOS control command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    async fn run_log_source(
        source: IosLogSource,
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
        child_process: Arc<Mutex<Option<Child>>>,
        provider_status: Arc<Mutex<ProviderStatus>>,
    ) -> Result<()> {
        let (device, bundle_id) = source.device_and_bundle_id();
        let device = device.to_string();
        let bundle_id = bundle_id.to_string();
        let initial_device = query_device_snapshot(&device).ok().flatten();
        let usb_was_attached = initial_device
            .as_ref()
            .and_then(|snapshot| snapshot.udid.as_deref())
            .and_then(usb_device_is_attached)
            .unwrap_or(false);
        let spec = source.log_command_spec(
            initial_device
                .as_ref()
                .and_then(|snapshot| snapshot.udid.as_deref()),
        )?;

        if Self::should_stop(&should_stop) {
            log::debug!("Stop signal received before iOS log connection");
            return Ok(());
        }

        log::debug!("Starting iOS log command: {}", spec.program);

        let spawned = match spawn_log_command(&spec) {
            Ok(spawned) => spawned,
            Err(e) => {
                log::error!("Failed to spawn {}: {}", spec.program, e);
                return Err(e);
            }
        };
        let mut stdout_lines = BufReader::new(spawned.stdout).lines();
        let mut stderr_lines = BufReader::new(spawned.stderr).lines();
        let mut stdout_open = true;
        let mut stderr_open = true;

        if let Ok(mut child_opt) = child_process.lock() {
            *child_opt = Some(spawned.child);
        }

        if let Some(control_spec) = source.control_command_spec() {
            log::debug!("Starting iOS control command: {}", control_spec.program);
            if let Err(error) = Self::run_control_command(&control_spec).await {
                let child_to_stop = child_process.lock().ok().and_then(|mut child| child.take());
                if let Some(mut child) = child_to_stop {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
                return Err(error);
            }
        }

        Self::set_status(&provider_status, ProviderStatus::Connected);
        let mut next_target_check = Instant::now() + Duration::from_secs(1);
        let mut target_exited = false;
        let mut diagnostic_tail = VecDeque::new();

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
                        Ok(Some(line)) if source.stderr_is_log_data() => {
                            Self::push_line(&log_buffer, line)
                        }
                        Ok(Some(line)) => Self::push_diagnostic(&mut diagnostic_tail, line),
                        Ok(None) => stderr_open = false,
                        Err(error) => {
                            log::error!("Error reading {} stderr: {}", spec.program, error);
                            stderr_open = false;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if source.monitors_target_app() && Instant::now() >= next_target_check {
                        next_target_check = Instant::now() + Duration::from_secs(1);
                        if matches!(query_app_running(&device, &bundle_id), Ok(false)) {
                            target_exited = true;
                            break;
                        }
                    }
                }
            }
        }

        let child_to_finish = child_process.lock().ok().and_then(|mut child| child.take());
        let status = if let Some(mut child) = child_to_finish {
            if Self::should_stop(&should_stop) || target_exited {
                let _ = child.start_kill();
            }
            Some(child.wait().await?)
        } else {
            None
        };

        if Self::should_stop(&should_stop) {
            return Ok(());
        }

        let reason = if target_exited {
            ProviderDisconnectReason::TargetExited
        } else {
            disconnect_reason(
                &device,
                &bundle_id,
                initial_device.as_ref(),
                usb_was_attached,
            )
        };
        Self::set_status(&provider_status, ProviderStatus::Disconnected(reason));

        if status.is_some_and(|status| !status.success()) {
            let diagnostics = diagnostic_tail
                .into_iter()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if diagnostics.is_empty() {
                bail!("{} process exited with {status:?}", spec.program);
            }
            bail!(
                "{} process exited with {status:?}:\n{diagnostics}",
                spec.program
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devicectl_json_queries_have_a_bounded_timeout() {
        let command = devicectl_json_command(
            &["device", "info", "apps", "--device", "device-123"],
            Path::new("/tmp/lazylog-test.json"),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert!(
            args.windows(2)
                .any(|pair| pair == ["--timeout", DEVICECTL_QUERY_TIMEOUT_SECONDS])
        );
    }

    #[test]
    fn control_readiness_accepts_an_empty_process_snapshot() {
        let document = serde_json::json!({
            "result": {"runningProcesses": []}
        });

        assert!(validate_control_snapshot(&document).is_ok());
    }

    #[test]
    fn control_readiness_rejects_a_missing_process_snapshot() {
        let document = serde_json::json!({"result": {}});

        assert!(
            validate_control_snapshot(&document)
                .unwrap_err()
                .to_string()
                .contains("running process snapshot")
        );
    }

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
    fn connected_devices_accept_older_ios_without_reality_when_hardware_udid_is_present() {
        let document = serde_json::json!({
            "result": {
                "devices": [{
                    "identifier": "older-ios-device",
                    "deviceProperties": {"name": "Older iPhone"},
                    "hardwareProperties": {
                        "platform": "iOS",
                        "udid": "00008120-001E316002A0C01E",
                        "marketingName": "iPhone 14 Pro"
                    },
                    "connectionProperties": {
                        "transportType": "wired",
                        "pairingState": "paired"
                    }
                }]
            }
        });

        assert_eq!(
            connected_device_infos(&document).unwrap(),
            vec![IosDeviceInfo {
                identifier: "older-ios-device".to_string(),
                name: "Older iPhone".to_string(),
                model: "iPhone 14 Pro".to_string(),
                transport: "wired".to_string(),
            }]
        );
    }

    #[test]
    fn connected_devices_do_not_infer_physical_without_a_hardware_udid() {
        let document = serde_json::json!({
            "result": {
                "devices": [{
                    "identifier": "ambiguous-device",
                    "hardwareProperties": {"platform": "iOS"},
                    "connectionProperties": {"transportType": "wired"}
                }]
            }
        });

        assert!(connected_device_infos(&document).unwrap().is_empty());
    }

    #[test]
    fn connected_devices_reject_explicit_nonphysical_reality() {
        let document = serde_json::json!({
            "result": {
                "devices": [{
                    "identifier": "simulated-device",
                    "hardwareProperties": {
                        "platform": "iOS",
                        "reality": "simulated",
                        "udid": "simulated-udid"
                    },
                    "connectionProperties": {"transportType": "wired"}
                }]
            }
        });

        assert!(connected_device_infos(&document).unwrap().is_empty());
    }

    #[test]
    fn empty_app_result_is_recognized_as_not_installed() {
        let apps: Value =
            serde_json::from_str(r#"{"info":{"outcome":"success"},"result":{"apps":[]}}"#).unwrap();
        let processes: Value = serde_json::from_str(
            r#"{"info":{"outcome":"success"},"result":{"runningProcesses":[]}}"#,
        )
        .unwrap();

        assert_eq!(
            app_states_from_documents(&apps, &processes, &["com.example.missing"]).unwrap(),
            vec![IosAppState::NotInstalled]
        );
    }

    #[test]
    fn empty_app_result_reports_not_installed_availability() {
        let apps: Value =
            serde_json::from_str(r#"{"info":{"outcome":"success"},"result":{"apps":[]}}"#).unwrap();

        assert_eq!(
            app_availabilities_from_document(&apps, &["com.example.missing"]).unwrap(),
            vec![IosAppAvailability::NotInstalled]
        );
    }

    #[test]
    fn app_availability_matches_exact_bundle_identifiers() {
        let apps: Value = serde_json::from_str(
            r#"{
              "result": {"apps": [{
                "bundleIdentifier": "com.example.one",
                "url": "file:///apps/One.app/"
              }, {
                "bundleIdentifier": "com.example.one.widget",
                "url": "file:///apps/OneWidget.app/"
              }]}
            }"#,
        )
        .unwrap();

        assert_eq!(
            app_availabilities_from_document(
                &apps,
                &[
                    "com.example.one",
                    "com.example.one.widget",
                    "com.example.two"
                ]
            )
            .unwrap(),
            vec![
                IosAppAvailability::Installed,
                IosAppAvailability::Installed,
                IosAppAvailability::NotInstalled,
            ]
        );
    }

    #[test]
    fn malformed_app_result_is_a_detection_failure() {
        let apps: Value = serde_json::from_str(r#"{"info":{"outcome":"success"}}"#).unwrap();

        assert!(app_availabilities_from_document(&apps, &["com.example.one"]).is_err());
    }

    #[test]
    fn one_process_snapshot_distinguishes_main_and_extension_processes() {
        let apps: Value = serde_json::from_str(
            r#"{
              "result": {"apps": [{
                "bundleIdentifier": "com.example.one",
                "url": "file:///apps/One.app/"
              }, {
                "bundleIdentifier": "com.example.two",
                "url": "file:///apps/Two.app/"
              }]}
            }"#,
        )
        .unwrap();
        let processes: Value = serde_json::from_str(
            r#"{
              "result": {"runningProcesses": [{
                "executable": "file:///apps/One.app/PlugIns/Widget.appex/Widget"
              }, {
                "executable": "file:///apps/Two.app/Two"
              }]}
            }"#,
        )
        .unwrap();

        assert_eq!(
            app_states_from_documents(&apps, &processes, &["com.example.one", "com.example.two"])
                .unwrap(),
            vec![IosAppState::NoProcess, IosAppState::ProcessPresent]
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
    fn app_console_source_builds_attached_devicectl_launch_with_split_channels() {
        let provider = IosLogProvider::new_app_console("device-123", "com.example.app");

        assert_eq!(
            provider.source.log_command_spec(None).unwrap(),
            CommandSpec {
                program: "xcrun".to_string(),
                args: vec![
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
                stdio: CommandStdio::PtyStdout,
            }
        );
        assert!(!provider.source.stderr_is_log_data());
        assert_eq!(provider.source.control_command_spec(), None);
    }

    #[test]
    fn device_syslog_source_keeps_devicectl_control_and_switches_only_log_capture() {
        let provider = IosLogProvider::new_device_syslog("device-123", "com.example.app");

        assert_eq!(
            provider.source.control_command_spec(),
            Some(CommandSpec {
                program: "xcrun".to_string(),
                args: vec![
                    "devicectl",
                    "device",
                    "process",
                    "launch",
                    "--device",
                    "device-123",
                    "--terminate-existing",
                    "com.example.app",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                stdio: CommandStdio::Pipes,
            })
        );
        assert_eq!(
            provider
                .source
                .log_command_spec(Some("device-udid"))
                .unwrap(),
            CommandSpec {
                program: "idevicesyslog".to_string(),
                args: vec!["-u".to_string(), "device-udid".to_string()],
                stdio: CommandStdio::Pipes,
            }
        );
        assert!(provider.source.stderr_is_log_data());
    }

    #[test]
    fn device_syslog_source_requires_devicectl_udid_evidence() {
        let provider = IosLogProvider::new_device_syslog("device-123", "com.example.app");

        assert!(
            provider
                .source
                .log_command_spec(None)
                .unwrap_err()
                .to_string()
                .contains("device UDID")
        );
    }

    #[test]
    fn app_console_preserves_identical_records_received_on_stdout() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let record = "## 2026-09-16 14:20:01 [effect] INFO ## [TAG] same".to_string();

        IosLogProvider::push_line(&buffer, record.clone());
        IosLogProvider::push_line(&buffer, record.clone());

        assert_eq!(*buffer.lock().unwrap(), vec![record.clone(), record]);
    }

    #[tokio::test]
    async fn pty_stdout_keeps_child_stderr_separate() {
        use tokio::io::AsyncReadExt;

        let spec = CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'stdout-only\\n'; printf 'stderr-only\\n' >&2".to_string(),
            ],
            stdio: CommandStdio::PtyStdout,
        };
        let SpawnedLogCommand {
            mut child,
            mut stdout,
            mut stderr,
        } = spawn_log_command(&spec).unwrap();
        let mut stdout_text = String::new();
        let mut stderr_text = String::new();
        let (stdout_result, stderr_result, status_result) = tokio::join!(
            stdout.read_to_string(&mut stdout_text),
            stderr.read_to_string(&mut stderr_text),
            child.wait(),
        );

        stdout_result.unwrap();
        stderr_result.unwrap();
        assert!(status_result.unwrap().success());
        assert!(stdout_text.contains("stdout-only"));
        assert!(!stdout_text.contains("stderr-only"));
        assert_eq!(stderr_text, "stderr-only\n");
    }
}
