// SPDX-License-Identifier: MIT
use crate::tailscale::{self, CliError};
use ashpd::desktop::file_chooser::SelectedFiles;
use std::{ffi::OsString, fs, io, path::{Path, PathBuf}, time::Duration};

pub fn downloads() -> io::Result<PathBuf> {
    dirs::download_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG Downloads directory unavailable"))
}

pub fn staging(download_dir: &Path) -> PathBuf { download_dir.join(".cosmic-tailscale-incoming") }

/// A link is atomic on the Downloads filesystem: an existing file can never be replaced.
pub fn deliver(download_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let incoming = staging(download_dir);
    fs::create_dir_all(&incoming)?;
    let mut delivered = Vec::new();
    for entry in fs::read_dir(&incoming)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() { continue; }
        let source = entry.path();
        let name = entry.file_name();
        for suffix in 0u64.. {
            let filename = if suffix == 0 { name.clone() } else {
                let mut candidate = name.clone();
                candidate.push(format!("-{suffix}"));
                candidate
            };
            let destination = download_dir.join(filename);
            match fs::hard_link(&source, &destination) {
                Ok(()) => { fs::remove_file(&source)?; delivered.push(destination); break; }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
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
    let args = [OsString::from("file"), OsString::from("get"), OsString::from("--wait"), OsString::from("--conflict=rename"), incoming.into_os_string()];
    match tailscale::run_with_timeout(&args, Duration::from_secs(65)).await {
        Ok(_) | Err(CliError::Timeout) => {},
        Err(error) => return Err(error),
    }
    deliver(&downloads).map_err(|e| CliError::Failed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_existing_files_and_recovers_staging() {
        let root = tempfile::tempdir().unwrap();
        let stage = staging(root.path());
        fs::create_dir(&stage).unwrap();
        fs::write(root.path().join("report.txt"), "original").unwrap();
        fs::write(stage.join("report.txt"), "incoming").unwrap();
        let delivered = deliver(root.path()).unwrap();
        assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), b"original");
        assert_eq!(fs::read(&delivered[0]).unwrap(), b"incoming");
        assert!(!stage.join("report.txt").exists());
    }
}
