use super::{Warning, open_configuration};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn opening_a_replaced_configuration_does_not_wait_for_a_fifo_writer() {
    let directory = std::env::temp_dir().join(format!("ibis-discovery-fifo-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap_or_else(|_| panic!("create synthetic fifo sandbox"));
    let fifo = directory.join("prefs.js");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap_or_else(|_| panic!("create synthetic fifo"));
    assert!(status.success());
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = sender.send(open_configuration(&fifo).err());
    });
    // Bound this regression even if the protective open flag is removed.
    let result = receiver.recv_timeout(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(directory);
    assert_eq!(result.ok(), Some(Some(Warning::NotRegularFile)));
    assert!(worker.join().is_ok());
}

#[test]
fn opening_a_replaced_configuration_does_not_follow_a_symlink() {
    let directory = std::env::temp_dir().join(format!("ibis-discovery-open-link-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap_or_else(|_| panic!("create synthetic link sandbox"));
    let target = directory.join("target");
    std::fs::write(&target, "// synthetic").unwrap_or_else(|_| panic!("write synthetic target"));
    let link = directory.join("prefs.js");
    std::os::unix::fs::symlink(&target, &link).unwrap_or_else(|_| panic!("create synthetic link"));
    let result = open_configuration(&link);
    let _ = std::fs::remove_dir_all(directory);
    assert_eq!(result.err(), Some(Warning::UnreadableConfiguration));
}
