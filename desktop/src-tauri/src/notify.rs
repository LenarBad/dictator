pub fn show(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        let title = serde_json::to_string(title).unwrap_or_else(|_| "\"Dictator\"".into());
        let message = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
        // osascript can hang on Automation prompts. Never wait for it on the
        // caller's thread — that used to freeze the Carbon hotkey callback.
        let _ = std::thread::Builder::new()
            .name("dictator-notify".into())
            .spawn(move || {
                let script = format!("display notification {message} with title {title}");
                let _ = std::process::Command::new("osascript")
                    .args(["-e", &script])
                    .status();
            });
        return;
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("dictator: {title}: {message}");
    }
}
