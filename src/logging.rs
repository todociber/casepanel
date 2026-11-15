pub fn write(message: &str) {
    let root = crate::config::default_path()
        .parent()
        .unwrap()
        .to_path_buf();
    if std::fs::create_dir_all(&root).is_err() {
        return;
    }
    let p = root.join("sensorpanel.log");
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > 1024 * 1024) {
        let _ = std::fs::rename(&p, root.join("sensorpanel.previous.log"));
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(
            f,
            "{} {}",
            crate::model::now_ms(),
            message.chars().take(2048).collect::<String>()
        );
    }
}
