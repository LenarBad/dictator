pub fn show(title: &str, message: &str) {
    crate::platform::notify(title, message);
}
