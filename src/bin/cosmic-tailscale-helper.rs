// SPDX-License-Identifier: MIT

use std::{collections::HashMap, ffi::CStr, time::Duration};
use zbus::{fdo::DBusProxy, message::Header, zvariant::Value, Connection};

const NAME: &str = "io.github.chispes.CosmicTailscale.Helper";
const ACTION: &str = "io.github.chispes.CosmicTailscale.enable-operator";

struct Helper {
    connection: Connection,
}

#[zbus::interface(name = "io.github.chispes.CosmicTailscale.Helper")]
impl Helper {
    #[zbus(name = "EnableOperator")]
    async fn enable_operator(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        let sender = header.sender().ok_or_else(denied)?;
        let bus = DBusProxy::new(&self.connection).await.map_err(failed)?;
        let uid = bus.get_connection_unix_user(sender.clone().into()).await.ok();
        let uid = uid.filter(|uid| *uid != 0).ok_or_else(denied)?;

        let authority = zbus::Proxy::new(&self.connection, "org.freedesktop.PolicyKit1",
            "/org/freedesktop/PolicyKit1/Authority", "org.freedesktop.PolicyKit1.Authority")
            .await.map_err(failed)?;
        let mut details: HashMap<&str, Value<'_>> = HashMap::new();
        details.insert("name", Value::from(sender.as_str()));
        let subject = ("system-bus-name", details);
        let result: (bool, bool, HashMap<String, String>) = authority.call("CheckAuthorization",
            &(subject, ACTION, HashMap::<String, String>::new(), 1u32, ""))
            .await.map_err(|_| denied())?;
        let current = bus.get_connection_unix_user(sender.clone().into()).await.ok();
        authorized_caller(Some(uid), result.0, current)?;
        let username = username(uid).ok_or_else(denied)?;
        let mut child = tokio::process::Command::new("/usr/bin/tailscale")
            .arg("set").arg(format!("--operator={username}"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn().map_err(failed)?;
        let status = tokio::time::timeout(Duration::from_secs(20), child.wait()).await
            .map_err(|_| zbus::fdo::Error::Failed("Timeout".into()))?
            .map_err(failed)?;
        if !status.success() { return Err(zbus::fdo::Error::Failed("Tailscale failed".into())); }
        Ok(())
    }
}

fn authorized_caller(original: Option<u32>, approved: bool, current: Option<u32>) -> zbus::fdo::Result<()> {
    match original {
        Some(uid) if uid != 0 && approved && current == Some(uid) => Ok(()),
        _ => Err(denied()),
    }
}

fn denied() -> zbus::fdo::Error { zbus::fdo::Error::AccessDenied("Denied".into()) }
fn failed(error: impl std::fmt::Display) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
}

fn username(uid: u32) -> Option<String> {
    let mut pwd = unsafe { std::mem::zeroed::<libc::passwd>() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; 16384];
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buffer.as_mut_ptr().cast(), buffer.len(), &mut result) };
    if rc != 0 || result.is_null() { return None; }
    let name = unsafe { CStr::from_ptr(pwd.pw_name) }.to_str().ok()?;
    if name.is_empty() || name.starts_with('-') { return None; }
    Some(name.to_owned())
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    if unsafe { libc::geteuid() } != 0 { return Err(zbus::Error::Failure("root required".into())); }
    let connection = zbus::Connection::system().await?;
    connection.object_server().at("/io/github/chispes/CosmicTailscale/Helper", Helper { connection: connection.clone() }).await?;
    connection.request_name(NAME).await?;
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_callers_never_reach_command() {
        for (original, approved, current) in [
            (None, true, Some(1000)),
            (Some(0), true, Some(0)),
            (Some(1000), false, Some(1000)),
            (Some(1000), true, None),
            (Some(1000), true, Some(1001)),
        ] {
            assert!(authorized_caller(original, approved, current).is_err());
        }
        assert!(authorized_caller(Some(1000), true, Some(1000)).is_ok());
    }
}
