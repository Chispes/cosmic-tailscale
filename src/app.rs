// SPDX-License-Identifier: MIT
use crate::{authorization::{self, AuthorizationError}, fl, taildrop, tailscale::{self, Account, CliError, ExitNode, Status}};
use cosmic::iced::futures::SinkExt;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{clipboard, time, window::Id, Limits, Subscription};
use cosmic::prelude::*;
use cosmic::widget;
use std::time::Duration;

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    status: Option<Status>,
    accounts: Vec<Account>,
    exit_nodes: Vec<ExitNode>,
    error: Option<String>,
    warning: Option<String>,
    action_error: Option<String>,
    notice: Option<String>,
    busy: bool,
    refreshing: bool,
    pending_login: bool,
    operator_denied: bool,
    expanded_peer: Option<String>,
    accounts_open: bool,
    nodes_open: bool,
}

#[derive(Debug, Clone)]
pub enum Operation { Up, Down, Switch(String), Exit(String), OpenAuth }

type Loaded = (Status, Vec<Account>, Vec<ExitNode>, Option<String>, bool);

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup, PopupClosed(Id), Tick, Refresh,
    AuthorizeOperator, OperatorAuthorized(Result<(), AuthorizationError>),
    TogglePeer(String), ToggleAccounts, ToggleNodes,
    Refreshed(Box<Result<Loaded, CliError>>),
    Act(Operation), Acted(Result<String, CliError>), Copy(String),
    Send(String), Sent(Result<Option<String>, CliError>), Received(Result<Vec<std::path::PathBuf>, CliError>), Notified,
}

fn error_text(error: &CliError) -> String {
    match error {
        CliError::Missing => fl!("error-missing"),
        CliError::Daemon => fl!("error-daemon"),
        CliError::ProfilesDenied => fl!("error-operator"),
        CliError::AccessDenied => fl!("error-access"),
        CliError::Unsupported => fl!("error-unsupported"),
        CliError::Timeout => fl!("error-timeout"),
        other => format!("{}: {other}", fl!("error-command")),
    }
}

async fn load() -> Result<Loaded, CliError> {
    let status = tailscale::status().await?;
    if !status.running() { return Ok((status, Vec::new(), Vec::new(), None, false)); }
    let (accounts, account_error, account_denied) = match tailscale::accounts().await {
        Ok(items) => (items, None, false), Err(err) => { let denied = denied(&err); (Vec::new(), Some(error_text(&err)), denied) },
    };
    let (nodes, exit_error, exit_denied) = match tailscale::exit_nodes().await {
        Ok(items) => (items, None, false), Err(err) => { let denied = denied(&err); (Vec::new(), Some(error_text(&err)), denied) },
    };
    let warning = match (account_error, exit_error) {
        (Some(account), Some(exit)) if account != exit => Some(format!("{account}; {exit}")),
        (Some(error), _) | (_, Some(error)) => Some(error),
        (None, None) => None,
    };
    Ok((status, accounts, nodes, warning, account_denied || exit_denied))
}

fn denied(error: &CliError) -> bool {
    matches!(error, CliError::ProfilesDenied | CliError::AccessDenied)
}

fn surface<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::container(content)
        .width(cosmic::iced::Length::Fill)
        .padding(12)
        .style(|theme: &cosmic::Theme| {
            let cosmic = theme.cosmic();
            let component = &cosmic.background(theme.transparent).component;
            let mut color: cosmic::iced::Color = component.base.into();
            if theme.transparent { color.a = 0.4; }
            cosmic::iced::widget::container::Style {
                text_color: Some(component.on.into()),
                background: Some(cosmic::iced::Background::Color(color)),
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    width: 1.0,
                    color: component.divider.into(),
                },
                ..Default::default()
            }
        })
        .into()
}


impl AppModel {
    fn refresh(&mut self) -> Task<cosmic::Action<Message>> {
        if self.refreshing || self.busy { return Task::none(); }
        self.refreshing = true;
        cosmic::task::future(async { cosmic::Action::App(Message::Refreshed(Box::new(load().await))) })
    }
    fn button(&self, label: String, action: Message) -> Element<'_, Message> {
        let button = widget::button::text(label).width(cosmic::iced::Length::Fill);
        if self.busy || self.refreshing { button.into() } else { button.on_press(action).into() }
    }
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "io.github.chispes.CosmicTailscale";

    fn core(&self) -> &cosmic::Core { &self.core }
    fn core_mut(&mut self) -> &mut cosmic::Core { &mut self.core }
    fn init(core: cosmic::Core, _: ()) -> (Self, Task<cosmic::Action<Message>>) {
        let mut app = Self { core, ..Self::default() };
        let task = app.refresh();
        (app, task)
    }
    fn on_close_requested(&self, id: Id) -> Option<Message> { Some(Message::PopupClosed(id)) }
    fn view(&self) -> Element<'_, Message> {
        let icon = widget::icon::from_svg_bytes(include_bytes!("../resources/icon.svg").as_slice()).symbolic(true);
        self.core.applet.icon_button_from_handle(icon).on_press(Message::TogglePopup).into()
    }
    fn view_window(&self, _: Id) -> Element<'_, Message> {
        use cosmic::iced::Length;

        let refresh = widget::button::text(fl!("refresh"));
        let refresh = if self.busy || self.refreshing { refresh } else { refresh.on_press(Message::Refresh) };
        let header = widget::Row::new()
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .push(widget::text::title4(fl!("app-title")).width(Length::Fill))
            .push(refresh);
        let state = match &self.status {
            Some(status) if status.running() => fl!("state-running"),
            Some(status) => match status.backend_state.as_str() {
                "Stopped" => fl!("state-stopped"), "NeedsLogin" => fl!("state-login"),
                "NeedsMachineAuth" => fl!("state-machine-auth"), "Starting" => fl!("state-starting"),
                other => format!("{}: {other}", fl!("state-unknown")),
            },
            None => fl!("state-loading"),
        };
        let mut connection = widget::Column::new().spacing(8).width(Length::Fill)
            .push(widget::text::title4(state).width(Length::Fill));
        if let Some(err) = &self.error { connection = connection.push(widget::text::body(err.clone()).width(Length::Fill)); }
        if let Some(err) = &self.action_error { connection = connection.push(widget::text::body(err.clone()).width(Length::Fill)); }
        if self.pending_login && self.status.as_ref().is_some_and(|status| matches!(status.backend_state.as_str(), "NeedsLogin" | "NeedsMachineAuth")) {
            connection = connection.push(widget::text::body(fl!("auth-pending")).width(Length::Fill));
        }
        if let Some(notice) = &self.notice { connection = connection.push(widget::text::body(notice.clone()).width(Length::Fill)); }
        if let Some(status) = &self.status {
            if status.running() {
                connection = connection.push(self.button(fl!("disconnect"), Message::Act(Operation::Down)));
            } else if status.auth_link().is_some() {
                connection = connection.push(self.button(fl!("authorize"), Message::Act(Operation::OpenAuth)));
            } else if status.backend_state == "Stopped" || status.backend_state == "NeedsLogin" {
                connection = connection.push(self.button(fl!("connect"), Message::Act(Operation::Up)));
            }
        }
        if self.operator_denied {
            connection = connection.push(widget::text::body(fl!("operator-warning")).width(Length::Fill))
                .push(self.button(fl!("authorize-operator"), Message::AuthorizeOperator));
        }
        let mut list = widget::Column::new().spacing(14).padding(16).width(Length::Fill)
            .push(header)
            .push(surface(connection));
        if let Some(status) = self.status.as_ref().filter(|status| status.running()) {
            let mut accounts = widget::Column::new().spacing(8).width(Length::Fill);
            if let Some(warning) = &self.warning {
                accounts = accounts.push(widget::text::body(warning.clone()).width(Length::Fill));
            }
            if self.accounts.is_empty() {
                accounts = accounts.push(widget::text::body(fl!("accounts-empty")).width(Length::Fill));
            }
            for account in &self.accounts {
                let prefix = if account.selected { "✓ " } else { "" };
                accounts = accounts.push(self.button(format!("{prefix}{}", account.label()), Message::Act(Operation::Switch(account.id.clone()))));
            }
            list = list.push(self.button(if self.accounts_open { fl!("hide-accounts") } else { fl!("show-accounts") }, Message::ToggleAccounts));
            if self.accounts_open { list = list.push(surface(accounts)); }

            let mut nodes = widget::Column::new().spacing(8).width(Length::Fill)
                .push(self.button(fl!("exit-none"), Message::Act(Operation::Exit(String::new()))));
            for (_, peer) in status.online_peers().filter(|(_, peer)| peer.exit_node_option) {
                if let Some(ip) = peer.tailscale_ips.first() {
                    nodes = nodes.push(self.button(format!("{} ({ip})", peer.name()), Message::Act(Operation::Exit(ip.clone()))));
                }
            }
            let mullvad: Vec<_> = self.exit_nodes.iter().filter(|node| node.hostname.ends_with(".mullvad.ts.net") && node.city != "Any").collect();
            if !mullvad.is_empty() { nodes = nodes.push(widget::text::heading(fl!("mullvad-regions"))); }
            for node in mullvad {
                nodes = nodes.push(self.button(node.region(), Message::Act(Operation::Exit(node.ip.clone()))));
            }
            list = list.push(self.button(if self.nodes_open { fl!("hide-exits") } else { fl!("show-exits") }, Message::ToggleNodes));
            if self.nodes_open { list = list.push(surface(nodes)); }

            list = list.push(widget::text::heading(fl!("devices")));
            let mut peers: Vec<_> = status.online_peers().collect();
            peers.sort_by(|(_, a), (_, b)| a.name().cmp(b.name()));
            if peers.is_empty() {
                list = list.push(surface(widget::text::body(fl!("devices-empty")).width(Length::Fill)));
            }
            for (id, peer) in peers {
                let expanded = self.expanded_peer.as_deref() == Some(id);
                let mut controls = widget::Column::new().spacing(8).width(Length::Fill)
                    .push(widget::Row::new().spacing(8).align_y(cosmic::iced::Alignment::Center)
                        .push(widget::text::body(format!("● {}", peer.name())).width(Length::Fill))
                        .push(widget::button::text(if expanded { fl!("hide-details") } else { fl!("details") }).on_press(Message::TogglePeer(id.clone()))));
                if expanded {
                    if let Some(ip) = peer.tailscale_ips.first() {
                        controls = controls
                            .push(widget::text::body(format!("IP: {ip}")).width(Length::Fill))
                            .push(self.button(fl!("copy-ip"), Message::Copy(ip.clone())));
                    }
                    if !peer.dns_name.is_empty() {
                        let dns = peer.dns_name.trim_end_matches('.');
                        controls = controls
                            .push(widget::text::body(format!("DNS: {dns}"))
                                .width(Length::Fill)
                                .wrapping(cosmic::iced::advanced::text::Wrapping::WordOrGlyph))
                            .push(self.button(fl!("copy-dns"), Message::Copy(dns.into())));
                    }
                    controls = controls.push(self.button(fl!("copy-name"), Message::Copy(peer.name().into())));
                    if status.file_sharing() && peer.can_receive() {
                        controls = controls.push(self.button(fl!("send-file"), Message::Send(id.clone())));
                    }
                }
                list = list.push(surface(controls));
            }
        }
        let body = widget::container(widget::scrollable(list).width(Length::Fill).height(Length::Shrink))
            .width(Length::Fixed(360.0))
            .max_height(620.0);
        self.core.applet.popup_container(body).into()
    }
    fn subscription(&self) -> Subscription<Message> {
        let ticks = time::every(Duration::from_secs(30)).map(|_| Message::Tick);
        if self.status.as_ref().is_some_and(Status::running) {
            Subscription::batch([ticks, Subscription::run(|| cosmic::iced::stream::channel(2, |mut sender: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                loop {
                    if sender.send(Message::Received(taildrop::receive_once().await)).await.is_err() { break; }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }))])
        } else { ticks }
    }
    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::Tick | Message::Refresh => return self.refresh(),
            Message::Refreshed(result) => {
                self.refreshing = false;
                match *result {
                    Ok((status, accounts, nodes, warning, denied)) => {
                        self.pending_login &= matches!(status.backend_state.as_str(), "NeedsLogin" | "NeedsMachineAuth");
                        if status.running() { self.operator_denied = denied; }
                        self.status = Some(status); self.accounts = accounts; self.exit_nodes = nodes;
                        self.warning = warning; self.error = None;
                    }
                    Err(error) => {
                        self.operator_denied = denied(&error);
                        self.pending_login = false;
                        self.notice = None;
                        self.error = Some(error_text(&error)); self.warning = None; self.status = None;
                    }
                }
            }
            Message::Act(action) => {
                if self.busy || self.refreshing { return Task::none(); }
                self.action_error = None;
                if matches!(action, Operation::Up | Operation::OpenAuth) { self.pending_login = true; }
                self.busy = true;
                let auth_url = self.status.as_ref().and_then(Status::auth_link).map(str::to_owned);
                return cosmic::task::future(async move {
                    let result = match &action {
                        Operation::Up => tailscale::connect().await,
                        Operation::Down => tailscale::run(&["down"]).await,
                        Operation::Switch(id) => tailscale::run(&["switch", id]).await,
                        Operation::Exit(value) => tailscale::run(&["set", &format!("--exit-node={value}")]).await,
                        Operation::OpenAuth => match auth_url {
                            Some(url) => {
                                let result = tokio::process::Command::new("xdg-open").arg(url).status().await;
                                match result { Ok(status) if status.success() => Ok(String::new()), Ok(status) => Err(CliError::Failed(status.to_string())), Err(e) => Err(CliError::Failed(e.to_string())) }
                            }
                            None => Err(CliError::InvalidData("invalid authorization URL".into())),
                        },
                    };
                    cosmic::Action::App(Message::Acted(result))
                });
            }
            Message::Acted(result) => {
                self.busy = false;
                match result {
                    Ok(_) => { self.notice = if self.pending_login { None } else { Some(fl!("action-complete")) }; self.action_error = None; },
                    Err(error) => { self.operator_denied |= denied(&error); self.pending_login = false; self.action_error = Some(error_text(&error)); self.notice = None; }
                }
                return self.refresh();
            }
            Message::AuthorizeOperator => {
                if self.busy || self.refreshing { return Task::none(); }
                self.busy = true;
                self.action_error = None;
                return cosmic::task::future(async { cosmic::Action::App(Message::OperatorAuthorized(authorization::enable_operator().await)) });
            }
            Message::OperatorAuthorized(result) => {
                self.busy = false;
                match result {
                    Ok(()) => { self.notice = Some(fl!("operator-enabled")); self.operator_denied = false; },
                    Err(AuthorizationError::HelperMissing) => self.action_error = Some(fl!("helper-missing")),
                    Err(AuthorizationError::Denied) => self.action_error = Some(fl!("operator-cancelled")),
                    Err(AuthorizationError::Timeout) => self.action_error = Some(fl!("operator-timeout")),
                    Err(AuthorizationError::Failed(error)) => self.action_error = Some(format!("{}: {error}", fl!("operator-failed"))),
                }
                return self.refresh();
            }
            Message::TogglePeer(id) => {
                self.expanded_peer = if self.expanded_peer.as_deref() == Some(&id) { None } else { Some(id) };
            }
            Message::ToggleAccounts => self.accounts_open = !self.accounts_open,
            Message::ToggleNodes => self.nodes_open = !self.nodes_open,
            Message::Send(id) => {
                if self.busy || self.refreshing { return Task::none(); }
                self.action_error = None;
                let Some(status) = self.status.as_ref().filter(|s| s.running() && s.file_sharing()) else { return Task::none(); };
                let Some(peer) = status.peer.get(&id).filter(|p| p.can_receive()) else { return Task::none(); };
                let destination = if peer.dns_name.is_empty() { peer.name().to_owned() } else { peer.dns_name.trim_end_matches('.').to_owned() };
                self.busy = true;
                let title = fl!("send-file");
                return cosmic::task::future(async move { cosmic::Action::App(Message::Sent(taildrop::choose_and_send(&destination, &title).await)) });
            }
            Message::Sent(result) => {
                self.busy = false;
                match result { Ok(Some(_)) => self.notice = Some(fl!("send-complete")), Ok(None) => {}, Err(error) => self.action_error = Some(error_text(&error)) }
                return self.refresh();
            }
            Message::Received(result) => match result {
                Ok(paths) if !paths.is_empty() => {
                    let title = fl!("received");
                    let body = paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
                    self.notice = Some(format!("{title}: {body}"));
                    return cosmic::task::future(async move {
                        use ashpd::desktop::notification::{Notification, NotificationProxy};
                        if let Ok(proxy) = NotificationProxy::new().await {
                            let _ = proxy.add_notification("taildrop-received", Notification::new(&title).body(body.as_str())).await;
                        }
                        cosmic::Action::App(Message::Notified)
                    });
                }
                Err(CliError::Timeout) | Ok(_) => {},
                Err(error) => self.error = Some(error_text(&error)),
            },
            Message::Notified => {},
            Message::Copy(value) => return clipboard::write(value).map(cosmic::Action::App),
            Message::TogglePopup => {
                return if let Some(id) = self.popup.take() { destroy_popup(id) } else {
                    let id = Id::unique(); self.popup = Some(id);
                    let mut settings = self.core.applet.get_popup_settings(self.core.main_window_id().unwrap(), id, None, None, None);
                    settings.positioner.size_limits = Limits::NONE.min_width(360.0).max_width(360.0).min_height(1.0).max_height(640.0);
                    get_popup(settings)
                };
            }
            Message::PopupClosed(id) => { if self.popup == Some(id) { self.popup = None; } }
        }
        Task::none()
    }
    fn style(&self) -> Option<cosmic::iced::theme::Style> { Some(cosmic::applet::style()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_auth_clears_on_running_and_refresh_failure() {
        let mut app = AppModel { pending_login: true, refreshing: true, ..Default::default() };
        let login = Status { backend_state: "NeedsLogin".into(), auth_url: "https://login.tailscale.com/a".into(), ..Default::default() };
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::Refreshed(Box::new(Ok((login, vec![], vec![], None, false)))));
        assert!(app.pending_login);
        let running = Status { backend_state: "Running".into(), ..Default::default() };
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::Refreshed(Box::new(Ok((running, vec![], vec![], None, false)))));
        assert!(!app.pending_login);
        app.pending_login = true;
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::Refreshed(Box::new(Err(CliError::Daemon))));
        assert!(!app.pending_login);
        assert!(app.status.is_none());
        assert!(app.error.is_some());
    }

    #[test]
    fn expanding_another_peer_closes_previous_and_sections_start_closed() {
        let mut app = AppModel::default();
        assert!(!app.accounts_open && !app.nodes_open && app.expanded_peer.is_none());
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::TogglePeer("peer-a".into()));
        assert_eq!(app.expanded_peer.as_deref(), Some("peer-a"));
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::TogglePeer("peer-b".into()));
        assert_eq!(app.expanded_peer.as_deref(), Some("peer-b"));
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::TogglePeer("peer-b".into()));
        assert!(app.expanded_peer.is_none());
    }

    #[test]
    fn denied_connect_remains_actionable_after_stopped_refresh() {
        let mut app = AppModel { busy: true, ..Default::default() };
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::Acted(Err(CliError::AccessDenied)));
        assert!(app.operator_denied);
        let stopped = Status { backend_state: "Stopped".into(), ..Default::default() };
        let _ = <AppModel as cosmic::Application>::update(&mut app, Message::Refreshed(Box::new(Ok((stopped, vec![], vec![], None, false)))));
        assert!(app.operator_denied);
    }
}
