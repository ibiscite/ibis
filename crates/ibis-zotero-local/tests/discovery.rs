use ibis_zotero_local::discovery::{DiscoveryRoots, Origin, Platform, Warning, discover};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ibis-discovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap_or_else(|_| panic!("create synthetic sandbox"));
        Self(path)
    }

    fn write(&self, path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap_or(&self.0))
            .unwrap_or_else(|_| panic!("create synthetic parent"));
        std::fs::write(path, contents).unwrap_or_else(|_| panic!("write synthetic file"));
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn profile_root(platform: Platform, home: &Path, appdata: &Path) -> PathBuf {
    match platform {
        Platform::MacOs => home.join("Library/Application Support/Zotero"),
        Platform::Windows => appdata.join("Zotero/Zotero"),
        Platform::Linux => home.join(".zotero/zotero"),
    }
}

#[test]
fn platform_preferences_and_default_are_discovered_from_synthetic_roots() {
    for (platform, prefs, expected) in [
        (Platform::MacOs, include_str!("../fixtures/discovery/macos-prefs.js"), Some("/Synthetic/研究/Library \"A\"")),
        (Platform::Windows, include_str!("../fixtures/discovery/windows-prefs.js"), Some("Z:\\Synthetic\\研究\\Library")),
        (Platform::Linux, include_str!("../fixtures/discovery/linux-prefs.js"), None),
    ] {
        let sandbox = Sandbox::new();
        let appdata = sandbox.0.join("roaming");
        let root = profile_root(platform, &sandbox.0, &appdata);
        sandbox.write(&root.join("profiles.ini"), "[General]\nStartWithLastProfile=1\n[Profile0]\nName=synthetic\nIsRelative=1\nPath=Profiles/test.default\nDefault=1\n");
        sandbox.write(&root.join("Profiles/test.default/prefs.js"), prefs);
        let result = discover(&DiscoveryRoots::new(platform, sandbox.0.clone(), Some(appdata)), None);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.directories[0].path(), expected.map(PathBuf::from).as_deref().unwrap_or(&sandbox.0.join("Zotero")));
        assert_eq!(result.directories[0].origin(), if expected.is_some() { Origin::Preferences } else { Origin::Default });
    }
}

#[test]
fn manual_override_bypasses_even_broken_profile_configuration() {
    let sandbox = Sandbox::new();
    let roots = DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None);
    sandbox.write(&sandbox.0.join(".zotero/zotero/profiles.ini"), "broken");
    let manual = sandbox.0.join("manual");
    let result = discover(&roots, Some(&manual));
    assert!(result.warnings.is_empty());
    assert_eq!(result.directories.len(), 1);
    assert_eq!(result.directories[0].path(), manual);
    assert_eq!(result.directories[0].origin(), Origin::Manual);
    let relative = discover(&roots, Some(Path::new("relative")));
    assert!(relative.directories.is_empty());
    assert_eq!(relative.warnings, [Warning::InvalidPath]);
}

#[test]
fn missing_registry_uses_the_platform_data_default() {
    let sandbox = Sandbox::new();
    let result = discover(&DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None), None);
    assert!(result.warnings.is_empty());
    assert_eq!(result.directories.len(), 1);
    assert_eq!(result.directories[0].path(), sandbox.0.join("Zotero"));
}

#[test]
fn paths_are_redacted_from_debug_and_diagnostics() {
    let secret = PathBuf::from("/synthetic-do-not-print");
    let roots = DiscoveryRoots::new(Platform::Linux, secret.clone(), None);
    let result = discover(&roots, Some(&secret));
    let debug = format!("{roots:?} {result:?}");
    assert!(!debug.contains("synthetic-do-not-print"));
    assert!(debug.contains("redacted"));
}
