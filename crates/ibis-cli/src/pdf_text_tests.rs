use super::*;

#[test]
fn resource_limits_apply_in_an_isolated_child() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pdf_text::tests::resource_limits_helper",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "subprocess helper; lowers limits only in its own process"]
fn resource_limits_helper() {
    use nix::sys::resource::{Resource, getrlimit};
    limit_resources().unwrap();
    let (soft, hard) = getrlimit(Resource::RLIMIT_CPU).unwrap();
    assert!(soft <= MAX_CPU_SECONDS && hard <= MAX_CPU_SECONDS);
    assert_eq!(getrlimit(Resource::RLIMIT_CORE).unwrap(), (0, 0));
    #[cfg(target_os = "linux")]
    {
        let (soft, hard) = getrlimit(Resource::RLIMIT_AS).unwrap();
        assert!(soft <= MAX_MEMORY_BYTES && hard <= MAX_MEMORY_BYTES);
    }
}
