// SPDX-License-Identifier: MIT
use crate::{fl, taildrop, tailscale::{self, Account, CliError, ExitNode, Status}};
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
    notice: Option<String>,
    busy: bool,
    refreshing: bool,
    pending_login: bool,
}

#[derive(Debug, Clone)]
pub enum Operation { Up, Down, Switch(String), Exit(String), OpenAuth }

type Loaded = (Status, Vec<Account>, Vec<ExitNode>, Option<String>);

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup, PopupClosed(Id), Tick, Refresh,
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
    let (accounts, account_error) = match tailscale::accounts().await {
        Ok(items) => (items, None), Err(err) => (Vec::new(), Some(error_text(&err))),
    };
    let (nodes, exit_error) = match tailscale::exit_nodes().await {
        Ok(items) => (items, None), Err(err) => (Vec::new(), Some(error_text(&err))),
    };
    let warning = match (account_error, exit_error) {
        (Some(account), Some(exit)) if account != exit => Some(format!("{account}; {exit}")),
        (Some(error), _) | (_, Some(error)) => Some(error),
        (None, None) => None,
    };
    Ok((status, accounts, nodes, warning))
}

impl AppModel {
    fn refresh(&mut self) -> Task<cosmic::Action<Message>> {
        if self.refreshing || self.busy { return Task::none(); }
        self.refreshing = true;
        cosmic::task::future(async { cosmic::Action::App(Message::Refreshed(Box::new(load().await))) })
    }
    fn button(&self, label: String, action: Message) -> Element<'_, Message> {
        let button = widget::button::text(label);
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
        let icon = if self.error.is_some() { "dialog-error-symbolic" } else if self.status.as_ref().is_some_and(Status::running) { "network-vpn-symbolic" } else { "network-vpn-disconnected-symbolic" };
        self.core.applet.icon_button(icon).on_press(Message::TogglePopup).into()
    }
    fn view_window(&self, _: Id) -> Element<'_, Message> {
        let mut list = widget::list_column()
            .add(widget::text::heading(fl!("app-title")))
            .add(widget::text::body(match &self.status {
                Some(status) if status.running() => fl!("state-running"),
                Some(status) => match status.backend_state.as_str() {
                    "Stopped" => fl!("state-stopped"), "NeedsLogin" => fl!("state-login"),
                    "NeedsMachineAuth" => fl!("state-machine-auth"), "Starting" => fl!("state-starting"),
                    other => format!("{}: {other}", fl!("state-unknown")),
                },
                None => fl!("state-loading"),
            }));
        if let Some(err) = &self.error { list = list.add(widget::text::body(err.clone())); }
        if let Some(notice) = &self.notice { list = list.add(widget::text::body(notice.clone())); }
        if let Some(status) = &self.status {
            if status.running() {
                list = list.add(self.button(fl!("disconnect"), Message::Act(Operation::Down)));
            } else if let Some(url) = status.auth_link() {
                if !url.is_empty() { list = list.add(self.button(fl!("authorize"), Message::Act(Operation::OpenAuth))); }
            } else if status.backend_state == "Stopped" || status.backend_state == "NeedsLogin" {
                list = list.add(self.button(fl!("connect"), Message::Act(Operation::Up)));
            }
            if status.running() {
                list = list.add(widget::text::title3(fl!("accounts")));
                for account in &self.accounts {
                    let prefix = if account.selected { "✓ " } else { "" };
                    list = list.add(self.button(format!("{prefix}{}", account.label()), Message::Act(Operation::Switch(account.id.clone()))));
                }
                list = list.add(widget::text::title3(fl!("exit-nodes")))
                    .add(self.button(fl!("exit-none"), Message::Act(Operation::Exit(String::new()))));
                for (_, peer) in status.online_peers().filter(|(_, p)| p.exit_node_option) {
                    if let Some(ip) = peer.tailscale_ips.first() {
                        list = list.add(self.button(format!("{} ({ip})", peer.name()), Message::Act(Operation::Exit(ip.clone()))));
                    }
                }
                let mullvad: Vec<_> = self.exit_nodes.iter().filter(|node| node.hostname.ends_with(".mullvad.ts.net") && node.city != "Any").collect();
                if !mullvad.is_empty() { list = list.add(widget::text::title3(fl!("mullvad-regions"))); }
                for node in mullvad { list = list.add(self.button(node.region(), Message::Act(Operation::Exit(node.ip.clone())))); }
                list = list.add(widget::text::title3(fl!("devices")));
                let mut peers: Vec<_> = status.online_peers().collect();
                peers.sort_by(|(_, a), (_, b)| a.name().cmp(b.name()));
                for (id, peer) in peers {
                    list = list.add(widget::text::body(peer.name().to_owned()));
                    if let Some(ip) = peer.tailscale_ips.first() { list = list.add(self.button(format!("{}: {ip}", fl!("copy-ip")), Message::Copy(ip.clone()))); }
                    if !peer.dns_name.is_empty() { list = list.add(self.button(format!("{}: {}", fl!("copy-dns"), peer.dns_name.trim_end_matches('.')), Message::Copy(peer.dns_name.trim_end_matches('.').into()))); }
                    list = list.add(self.button(fl!("copy-name"), Message::Copy(peer.name().into())));
                    if status.file_sharing() && peer.can_receive(status.self_node.user_id) {
                        list = list.add(self.button(fl!("send-file"), Message::Send(id.clone())));
                    }
                }
            }
        }
        list = list.add(self.button(fl!("refresh"), Message::Refresh));
        self.core.applet.popup_container(widget::scrollable(list).height(550)).into()
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
                    Ok((status, accounts, nodes, warning)) => {
                        if status.running() { self.pending_login = false; }
                        self.status = Some(status); self.accounts = accounts; self.exit_nodes = nodes; self.error = warning;
                    }
                    Err(error) => { self.error = Some(error_text(&error)); self.status = None; }
                }
            }
            Message::Act(action) => {
                if self.busy || self.refreshing { return Task::none(); }
                self.busy = true;
                if matches!(action, Operation::Up | Operation::OpenAuth) { self.pending_login = true; }
                let auth_url = self.status.as_ref().and_then(Status::auth_link).map(str::to_owned);
                return cosmic::task::future(async move {
                    let result = match &action {
                        Operation::Up => tailscale::run(&["up"]).await,
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
                match result { Ok(_) => { self.notice = Some(if self.pending_login { fl!("auth-pending") } else { fl!("action-complete") }); self.error = None; }, Err(error) => { self.pending_login = false; self.error = Some(error_text(&error)); self.notice = None; } }
                return self.refresh();
            }
            Message::Send(id) => {
                if self.busy || self.refreshing { return Task::none(); }
                let Some(status) = self.status.as_ref().filter(|s| s.running() && s.file_sharing()) else { return Task::none(); };
                let Some(peer) = status.peer.get(&id).filter(|p| p.can_receive(status.self_node.user_id)) else { return Task::none(); };
                let destination = if peer.dns_name.is_empty() { peer.name().to_owned() } else { peer.dns_name.trim_end_matches('.').to_owned() };
                self.busy = true;
                let title = fl!("send-file");
                return cosmic::task::future(async move { cosmic::Action::App(Message::Sent(taildrop::choose_and_send(&destination, &title).await)) });
            }
            Message::Sent(result) => {
                self.busy = false;
                match result { Ok(Some(_)) => self.notice = Some(fl!("send-complete")), Ok(None) => {}, Err(error) => self.error = Some(error_text(&error)) }
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
                    settings.positioner.size_limits = Limits::NONE.max_width(372.0).min_width(300.0).min_height(200.0).max_height(1080.0);
                    get_popup(settings)
                };
            }
            Message::PopupClosed(id) => { if self.popup == Some(id) { self.popup = None; } }
        }
        Task::none()
    }
    fn style(&self) -> Option<cosmic::iced::theme::Style> { Some(cosmic::applet::style()) }
}
