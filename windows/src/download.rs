//! Fast model downloads and archive unpacking.
//!
//! A single TCP stream rarely fills a fast line (CDNs throttle per
//! connection), so a file is fetched as 8 MB ranges over several connections
//! at once and written in place. Servers without range support fall back to
//! one stream. Progress is reported a few times per second, never per chunk.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const CONNECTIONS: usize = 8;
const CHUNK_SIZE: u64 = 8 * 1024 * 1024;
const MIN_PARALLEL_SIZE: u64 = 16 * 1024 * 1024;
const COPY_BUFFER: usize = 256 * 1024;
const MAX_ATTEMPTS: u32 = 4;
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
const REPORT_INTERVAL: Duration = Duration::from_millis(150);

/// What a model install is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Progress {
    Downloading {
        /// 0..=1
        fraction: f64,
        /// Smoothed transfer rate.
        bytes_per_second: f64,
    },
    Unpacking,
}

/// Tunables, so tests can exercise the parallel path with small files.
#[derive(Debug, Clone, Copy)]
struct Plan {
    connections: usize,
    chunk_size: u64,
    min_parallel_size: u64,
}

const PLAN: Plan = Plan { connections: CONNECTIONS, chunk_size: CHUNK_SIZE, min_parallel_size: MIN_PARALLEL_SIZE };

/// Downloads `url` to `destination`. `known_size` is used for the progress
/// fraction when the server does not announce a length.
///
/// # Errors
/// Returns a user-facing message when the transfer fails.
pub fn fetch(url: &str, destination: &Path, known_size: u64, progress: &mut dyn FnMut(Progress)) -> Result<(), String> {
    fetch_with(url, destination, known_size, progress, PLAN)
}

fn fetch_with(
    url: &str,
    destination: &Path,
    known_size: u64,
    progress: &mut dyn FnMut(Progress),
    plan: Plan,
) -> Result<(), String> {
    let agent = crate::http::download_agent(STALL_TIMEOUT);
    let (mut total, ranges) = probe(&agent, url)?;
    if total == 0 {
        total = known_size;
    }

    let counter = AtomicU64::new(0);
    let finished = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let result = if ranges && total >= plan.min_parallel_size {
                parallel(&agent, url, destination, total, &counter, plan)
            } else {
                single(&agent, url, destination, &counter)
            };
            finished.store(true, Ordering::Release);
            result
        });
        report(&counter, total, &finished, progress);
        worker.join().unwrap_or_else(|_| Err("the download thread panicked".to_string()))
    });
    result?;
    progress(Progress::Downloading { fraction: 1.0, bytes_per_second: 0.0 });
    Ok(())
}

fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("the server answered HTTP {code}"),
        ureq::Error::Transport(t) => format!("network error: {t}"),
    }
}

/// Asks for the first byte: a 206 answer reveals the total size and proves
/// range support. No compression, because byte ranges must address the file
/// as stored.
fn probe(agent: &ureq::Agent, url: &str) -> Result<(u64, bool), String> {
    let response =
        agent.get(url).set("Accept-Encoding", "identity").set("Range", "bytes=0-0").call().map_err(|e| describe(&e))?;
    if response.status() == 206 {
        let total = response.header("Content-Range").and_then(|v| v.rsplit('/').next()).and_then(|v| v.parse().ok());
        if let Some(total) = total {
            return Ok((total, true));
        }
    }
    let length = response.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    Ok((length, false))
}

/// What the chunk workers share.
struct Transfer<'a> {
    agent: &'a ureq::Agent,
    url: &'a str,
    file: &'a File,
    counter: &'a AtomicU64,
    failed: &'a AtomicBool,
}

fn parallel(
    agent: &ureq::Agent,
    url: &str,
    destination: &Path,
    total: u64,
    counter: &AtomicU64,
    plan: Plan,
) -> Result<(), String> {
    let file = File::create(destination).map_err(|e| e.to_string())?;
    file.set_len(total).map_err(|e| e.to_string())?;
    let failed = AtomicBool::new(false);
    let transfer = Transfer { agent, url, file: &file, counter, failed: &failed };
    let chunks = usize::try_from(total.div_ceil(plan.chunk_size)).unwrap_or(usize::MAX);
    let next = AtomicUsize::new(0);
    let error = Mutex::new(None::<String>);

    std::thread::scope(|scope| {
        for _ in 0..plan.connections.min(chunks) {
            scope.spawn(|| {
                while !failed.load(Ordering::Relaxed) {
                    let chunk = next.fetch_add(1, Ordering::Relaxed);
                    if chunk >= chunks {
                        break;
                    }
                    let from = chunk as u64 * plan.chunk_size;
                    let to = (from + plan.chunk_size).min(total) - 1;
                    if let Err(e) = fetch_chunk(&transfer, from, to) {
                        failed.store(true, Ordering::Relaxed);
                        if let Ok(mut slot) = error.lock() {
                            slot.get_or_insert(e);
                        }
                        break;
                    }
                }
            });
        }
    });
    match error.into_inner().ok().flatten() {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

/// Downloads bytes `[from, to]` to the same offsets, retrying the whole chunk
/// on failure or stall.
fn fetch_chunk(transfer: &Transfer<'_>, from: u64, to: u64) -> Result<(), String> {
    let mut attempt = 1;
    loop {
        let mut received = 0u64;
        match try_chunk(transfer, from, to, &mut received) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < MAX_ATTEMPTS && !transfer.failed.load(Ordering::Relaxed) => {
                transfer.counter.fetch_sub(received, Ordering::Relaxed); // this chunk starts over
                log::warn!("download chunk {from}-{to} failed (attempt {attempt}): {e}");
                std::thread::sleep(Duration::from_millis(300 * u64::from(attempt)));
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn try_chunk(transfer: &Transfer<'_>, from: u64, to: u64, received: &mut u64) -> Result<(), String> {
    let response = transfer
        .agent
        .get(transfer.url)
        .set("Accept-Encoding", "identity")
        .set("Range", &format!("bytes={from}-{to}"))
        .call()
        .map_err(|e| describe(&e))?;
    if response.status() != 206 {
        return Err(format!("server ignored the range request ({})", response.status()));
    }
    let mut body = response.into_reader();
    let mut buffer = vec![0u8; COPY_BUFFER];
    while !transfer.failed.load(Ordering::Relaxed) {
        let read = body.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        write_at(transfer.file, &buffer[..read], from + *received).map_err(|e| e.to_string())?;
        *received += read as u64;
        transfer.counter.fetch_add(read as u64, Ordering::Relaxed);
    }
    if *received == to - from + 1 { Ok(()) } else { Err("the connection closed early".into()) }
}

#[cfg(windows)]
fn write_at(file: &File, mut buffer: &[u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buffer.is_empty() {
        let written = file.seek_write(buffer, offset)?;
        buffer = &buffer[written..];
        offset += written as u64;
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_at(file: &File, buffer: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(buffer, offset)
}

fn single(agent: &ureq::Agent, url: &str, destination: &Path, counter: &AtomicU64) -> Result<(), String> {
    let response = agent.get(url).set("Accept-Encoding", "identity").call().map_err(|e| describe(&e))?;
    let mut body = response.into_reader();
    let mut file = File::create(destination).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = body.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            return file.flush().map_err(|e| e.to_string());
        }
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        counter.fetch_add(read as u64, Ordering::Relaxed);
    }
}

/// Reports progress until the transfer is `finished`.
fn report(counter: &AtomicU64, total: u64, finished: &AtomicBool, progress: &mut dyn FnMut(Progress)) {
    let clock = Instant::now();
    let mut last_bytes = 0u64;
    let mut last_time = Duration::ZERO;
    let mut rate = 0.0_f64;
    while !finished.load(Ordering::Acquire) {
        std::thread::sleep(REPORT_INTERVAL);
        let bytes = counter.load(Ordering::Relaxed);
        let now = clock.elapsed();
        #[allow(clippy::cast_precision_loss)]
        let (instant, fraction) = (
            bytes.saturating_sub(last_bytes) as f64 / now.saturating_sub(last_time).as_secs_f64().max(0.001),
            if total > 0 { (bytes as f64 / total as f64).min(1.0) } else { 0.0 },
        );
        rate = if rate == 0.0 { instant } else { rate * 0.7 + instant * 0.3 };
        (last_bytes, last_time) = (bytes, now);
        progress(Progress::Downloading { fraction, bytes_per_second: rate });
    }
}

/// Extracts a `.tar.bz2` into `target`, dropping the archive's top-level
/// folder. The folder appears only when extraction is complete.
///
/// Uses the system `tar` (bsdtar ships with Windows 10+). Both bsdtar and GNU
/// tar refuse or neutralize absolute and `..` paths, so nothing escapes `-C`.
///
/// # Errors
/// Returns a user-facing message when `tar` is missing or fails.
pub fn extract_tar_bz2(archive: &Path, target: &Path) -> Result<(), String> {
    let mut staging = target.as_os_str().to_owned();
    staging.push(".extracting");
    let staging = std::path::PathBuf::from(staging);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let result = run_tar(archive, &staging).and_then(|()| {
        let _ = std::fs::remove_dir_all(target);
        std::fs::rename(&staging, target).map_err(|e| e.to_string())
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn run_tar(archive: &Path, staging: &Path) -> Result<(), String> {
    let tar = if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").map_or_else(|| "C:\\Windows".into(), std::path::PathBuf::from);
        root.join("System32").join("tar.exe")
    } else {
        "tar".into()
    };
    let mut command = Command::new(&tar);
    command.arg("-xjf").arg(archive).arg("-C").arg(staging).arg("--strip-components=1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|e| format!("cannot run {}: {e}", tar.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("unpacking failed: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;

    /// Serves `data`; with `ranges` it honors `Range: bytes=a-b` with 206.
    fn serve(data: Vec<u8>, ranges: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let data = Arc::new(data);
            for stream in listener.incoming().flatten() {
                let data = Arc::clone(&data);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    let mut range = None;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                            if line.trim().is_empty() {
                                break;
                            }
                            return;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                            let (a, b) = v.trim().split_once('-').unwrap();
                            range = Some((a.parse::<usize>().unwrap(), b.parse::<usize>().unwrap()));
                        }
                    }
                    let response = if let Some((a, b)) = range.filter(|_| ranges) {
                        let slice = &data[a..=b.min(data.len() - 1)];
                        let head = format!(
                            "HTTP/1.1 206 Partial Content
Content-Range: bytes {a}-{}/{}
Content-Length: {}
Connection: close

",
                            a + slice.len() - 1,
                            data.len(),
                            slice.len()
                        );
                        [head.into_bytes(), slice.to_vec()].concat()
                    } else {
                        let head = format!(
                            "HTTP/1.1 200 OK
Content-Length: {}
Connection: close

",
                            data.len()
                        );
                        [head.into_bytes(), data.to_vec()].concat()
                    };
                    let _ = stream.write_all(&response);
                });
            }
        });
        url
    }

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| u8::try_from((i * 31 + i / 7) % 251).unwrap()).collect()
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("voiceink-dl-{}-{name}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn downloads_in_parallel_ranges() {
        let data = sample(1_000_003);
        let url = serve(data.clone(), true);
        let target = temp_file("parallel");
        let plan = Plan { connections: 4, chunk_size: 64 * 1024, min_parallel_size: 1024 };
        let mut last = None;
        fetch_with(&url, &target, 0, &mut |p| last = Some(p), plan).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
        assert_eq!(last, Some(Progress::Downloading { fraction: 1.0, bytes_per_second: 0.0 }));
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn falls_back_to_a_single_stream_without_range_support() {
        let data = sample(200_000);
        let url = serve(data.clone(), false);
        let target = temp_file("single");
        let plan = Plan { connections: 4, chunk_size: 16 * 1024, min_parallel_size: 1024 };
        fetch_with(&url, &target, 0, &mut |_| {}, plan).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn reports_http_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/missing", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        });
        let target = temp_file("missing");
        let error = fetch(&url, &target, 0, &mut |_| {}).unwrap_err();
        assert_eq!(error, "the server answered HTTP 404");
    }

    #[test]
    fn unpacks_tar_bz2_without_the_top_level_folder() {
        let root = temp_file("tar");
        let source = root.join("model-1");
        std::fs::create_dir_all(source.join("sub")).unwrap();
        std::fs::write(source.join("tokens.txt"), "a").unwrap();
        std::fs::write(source.join("sub").join("encoder.onnx"), "b").unwrap();
        let archive = root.join("model-1.tar.bz2");
        let made = Command::new("tar").arg("-cjf").arg(&archive).arg("-C").arg(&root).arg("model-1").status();
        if !made.is_ok_and(|s| s.success()) {
            return; // no bzip2-capable tar on this machine
        }

        let target = root.join("model-1-installed");
        extract_tar_bz2(&archive, &target).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("tokens.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(target.join("sub").join("encoder.onnx")).unwrap(), "b");
        assert!(!root.join("model-1-installed.extracting").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
