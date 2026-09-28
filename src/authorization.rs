// SPDX-License-Identifier: MIT

#[derive(Debug, Clone)]
pub enum AuthorizationError {
    HelperMissing,
    Denied,
    Timeout,
    Failed(String),
}

pub async fn enable_operator() -> Result<(), AuthorizationError> {
    let connection = zbus::Connection::system()
        .await
        .map_err(|error| AuthorizationError::Failed(error.to_string()))?;
    let proxy = zbus::Proxy::new(
        &connection,
        "io.github.chispes.CosmicTailscale.Helper",
        "/io/github/chispes/CosmicTailscale/Helper",
        "io.github.chispes.CosmicTailscale.Helper",
    )
    .await
    .map_err(classify)?;
    proxy.call("EnableOperator", &()).await.map_err(classify)
}

fn classify(error: zbus::Error) -> AuthorizationError {
    let text = error.to_string();
    if text.contains("ServiceUnknown") || text.contains("NameHasNoOwner") {
        AuthorizationError::HelperMissing
    } else if text.contains("Denied") || text.contains("NotAuthorized") || text.contains("Cancelled") {
        AuthorizationError::Denied
    } else if text.contains("Timeout") || text.contains("NoReply") {
        AuthorizationError::Timeout
    } else {
        AuthorizationError::Failed(text)
    }
}
