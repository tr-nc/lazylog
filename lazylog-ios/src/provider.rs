use anyhow::Result;
use lazylog_framework::provider::{LogItem, LogProvider};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;

use crate::decoder::decode_syslog;

/// log provider for iOS device logs (syslog relay)
pub struct IosLogProvider {
    log_buffer: Arc<Mutex<Vec<String>>>,
    should_stop: Arc<Mutex<bool>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    child_process: Option<Arc<Mutex<Option<Child>>>>,
}

impl IosLogProvider {
    pub fn new() -> Self {
        Self {
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(Mutex::new(false)),
            thread_handle: None,
            child_process: None,
        }
    }

    fn parse_ios_log(raw_log: &str) -> LogItem {
        // iOS log format: "Oct 27 16:10:13 deviceName processName[pid] <Level>: content"
        // or: "Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: content"
        let parts: Vec<&str> = raw_log.splitn(5, ' ').collect();

        if parts.len() < 5 {
            // malformed log, return as-is
            return LogItem::new(raw_log.to_string(), raw_log.to_string());
        }

        // extract tag: the 4th item (index 3), process it to only leave the name before [ or (
        let tag = parts[3]
            .split('[')
            .next()
            .and_then(|s| s.split('(').next())
            .unwrap_or(parts[3])
            .to_string();

        // level and content from the 5th item onwards
        let level_and_content = parts[4];
        let (level, content) = if let Some(start) = level_and_content.find('<') {
            if let Some(end) = level_and_content.find(">:") {
                // extract level without angle brackets
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

        let log_buffer = self.log_buffer.clone();
        let should_stop = self.should_stop.clone();
        let child_process = Arc::new(Mutex::new(None));
        self.child_process = Some(child_process.clone());

        // spawn a thread to run the command-line tool
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
                match Self::run_syslog_relay(log_buffer, should_stop, child_process).await {
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

        // kill the child process
        if let Some(child_mutex) = &self.child_process {
            if let Ok(mut child_opt) = child_mutex.lock() {
                if let Some(child) = child_opt.as_mut() {
                    let _ = child.start_kill();
                }
            }
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

// async helper function to spawn idevicesyslog command and stream logs
impl IosLogProvider {
    async fn run_syslog_relay(
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
        child_process: Arc<Mutex<Option<Child>>>,
    ) -> Result<()> {
        log::debug!("Spawning idevicesyslog command...");

        // spawn idevicesyslog command
        let mut child = Command::new("idevicesyslog")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("Failed to get stdout");
        let mut reader = BufReader::new(stdout).lines();

        // store the child process handle
        if let Ok(mut child_opt) = child_process.lock() {
            *child_opt = Some(child);
        }

        log::debug!("Syslog relay connected, streaming logs...");

        // stream logs continuously
        loop {
            // check if we should stop
            if let Ok(stop) = should_stop.lock() {
                if *stop {
                    log::debug!("Stop signal received, exiting syslog relay");
                    break;
                }
            }

            // read next line with a timeout approach
            match tokio::time::timeout(std::time::Duration::from_millis(100), reader.next_line())
                .await
            {
                Ok(Ok(Some(log_line))) => {
                    // decode the vis-encoded syslog line first
                    // the trim_matches is to remove the null byte at the beginning, not sure why it's there
                    let decoded_log = decode_syslog(&log_line);

                    // push to buffer
                    if let Ok(mut buffer) = log_buffer.lock() {
                        buffer.push(decoded_log);
                    }
                }
                Ok(Ok(None)) => {
                    log::debug!("idevicesyslog stream ended");
                    break;
                }
                Ok(Err(e)) => {
                    log::error!("Error reading log: {}", e);
                    break;
                }
                Err(_) => {
                    // timeout - just continue to check should_stop
                    continue;
                }
            }
        }

        // clean up the child process
        if let Ok(mut child_opt) = child_process.lock() {
            if let Some(mut child) = child_opt.take() {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }

        Ok(())
    }
}
