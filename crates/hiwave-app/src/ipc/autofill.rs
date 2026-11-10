//! Autofill hardening: credentials are delivered only for a user-requested
//! fill on the page's own origin.

#[cfg(test)]
mod tests {
    use crate::ipc::{commands, IpcMessage, IpcResponse};
    use crate::state::{AppConfig, AppState};
    use std::sync::{Arc, Mutex};
    use url::Url;

    const SECRET: &str = "s3cret-pw-for-tests";

    fn state_with_vault(dir: &std::path::Path) -> AppState {
        let config = AppConfig {
            data_dir: dir.to_path_buf(),
            download_dir: dir.join("downloads"),
            ..Default::default()
        };
        let mut state = AppState::new(config).expect("app state in a temp dir");
        state.vault.unlock("master").expect("unlock");
        state
            .vault
            .save_credential(&Url::parse("https://a.example/login").unwrap(), "alice", SECRET)
            .expect("save");
        state
            .vault
            .save_credential(&Url::parse("https://b.example/login").unwrap(), "bob", "other-secret")
            .expect("save");
        state
    }

    /// A request that reaches the shared command handler (the path a page's
    /// own message took) is never answered with credential material.
    #[test]
    fn an_unsolicited_request_gets_no_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(state_with_vault(dir.path())));
        let response = commands::handle_message(
            &state,
            IpcMessage::GetCredentialsForAutofill { domain: "a.example".into() },
        );
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains(SECRET), "response carried a password: {json}");
        assert!(!json.contains("alice"), "response carried a username: {json}");
        assert!(matches!(response, IpcResponse::Error { .. }), "{json}");
    }

    const HELPER: &str = include_str!("../ui/autofill.js");

    /// The helper takes its list only from the app: no window message
    /// listener, and it does not stand down for a pre-existing global.
    #[test]
    fn the_page_helper_takes_its_list_only_from_the_app() {
        assert!(!HELPER.contains("addEventListener('message'"), "helper listens for window messages");
        assert!(!HELPER.contains("addEventListener(\"message\""), "helper listens for window messages");
        assert!(!HELPER.contains("if (window.hiwaveAutofill) return"), "helper skips setup for an existing global");
        assert!(!HELPER.contains(".password"), "helper handles credential secrets");
    }
}
