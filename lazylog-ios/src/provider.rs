use anyhow::Result;
use lazylog_framework::provider::LogProvider;
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

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        // drain the log buffer and return decoded strings
        let mut buffer = self.log_buffer.lock().unwrap();
        let raw_logs: Vec<String> = buffer.drain(..).collect();

        if !raw_logs.is_empty() {
            log::debug!("IosLogProvider: Polled {} log lines", raw_logs.len());
        }

        Ok(raw_logs)
    }
}

// async helper function to spawn idevicesyslog command and stream logs
impl IosLogProvider {
    async fn run_syslog_relay(
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
        child_process: Arc<Mutex<Option<Child>>>,
    ) -> Result<()> {
        loop {
            // check if we should stop before attempting connection
            if let Ok(stop) = should_stop.lock() {
                if *stop {
                    log::debug!("Stop signal received before device connection");
                    return Ok(());
                }
            }

            log::debug!("Attempting to connect to iOS device...");

            // spawn idevicesyslog command
            let mut child = match Command::new("idevicesyslog")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    log::error!("Failed to spawn idevicesyslog: {}", e);
                    return Err(e.into());
                }
            };

            // check stderr for "No device found" message
            let _stderr = child.stderr.take();
            let stdout = child.stdout.take();

            // wait briefly for the process to either start streaming or fail
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // check if process has exited (indicating no device found)
            match child.try_wait() {
                Ok(Some(status)) => {
                    // process exited - likely no device found
                    log::warn!(
                        "No iOS device found (exit status: {}), retrying in 1s...",
                        status
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                Ok(None) => {
                    // process still running - device found!
                    log::debug!("iOS device connected, streaming logs...");

                    let stdout = stdout.expect("Failed to get stdout");
                    let mut reader = BufReader::new(stdout).lines();

                    // store the child process handle
                    if let Ok(mut child_opt) = child_process.lock() {
                        *child_opt = Some(child);
                    }

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
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(100),
                            reader.next_line(),
                        )
                        .await
                        {
                            Ok(Ok(Some(log_line))) => {
                                // decode the vis-encoded syslog line first
                                let decoded_log = decode_syslog(&log_line);

                                // push to buffer
                                if let Ok(mut buffer) = log_buffer.lock() {
                                    buffer.push(decoded_log);
                                }
                            }
                            Ok(Ok(None)) => {
                                log::debug!("idevicesyslog stream ended, device disconnected");
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

                    // after device disconnects, retry connection
                    log::debug!("Retrying device connection...");
                    continue;
                }
                Err(e) => {
                    log::error!("Error checking process status: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
}
