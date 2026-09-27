use serde::Deserialize;
use std::{collections::HashMap, ffi::OsString, io, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

const LIMIT: u64 = 1024 * 1024;
const FILE_SHARING: &str = "https://tailscale.com/cap/file-sharing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    Missing,
    Daemon,
    AccessDenied,
    ProfilesDenied,
    Unsupported,
    Timeout,
    TooLarge,
    InvalidData(String),
    Failed(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub async fn run(args: &[&str]) -> Result<String, CliError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    run_os(&args).await
}

pub async fn run_os(args: &[OsString]) -> Result<String, CliError> {
    run_with_timeout(args, Duration::from_secs(20)).await
}

pub async fn run_with_timeout(args: &[OsString], duration: Duration) -> Result<String, CliError> {
    let mut cmd = if cfg!(feature = "flatpak") {
        Command::new("/app/bin/tailscale")
    } else {
        Command::new("tailscale")
    };
    if cfg!(feature = "flatpak") {
        cmd.arg("--socket=/run/tailscale/tailscaled.sock");
    }
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| if e.kind() == io::ErrorKind::NotFound { CliError::Missing } else { CliError::Failed(e.to_string()) })?;
    let mut stdout = child.stdout.take().expect("piped stdout").take(LIMIT + 1);
    let mut stderr = child.stderr.take().expect("piped stderr").take(LIMIT + 1);
    let result = timeout(duration, async {
        let (out, err, status) = tokio::try_join!(async { let mut buf = Vec::new(); stdout.read_to_end(&mut buf).await?; Ok::<_, io::Error>(buf) }, async { let mut buf = Vec::new(); stderr.read_to_end(&mut buf).await?; Ok::<_, io::Error>(buf) }, child.wait())?;
        Ok::<_, io::Error>((out, err, status))
    }).await.map_err(|_| CliError::Timeout)?.map_err(|e| CliError::Failed(e.to_string()))?;
    let (out, err, status) = result;
    if out.len() as u64 > LIMIT || err.len() as u64 > LIMIT { return Err(CliError::TooLarge); }
    if !status.success() {
        let message = String::from_utf8_lossy(&err).trim().to_string();
        let lower = message.to_ascii_lowercase();
        return Err(if lower.contains("profiles access denied") { CliError::ProfilesDenied }
            else if lower.contains("permission denied") || lower.contains("access denied") { CliError::AccessDenied }
            else if lower.contains("failed to connect to local tailscaled") || lower.contains("tailscaled.sock") || lower.contains("not running") { CliError::Daemon }
            else if lower.contains("unknown command") || lower.contains("flag provided but not defined") || lower.contains("unknown flag") { CliError::Unsupported }
            else { CliError::Failed(if message.is_empty() { format!("exit status: {status}") } else { message }) });
    }
    String::from_utf8(out).map_err(|e| CliError::InvalidData(e.to_string()))
}

fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where D: serde::Deserializer<'de>, T: Deserialize<'de> + Default {
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub struct Peer {
    pub host_name: String,
    #[serde(rename = "DNSName")]
    pub dns_name: String,
    #[serde(rename = "TailscaleIPs", deserialize_with = "null_default")]
    pub tailscale_ips: Vec<String>,
    #[serde(rename = "UserID")]
    pub user_id: u64,
    pub online: bool,
    pub exit_node_option: bool,
    pub exit_node: bool,
    pub taildrop_target: i32,
    #[serde(rename = "CapMap", deserialize_with = "null_default")]
    pub cap_map: HashMap<String, serde_json::Value>,
    #[serde(deserialize_with = "null_default")]
    pub capabilities: Vec<String>,
}

impl Peer {
    pub fn name(&self) -> &str {
        if !self.host_name.is_empty() && self.host_name != "localhost" { &self.host_name } else { self.dns_name.trim_end_matches('.') }
    }
    pub fn can_receive(&self, self_id: u64) -> bool {
        self.online && match self.taildrop_target { 1 => true, 0 => self.user_id != 0 && self.user_id == self_id, _ => false }
    }
    pub fn mullvad(&self) -> bool { self.dns_name.trim_end_matches('.').to_ascii_lowercase().ends_with(".mullvad.ts.net") }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub struct Tailnet { pub name: String, #[serde(rename = "MagicDNSSuffix")] pub magic_dns_suffix: String }

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
pub struct Status {
    pub backend_state: String,
    #[serde(rename = "AuthURL")]
    pub auth_url: String,
    #[serde(rename = "Self")]
    pub self_node: Peer,
    #[serde(deserialize_with = "null_default")]
    pub peer: HashMap<String, Peer>,
    pub current_tailnet: Option<Tailnet>,
}
impl Status {
    pub fn running(&self) -> bool { self.backend_state == "Running" }
    pub fn file_sharing(&self) -> bool { self.self_node.cap_map.contains_key(FILE_SHARING) || self.self_node.capabilities.iter().any(|c| c == FILE_SHARING) }
    pub fn online_peers(&self) -> impl Iterator<Item = (&String, &Peer)> { self.peer.iter().filter(|(_, peer)| peer.online && !peer.mullvad()) }
    pub fn auth_link(&self) -> Option<&str> {
        let url = url::Url::parse(&self.auth_url).ok()?;
        let host = url.host_str()?;
        ((self.backend_state == "NeedsLogin" || self.backend_state == "NeedsMachineAuth") && url.scheme() == "https" && (host == "tailscale.com" || host.ends_with(".tailscale.com")) && url.port().is_none() && url.username().is_empty() && url.password().is_none()).then_some(self.auth_url.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    #[serde(alias = "ID")]
    pub id: String,
    #[serde(default, alias = "Nickname")]
    pub nickname: String,
    #[serde(default, alias = "Tailnet")]
    pub tailnet: String,
    #[serde(default, alias = "Account", alias = "loginName")]
    pub account: String,
    #[serde(default, alias = "Selected")]
    pub selected: bool,
}
impl Account { pub fn label(&self) -> &str { if !self.nickname.is_empty() { &self.nickname } else if !self.tailnet.is_empty() { &self.tailnet } else if !self.account.is_empty() { &self.account } else { &self.id } } }

#[derive(Debug, Clone)]
pub struct ExitNode { pub ip: String, pub hostname: String, pub country: String, pub city: String }
impl ExitNode { pub fn region(&self) -> String { format!("{}, {}", self.city, self.country) } }

pub fn parse_exit_nodes(text: &str) -> Result<Vec<ExitNode>, CliError> {
    let mut lines = text.lines();
    let Some(header) = lines.find(|line| line.trim_start().starts_with("IP") && line.contains("HOSTNAME") && line.contains("COUNTRY") && line.contains("CITY") && line.contains("STATUS")) else {
        if text.trim().is_empty() || text.contains("no exit nodes found") { return Ok(Vec::new()); }
        return Err(CliError::InvalidData("exit node table header missing".into()));
    };
    let positions: Vec<usize> = ["IP", "HOSTNAME", "COUNTRY", "CITY", "STATUS"].iter().map(|key| header.find(key).expect("checked header")).collect();
    let mut nodes = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#')) {
        let col = |i: usize| -> &str { line.get(positions[i]..positions.get(i + 1).copied().unwrap_or(line.len())).unwrap_or("").trim() };
        if col(0).is_empty() || col(1).is_empty() { return Err(CliError::InvalidData("invalid exit node row".into())); }
        nodes.push(ExitNode { ip: col(0).into(), hostname: col(1).into(), country: col(2).into(), city: col(3).into() });
    }
    Ok(nodes)
}

pub async fn status() -> Result<Status, CliError> {
    serde_json::from_str(&run(&["status", "--json"]).await?).map_err(|e| CliError::InvalidData(e.to_string()))
}
pub async fn accounts() -> Result<Vec<Account>, CliError> {
    serde_json::from_str(&run(&["switch", "--list", "--json"]).await?).map_err(|e| CliError::InvalidData(e.to_string()))
}
pub async fn exit_nodes() -> Result<Vec<ExitNode>, CliError> {
    match run(&["exit-node", "list"]).await {
        Ok(text) => parse_exit_nodes(&text),
        Err(CliError::Failed(message)) if message.contains("no exit nodes found") => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_distinguishes_states_and_rejects_forged_auth() {
        let status: Status = serde_json::from_str(r#"{"BackendState":"NeedsLogin","AuthURL":"https://login.tailscale.com/a","Self":{"UserID":4,"CapMap":{"https://tailscale.com/cap/file-sharing":[]}},"Peer":{"key":{"HostName":"desk","UserID":4,"Online":true,"TaildropTarget":1}}}"#).unwrap();
        assert!(!status.running());
        assert!(status.auth_link().is_some());
        assert!(status.file_sharing());
        assert_eq!(status.online_peers().count(), 1);
        let mut forged = status.clone();
        forged.auth_url = "https://tailscale.com.evil.test/login".into();
        assert!(forged.auth_link().is_none());
    }
    #[test]
    fn logged_out_daemon_null_fields_are_not_parse_errors() {
        let status: Status = serde_json::from_str(r#"{"BackendState":"NeedsLogin","Self":{"HostName":"fedora","TailscaleIPs":null,"CapMap":null,"Capabilities":null},"Peer":null,"CurrentTailnet":null}"#).unwrap();
        assert_eq!(status.backend_state, "NeedsLogin");
        assert!(status.online_peers().next().is_none());
    }
    #[test]
    fn explicit_taildrop_denial_overrides_same_owner() {
        let peer = Peer { online: true, user_id: 4, taildrop_target: 2, ..Peer::default() };
        assert!(!peer.can_receive(4));
    }
}
