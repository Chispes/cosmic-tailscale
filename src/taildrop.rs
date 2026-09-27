// SPDX-License-Identifier: MIT
use crate::tailscale::{self, CliError};
use ashpd::desktop::file_chooser::SelectedFiles;
use std::{ffi::OsString, fs, io, os::unix::fs::MetadataExt, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};

pub fn downloads() -> io::Result<PathBuf> {
    dirs::download_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG Downloads directory unavailable"))
}

pub fn staging(download_dir: &Path) -> PathBuf { download_dir.join(".cosmic-tailscale-incoming") }

static BATCH: AtomicU64 = AtomicU64::new(0);

/// Only completed batches are eligible for delivery; an interrupted CLI may
/// leave a partial file in an active batch, which must never be published.
pub fn deliver(download_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let incoming = staging(download_dir);
    fs::create_dir_all(&incoming)?;
    let mut delivered = Vec::new();
    for batch in fs::read_dir(&incoming)? {
        let batch = batch?;
        if !batch.file_type()?.is_dir() || !batch.file_name().to_string_lossy().starts_with("ready-") { continue; }
        for entry in fs::read_dir(batch.path())? {
            let entry = entry?;
            if !entry.file_type()?.is_file() { continue; }
            let source = entry.path();
            let name = entry.file_name();
            let source_meta = entry.metadata()?;
            for suffix in 0u64.. {
                let filename = if suffix == 0 { name.clone() } else {
                    let mut candidate = name.clone();
                    candidate.push(format!("-{suffix}"));
                    candidate
                };
                let destination = download_dir.join(filename);
                match fs::hard_link(&source, &destination) {
                    Ok(()) => { fs::remove_file(&source)?; delivered.push(destination); break; }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                        let existing = fs::metadata(&destination)?;
                        if existing.dev() == source_meta.dev() && existing.ino() == source_meta.ino() {
                            fs::remove_file(&source)?;
                            break;
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        fs::remove_dir(batch.path())?;
    }
    Ok(delivered)
}

pub async fn choose_and_send(destination: &str, title: &str) -> Result<Option<String>, CliError> {
    let response = SelectedFiles::open_file().title(title).multiple(true).send().await;
    let selected = match response {
        Ok(request) => match request.response() { Ok(files) => files, Err(ashpd::Error::Response(_)) => return Ok(None), Err(err) => return Err(CliError::Failed(err.to_string())) },
        Err(ashpd::Error::Response(_)) => return Ok(None),
        Err(err) => return Err(CliError::Failed(err.to_string())),
    };
    let mut args = vec![OsString::from("file"), OsString::from("cp"), OsString::from("--update-interval=0"), OsString::from("--")];
    for uri in selected.uris() {
        let path = uri.to_file_path().map_err(|_| CliError::InvalidData("portal returned a non-local file".into()))?;
        args.push(path.into_os_string());
    }
    if args.len() == 4 { return Ok(None); }
    args.push(OsString::from(format!("{destination}:")));
    tailscale::run_os(&args).await.map(Some)
}

pub async fn receive_once() -> Result<Vec<PathBuf>, CliError> {
    let downloads = downloads().map_err(|e| CliError::Failed(e.to_string()))?;
    let recovered = deliver(&downloads).map_err(|e| CliError::Failed(e.to_string()))?;
    if !recovered.is_empty() { return Ok(recovered); }
    let incoming = staging(&downloads);
    let id = format!("{}-{}", std::process::id(), BATCH.fetch_add(1, Ordering::Relaxed));
    let active = incoming.join(format!("active-{id}"));
    fs::create_dir(&active).map_err(|e| CliError::Failed(e.to_string()))?;
    let args = [OsString::from("file"), OsString::from("get"), OsString::from("--wait"), OsString::from("--conflict=rename"), active.clone().into_os_string()];
    tailscale::run_wait(&args).await?;
    fs::rename(active, incoming.join(format!("ready-{id}"))).map_err(|e| CliError::Failed(e.to_string()))?;
    deliver(&downloads).map_err(|e| CliError::Failed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_existing_files_and_recovers_only_completed_batches() {
        let root = tempfile::tempdir().unwrap();
        let stage = staging(root.path());
        fs::create_dir(&stage).unwrap();
        let active = stage.join("active-interrupted");
        let ready = stage.join("ready-complete");
        fs::create_dir(&active).unwrap();
        fs::create_dir(&ready).unwrap();
        fs::write(active.join("partial.txt"), "incomplete").unwrap();
        fs::write(root.path().join("report.txt"), "original").unwrap();
        fs::write(ready.join("report.txt"), "incoming").unwrap();
        let delivered = deliver(root.path()).unwrap();
        assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), b"original");
        assert_eq!(fs::read(&delivered[0]).unwrap(), b"incoming");
        assert_eq!(fs::read(active.join("partial.txt")).unwrap(), b"incomplete");
        assert!(!root.path().join("partial.txt").exists());
        assert!(!ready.exists());
    }
    #[test]
    fn completed_link_is_not_delivered_twice_after_interruption() {
        let root = tempfile::tempdir().unwrap();
        let ready = staging(root.path()).join("ready-retry");
        fs::create_dir_all(&ready).unwrap();
        let source = ready.join("report.txt");
        let destination = root.path().join("report.txt");
        fs::write(&source, "complete").unwrap();
        fs::hard_link(&source, &destination).unwrap();
        assert!(deliver(root.path()).unwrap().is_empty());
        assert_eq!(fs::read(&destination).unwrap(), b"complete");
        assert!(!root.path().join("report.txt-1").exists());
        assert!(!ready.exists());
    }
}
