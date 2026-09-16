use lazylog_framework::provider::{LogParser, LogProvider, ProviderStatus};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    flag, low_level,
};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub(crate) struct AgentOptions {
    pub(crate) duration: Option<Duration>,
}

struct AgentOutput<C> {
    capture: C,
    captured_items: usize,
    captured_lines: usize,
    captured_bytes: usize,
}

impl<C: Write> AgentOutput<C> {
    fn new(capture: C) -> Self {
        Self {
            capture,
            captured_items: 0,
            captured_lines: 0,
            captured_bytes: 0,
        }
    }

    fn write_item(&mut self, content: &str) -> io::Result<()> {
        let item_lines = physical_line_count(content);
        let item_bytes = content.len().saturating_add(1);

        self.capture.write_all(content.as_bytes())?;
        self.capture.write_all(b"\n")?;
        self.captured_items = self.captured_items.saturating_add(1);
        self.captured_lines = self.captured_lines.saturating_add(item_lines);
        self.captured_bytes = self.captured_bytes.saturating_add(item_bytes);
        Ok(())
    }

    fn flush_capture(&mut self) -> io::Result<()> {
        self.capture.flush()
    }

    fn finish(&mut self) -> io::Result<()> {
        self.capture.flush()
    }
}

fn physical_line_count(content: &str) -> usize {
    content
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        .saturating_add(1)
}

fn open_new_capture(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

fn create_capture_file() -> io::Result<(PathBuf, File)> {
    let capture_dir = std::env::temp_dir().join("lazylog").join("agent-captures");
    fs::create_dir_all(&capture_dir)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let process_id = std::process::id();

    for suffix in 0..1000 {
        let suffix = if suffix == 0 {
            String::new()
        } else {
            format!("-{suffix}")
        };
        let path = capture_dir.join(format!(
            "lazylog-agent-{timestamp}-{process_id}{suffix}.log"
        ));
        match open_new_capture(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique lazylog capture file",
    ))
}

struct SignalRegistrations(Vec<signal_hook::SigId>);

impl Drop for SignalRegistrations {
    fn drop(&mut self) {
        for signal_id in self.0.drain(..) {
            low_level::unregister(signal_id);
        }
    }
}

fn install_stop_signals() -> io::Result<(Arc<AtomicBool>, SignalRegistrations)> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut registrations = SignalRegistrations(Vec::new());

    for signal in [SIGINT, SIGTERM] {
        let signal_id = flag::register(signal, stop.clone())?;
        registrations.0.push(signal_id);
    }

    Ok((stop, registrations))
}

fn capture_batch<C: Write>(
    raw_logs: Vec<String>,
    parser: &Arc<dyn LogParser>,
    output: &mut AgentOutput<C>,
) -> io::Result<()> {
    for raw_log in raw_logs {
        let Some(item) = parser.parse(&raw_log) else {
            continue;
        };
        output.write_item(&item.raw_content)?;
    }

    output.flush_capture()
}

fn remember_first_error(slot: &mut Option<io::Error>, result: io::Result<()>) {
    if slot.is_none()
        && let Err(error) = result
    {
        *slot = Some(error);
    }
}

fn report_provider_status<P: LogProvider>(provider: &P, previous: &mut Option<ProviderStatus>) {
    let current = provider.status();
    if current != *previous {
        if let Some(status) = current {
            eprintln!("[lazylog] 连接状态: {}", status.label());
        }
        *previous = current;
    }
}

pub(crate) fn run_agent<P>(
    mut provider: P,
    parser: Arc<dyn LogParser>,
    poll_interval: Duration,
    options: &AgentOptions,
) -> io::Result<()>
where
    P: LogProvider,
{
    let (capture_path, capture_file) = create_capture_file()?;
    let (stop, _signal_registrations) = install_stop_signals()?;
    let mut output = AgentOutput::new(BufWriter::new(capture_file));

    eprintln!("[lazylog] complete capture: {}", capture_path.display());
    provider.start().map_err(io::Error::other)?;

    let started_at = Instant::now();
    let mut first_error = None;
    let mut provider_status = None;
    report_provider_status(&provider, &mut provider_status);

    while !stop.load(Ordering::Relaxed)
        && options
            .duration
            .is_none_or(|duration| started_at.elapsed() < duration)
    {
        match provider.poll_logs() {
            Ok(raw_logs) => {
                if let Err(error) = capture_batch(raw_logs, &parser, &mut output) {
                    first_error = Some(error);
                    break;
                }
            }
            Err(error) => eprintln!("Provider poll error: {error}"),
        }
        report_provider_status(&provider, &mut provider_status);

        let sleep_for = options
            .duration
            .map(|duration| {
                duration
                    .saturating_sub(started_at.elapsed())
                    .min(poll_interval)
            })
            .unwrap_or(poll_interval);
        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
    }

    remember_first_error(&mut first_error, provider.stop().map_err(io::Error::other));

    if first_error.is_none() {
        match provider.poll_logs() {
            Ok(raw_logs) => remember_first_error(
                &mut first_error,
                capture_batch(raw_logs, &parser, &mut output),
            ),
            Err(error) => eprintln!("Provider final poll error: {error}"),
        }
    }

    remember_first_error(&mut first_error, output.finish());
    eprintln!(
        "[lazylog] capture complete: {} ({} items, {} lines, {} bytes)",
        capture_path.display(),
        output.captured_items,
        output.captured_lines,
        output.captured_bytes
    );

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_lines_include_the_record_terminator() {
        assert_eq!(physical_line_count(""), 1);
        assert_eq!(physical_line_count("one"), 1);
        assert_eq!(physical_line_count("one\ntwo"), 2);
        assert_eq!(physical_line_count("one\n"), 2);
    }

    #[test]
    fn output_captures_every_item_without_a_preview_sink() {
        let mut output = AgentOutput::new(Vec::new());

        output.write_item("one\ntwo").unwrap();
        output.write_item("three").unwrap();
        output.write_item("four").unwrap();
        output.finish().unwrap();

        assert_eq!(output.capture, b"one\ntwo\nthree\nfour\n");
        assert_eq!(output.captured_items, 3);
        assert_eq!(output.captured_lines, 4);
        assert_eq!(output.captured_bytes, 19);
    }

    #[test]
    fn every_capture_uses_a_unique_os_temporary_file() {
        let (first_path, first_file) = create_capture_file().unwrap();
        let (second_path, second_file) = create_capture_file().unwrap();
        drop((first_file, second_file));

        assert_ne!(first_path, second_path);
        assert!(first_path.starts_with(std::env::temp_dir().join("lazylog/agent-captures")));
        assert!(second_path.starts_with(std::env::temp_dir().join("lazylog/agent-captures")));

        fs::remove_file(first_path).unwrap();
        fs::remove_file(second_path).unwrap();
    }
}
