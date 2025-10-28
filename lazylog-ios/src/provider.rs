use anyhow::Result;
use lazylog_framework::provider::{LogItem, LogProvider};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::runtime::Runtime;

/// log provider for iOS device logs (syslog relay)
pub struct IosLogProvider {
    runtime: Option<Runtime>,
    log_buffer: Arc<Mutex<Vec<String>>>,
    should_stop: Arc<Mutex<bool>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl IosLogProvider {
    pub fn new() -> Self {
        Self {
            runtime: None,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(Mutex::new(false)),
            thread_handle: None,
        }
    }

    fn parse_ios_log(raw_log: &str) -> LogItem {
        // iOS log format: "Oct 27 16:10:13 deviceName processName[pid] <Level>: content"
        let parts: Vec<&str> = raw_log.splitn(6, ' ').collect();

        if parts.len() < 6 {
            // malformed log, return as-is
            return LogItem::new(raw_log.to_string(), raw_log.to_string());
        }

        // process name (strip [pid] or (subsystem) if present)
        let tag = parts[4]
            .split('[')
            .next()
            .and_then(|s| s.split('(').next())
            .unwrap_or(parts[4])
            .to_string();

        // level and content
        let level_and_content = parts[5];
        let (level, content) = if let Some(start) = level_and_content.find('<') {
            if let Some(end) = level_and_content.find(">:") {
                let level = &level_and_content[start + 1..end];
                let content = &level_and_content[end + 2..];
                (level.to_string(), content.trim().to_string())
            } else {
                (String::new(), level_and_content.to_string())
            }
        } else {
            (String::new(), level_and_content.to_string())
        };

        let mut item = LogItem::new(content, raw_log.to_string());
        if !level.is_empty() {
            item = item.with_metadata("level", level);
        }
        if !tag.is_empty() {
            item = item.with_metadata("tag", tag);
        }
        item
    }
}

impl Default for IosLogProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LogProvider for IosLogProvider {
    fn start(&mut self) -> Result<()> {
        log::debug!("IosLogProvider: Starting");

        // create a tokio runtime for async operations
        let runtime = Runtime::new()?;
        self.runtime = Some(runtime);

        let log_buffer = self.log_buffer.clone();
        let should_stop = self.should_stop.clone();

        // spawn a thread to run the async syslog reader
        let handle = thread::spawn(move || {
            // we need a tokio runtime in this thread
            let rt = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("Failed to create tokio runtime: {}", e);
                    return;
                }
            };

            rt.block_on(async {
                match Self::run_syslog_relay(log_buffer, should_stop).await {
                    Ok(_) => log::debug!("Syslog relay stopped normally"),
                    Err(e) => log::error!("Syslog relay error: {}", e),
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

        // wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<LogItem>> {
        // drain the log buffer and parse into LogItems
        let mut buffer = self.log_buffer.lock().unwrap();
        let raw_logs: Vec<String> = buffer.drain(..).collect();

        let log_items: Vec<LogItem> = raw_logs
            .iter()
            .map(|log| Self::parse_ios_log(log))
            .collect();

        if !log_items.is_empty() {
            log::debug!("IosLogProvider: Polled {} log items", log_items.len());
        }

        Ok(log_items)
    }
}

// async helper function to connect to iOS device and stream logs
impl IosLogProvider {
    async fn run_syslog_relay(
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
    ) -> Result<()> {
        use idevice::{
            IdeviceService,
            syslog_relay::SyslogRelayClient,
            usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
        };

        log::debug!("Connecting to usbmuxd...");

        let addr = UsbmuxdAddr::default();
        let mut conn = UsbmuxdConnection::default().await?;

        // get all connected iOS devices
        let devices = conn.get_devices().await?;

        if devices.is_empty() {
            log::error!("No iOS devices found");
            return Err(anyhow::anyhow!("No iOS devices connected"));
        }

        // use the first device
        let device = &devices[0];
        log::debug!("Connected to device: {}", device.udid);

        // create provider for the device
        let provider = device.to_provider(addr, "lazylog-ios");

        // connect to syslog relay service
        let mut syslog = SyslogRelayClient::connect(&provider).await?;

        log::debug!("Syslog relay connected, streaming logs...");

        // stream logs continuously
        loop {
            // check if we should stop
            if let Ok(stop) = should_stop.lock()
                && *stop
            {
                log::debug!("Stop signal received, exiting syslog relay");
                break;
            }

            match syslog.next().await {
                Ok(log_line) => {
                    // push to buffer
                    if let Ok(mut buffer) = log_buffer.lock() {
                        buffer.push(log_line);
                    }
                }
                Err(e) => {
                    log::error!("Error reading log: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
