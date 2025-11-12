use anyhow::Result;
use lazylog_framework::provider::LogProvider;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;

/// log provider for Android device logs (adb logcat)
pub struct AndroidLogProvider {
    log_buffer: Arc<Mutex<Vec<String>>>,
    should_stop: Arc<Mutex<bool>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    child_process: Option<Arc<Mutex<Option<Child>>>>,
}

impl AndroidLogProvider {
    pub fn new() -> Self {
        Self {
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            should_stop: Arc::new(Mutex::new(false)),
            thread_handle: None,
            child_process: None,
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
                match Self::run_adb_logcat(log_buffer, should_stop, child_process).await {
                    Ok(_) => log::debug!("adb logcat stopped normally"),
                    Err(e) => log::error!("adb logcat error: {}", e),
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
}

// async helper function to spawn adb logcat command and stream logs
impl AndroidLogProvider {
    async fn run_adb_logcat(
        log_buffer: Arc<Mutex<Vec<String>>>,
        should_stop: Arc<Mutex<bool>>,
        child_process: Arc<Mutex<Option<Child>>>,
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

            // spawn adb logcat command with '*:V' to get all verbose logs
            let mut child = match Command::new("adb")
                .arg("logcat")
                .arg("*:V")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    log::error!("Failed to spawn adb logcat: {}", e);
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
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                Ok(None) => {
                    // process still running - device found!
                    log::debug!("Android device connected, streaming logs...");

                    let stdout = stdout.expect("Failed to get stdout");
                    let mut reader = BufReader::new(stdout).lines();

                    // store the child process handle
                    if let Ok(mut child_opt) = child_process.lock() {
                        *child_opt = Some(child);
                    }

                    // stream logs continuously
                    loop {
                        // check if we should stop
                        if let Ok(stop) = should_stop.lock()
                            && *stop
                        {
                            log::debug!("Stop signal received, exiting adb logcat");
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
                                // push to buffer
                                if let Ok(mut buffer) = log_buffer.lock() {
                                    buffer.push(log_line);
                                }
                            }
                            Ok(Ok(None)) => {
                                log::debug!("adb logcat stream ended, device disconnected");
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
