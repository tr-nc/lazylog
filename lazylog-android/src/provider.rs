use anyhow::{Result, anyhow};
use lazylog_framework::provider::{LogProvider, ProviderDisconnectReason, ProviderStatus};
use std::process::Command as StdCommand;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidDeviceInfo {
    pub serial: String,
    pub name: String,
    pub product: Option<String>,
}

fn parse_connected_devices(output: &str) -> Vec<AndroidDeviceInfo> {
    output
        .lines()
        .skip_while(|line| !line.starts_with("List of devices attached"))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            if fields.next()? != "device" {
                return None;
            }

            let mut model = None;
            let mut product = None;
            for field in fields {
                if let Some(value) = field.strip_prefix("model:") {
                    model = Some(value.replace('_', " "));
                } else if let Some(value) = field.strip_prefix("product:") {
                    product = Some(value.replace('_', " "));
                }
            }

            Some(AndroidDeviceInfo {
                serial: serial.to_string(),
                name: model.unwrap_or_else(|| "Android device".to_string()),
                product,
            })
        })
        .collect()
}

/// Return all Android devices that ADB currently reports as online.
pub fn connected_devices() -> Result<Vec<AndroidDeviceInfo>> {
    let output = StdCommand::new("adb").args(["devices", "-l"]).output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "adb devices failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(parse_connected_devices(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Return the first Android device in ADB's stable listing order.
pub fn default_device_serial() -> Result<String> {
    connected_devices()?
        .into_iter()
        .next()
        .map(|device| device.serial)
        .ok_or_else(|| anyhow!("No connected Android device was found"))
}

/// log provider for Android device logs (adb logcat)
pub struct AndroidLogProvider {
    device_serial: Option<String>,
    log_buffer: Arc<Mutex<Vec<String>>>,
    should_stop: Arc<Mutex<bool>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    child_process: Option<Arc<Mutex<Option<Child>>>>,
    status: Arc<Mutex<ProviderStatus>>,
}

impl AndroidLogProvider {
    pub fn new() -> Self {
        Self::with_device(None)
    }

    pub fn new_for_device(device_serial: impl Into<String>) -> Self {
        Self::with_device(Some(device_serial.into()))
    }

    fn with_device(device_serial: Option<String>) -> Self {
        Self {
            device_serial,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(Mutex::new(false)),
            thread_handle: None,
            child_process: None,
            status: Arc::new(Mutex::new(ProviderStatus::Connecting)),
        }
    }
}

impl Default for AndroidLogProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LogProvider for AndroidLogProvider {
    fn start(&mut self) -> Result<()> {
        log::debug!("AndroidLogProvider: Starting");

        if let Ok(mut stop) = self.should_stop.lock() {
            *stop = false;
        }
        Self::set_status(&self.status, ProviderStatus::Connecting);

        let log_buffer = self.log_buffer.clone();
        let should_stop = self.should_stop.clone();
        let child_process = Arc::new(Mutex::new(None));
        self.child_process = Some(child_process.clone());
        let provider_status = self.status.clone();
        let device_serial = self.device_serial.clone();

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
                match Self::run_adb_logcat(
                    device_serial,
                    log_buffer,
                    should_stop.clone(),
                    child_process,
                    provider_status.clone(),
                )
                .await
                {
                    Ok(_) => log::debug!("adb logcat stopped normally"),
                    Err(e) => {
                        log::error!("adb logcat error: {}", e);
                        if !Self::should_stop(&should_stop) {
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
        log::debug!("AndroidLogProvider: Stopping");

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
        // drain the log buffer and return strings
        let mut buffer = self.log_buffer.lock().unwrap();
        let raw_logs: Vec<String> = buffer.drain(..).collect();

        if !raw_logs.is_empty() {
            log::debug!("AndroidLogProvider: Polled {} log lines", raw_logs.len());
        }

        Ok(raw_logs)
    }

    fn status(&self) -> Option<ProviderStatus> {
        Some(Self::current_status(&self.status))
    }
}

// async helper function to spawn adb logcat command and stream logs
impl AndroidLogProvider {
    fn should_stop(should_stop: &Arc<Mutex<bool>>) -> bool {
        should_stop.lock().is_ok_and(|stop| *stop)
    }

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

    fn classify_disconnect(device_connected: bool) -> ProviderDisconnectReason {
        if device_connected {
            ProviderDisconnectReason::CaptureFailed
        } else {
            ProviderDisconnectReason::DeviceDisconnected
        }
    }

    fn adb_command(device_serial: Option<&str>) -> Command {
        let mut command = Command::new("adb");
        if let Some(device_serial) = device_serial {
            command.arg("-s").arg(device_serial);
        }
        command
    }

    async fn adb_device_is_connected(device_serial: Option<&str>) -> bool {
        let mut command = Self::adb_command(device_serial);
        command
            .arg("get-state")
            .kill_on_drop(true)
            .stderr(std::process::Stdio::null());

        tokio::time::timeout(Duration::from_secs(1), command.output())
            .await
            .is_ok_and(|result| {
                result.is_ok_and(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout).trim() == "device"
                })
            })
    }

    async fn sleep_interruptible(duration: std::time::Duration, should_stop: &Arc<Mutex<bool>>) {
        const CHECK_INTERVAL_MS: u64 = 25;
        let check_interval = std::time::Duration::from_millis(CHECK_INTERVAL_MS);

        if duration <= check_interval {
            tokio::time::sleep(duration).await;
            return;
        }

        let mut elapsed = std::time::Duration::ZERO;
        while elapsed < duration {
            if let Ok(stop) = should_stop.lock()
                && *stop
            {
                return;
            }
            let sleep_time = check_interval.min(duration - elapsed);
            tokio::time::sleep(sleep_time).await;
            elapsed += sleep_time;
        }
    }

    async fn clear_logcat_cache(device_serial: Option<&str>) -> Result<()> {
        log::debug!("Clearing adb logcat buffer before streaming...");

        let mut command = Self::adb_command(device_serial);
        command
            .arg("logcat")
            .arg("-c")
            .kill_on_drop(true)
            .stderr(std::process::Stdio::null());
        let status = tokio::time::timeout(Duration::from_secs(2), command.status())
            .await
            .map_err(|_| anyhow!("adb logcat -c timed out"))??;
        if status.success() {
            log::debug!("adb logcat buffer cleared");
            Ok(())
        } else {
            Err(anyhow!("adb logcat -c exited with status {}", status))
        }
    }

    async fn run_adb_logcat(
        device_serial: Option<String>,
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
        child_process: Arc<Mutex<Option<Child>>>,
        provider_status: Arc<Mutex<ProviderStatus>>,
    ) -> Result<()> {
        loop {
            // check if we should stop before attempting connection
            if let Ok(stop) = should_stop.lock()
                && *stop
            {
                log::debug!("Stop signal received before device connection");
                return Ok(());
            }

            log::debug!("Attempting to connect to Android device...");

            if !Self::adb_device_is_connected(device_serial.as_deref()).await {
                Self::set_status(
                    &provider_status,
                    ProviderStatus::Disconnected(ProviderDisconnectReason::DeviceDisconnected),
                );
                Self::sleep_interruptible(Duration::from_secs(1), &should_stop).await;
                continue;
            }

            Self::set_status(&provider_status, ProviderStatus::Connecting);

            if let Err(e) = Self::clear_logcat_cache(device_serial.as_deref()).await {
                log::warn!("Failed to clear adb log buffer: {}; retrying in 1s...", e);
                let reason = Self::classify_disconnect(
                    Self::adb_device_is_connected(device_serial.as_deref()).await,
                );
                Self::set_status(&provider_status, ProviderStatus::Disconnected(reason));
                Self::sleep_interruptible(std::time::Duration::from_secs(1), &should_stop).await;
                continue;
            }

            // spawn adb logcat command with '-v long' for detailed multi-line format
            let mut command = Self::adb_command(device_serial.as_deref());
            let mut child = match command
                .arg("logcat")
                .arg("-v")
                .arg("long")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    log::error!("Failed to spawn adb logcat: {}", e);
                    Self::set_status(
                        &provider_status,
                        ProviderStatus::Disconnected(ProviderDisconnectReason::CaptureFailed),
                    );
                    return Err(e.into());
                }
            };

            // check stderr for error messages
            let _stderr = child.stderr.take();
            let stdout = child.stdout.take();

            // wait briefly for the process to either start streaming or fail
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // check if process has exited (indicating no device found)
            match child.try_wait() {
                Ok(Some(status)) => {
                    // process exited - likely no device found
                    log::warn!(
                        "No Android device found (exit status: {}), retrying in 1s...",
                        status
                    );
                    let reason = Self::classify_disconnect(
                        Self::adb_device_is_connected(device_serial.as_deref()).await,
                    );
                    Self::set_status(&provider_status, ProviderStatus::Disconnected(reason));
                    Self::sleep_interruptible(std::time::Duration::from_secs(1), &should_stop)
                        .await;
                    continue;
                }
                Ok(None) => {
                    // process still running - device found!
                    log::debug!("Android device connected, streaming logs...");
                    Self::set_status(&provider_status, ProviderStatus::Connected);

                    let stdout = stdout.expect("Failed to get stdout");
                    let mut reader = BufReader::new(stdout).lines();

                    // store the child process handle
                    if let Ok(mut child_opt) = child_process.lock() {
                        *child_opt = Some(child);
                    }

                    // accumulator for multi-line log entries
                    let mut current_entry = Vec::new();
                    let mut last_device_check = Instant::now();

                    // stream logs continuously
                    loop {
                        // check if we should stop
                        if let Ok(stop) = should_stop.lock()
                            && *stop
                        {
                            log::debug!("Stop signal received, exiting adb logcat");
                            // flush any remaining entry
                            if !current_entry.is_empty()
                                && let Ok(mut buffer) = log_buffer.lock()
                            {
                                buffer.push(current_entry.join("\n"));
                            }
                            break;
                        }

                        // read next line with a timeout approach
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(100),
                            reader.next_line(),
                        )
                        .await
                        {
                            Ok(Ok(Some(log_line))) => {
                                if log_line.trim().is_empty() {
                                    // empty line - end of current log entry
                                    if !current_entry.is_empty() {
                                        if let Ok(mut buffer) = log_buffer.lock() {
                                            buffer.push(current_entry.join("\n"));
                                        }
                                        current_entry.clear();
                                    }
                                } else {
                                    // accumulate line
                                    current_entry.push(log_line);
                                }
                            }
                            Ok(Ok(None)) => {
                                log::debug!("adb logcat stream ended, device disconnected");
                                // flush any remaining entry
                                if !current_entry.is_empty()
                                    && let Ok(mut buffer) = log_buffer.lock()
                                {
                                    buffer.push(current_entry.join("\n"));
                                }
                                break;
                            }
                            Ok(Err(e)) => {
                                log::error!("Error reading log: {}", e);
                                break;
                            }
                            Err(_) => {
                                if last_device_check.elapsed() >= Duration::from_secs(1) {
                                    last_device_check = Instant::now();
                                    if !Self::adb_device_is_connected(device_serial.as_deref())
                                        .await
                                    {
                                        log::debug!("Android device disconnected while streaming");
                                        break;
                                    }
                                }
                                continue;
                            }
                        }
                    }

                    // clean up the child process
                    let child_to_kill = {
                        if let Ok(mut child_opt) = child_process.lock() {
                            child_opt.take()
                        } else {
                            None
                        }
                    };

                    if let Some(mut child) = child_to_kill {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }

                    if Self::should_stop(&should_stop) {
                        return Ok(());
                    }

                    let reason = Self::classify_disconnect(
                        Self::adb_device_is_connected(device_serial.as_deref()).await,
                    );
                    Self::set_status(&provider_status, ProviderStatus::Disconnected(reason));

                    // after device disconnects, retry connection
                    log::debug!("Retrying device connection...");
                    Self::sleep_interruptible(std::time::Duration::from_secs(1), &should_stop)
                        .await;
                    continue;
                }
                Err(e) => {
                    log::error!("Error checking process status: {}", e);
                    Self::set_status(
                        &provider_status,
                        ProviderStatus::Disconnected(ProviderDisconnectReason::CaptureFailed),
                    );
                    return Err(e.into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_classifier_uses_the_shared_status_vocabulary() {
        assert_eq!(
            AndroidLogProvider::classify_disconnect(false),
            ProviderDisconnectReason::DeviceDisconnected
        );
        assert_eq!(
            AndroidLogProvider::classify_disconnect(true),
            ProviderDisconnectReason::CaptureFailed
        );
    }

    #[test]
    fn device_parser_keeps_only_online_devices_and_their_names() {
        let output = "List of devices attached\n\
            R58M123 device product:beyond1qlte model:SM_G9730 device:beyond1q\n\
            offline-1 offline transport_id:2\n\
            unauthorized-1 unauthorized usb:1-2\n\
            emulator-5554 device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64\n";

        assert_eq!(
            parse_connected_devices(output),
            vec![
                AndroidDeviceInfo {
                    serial: "R58M123".to_string(),
                    name: "SM G9730".to_string(),
                    product: Some("beyond1qlte".to_string()),
                },
                AndroidDeviceInfo {
                    serial: "emulator-5554".to_string(),
                    name: "sdk gphone64 arm64".to_string(),
                    product: Some("sdk gphone64 arm64".to_string()),
                },
            ]
        );
    }
}
