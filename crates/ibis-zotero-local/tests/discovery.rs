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
        std::fs::create_dir_all(path.parent().unwrap_or(&self.0)).unwrap_or_else(|_| panic!("create synthetic parent"));
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
        (
            Platform::MacOs,
            include_str!("../fixtures/discovery/macos-prefs.js"),
            Some("/Synthetic/研究/Library \"A\""),
        ),
        (
            Platform::Windows,
            include_str!("../fixtures/discovery/windows-prefs.js"),
            Some("Z:\\Synthetic\\研究\\Library"),
        ),
        (
            Platform::Linux,
            include_str!("../fixtures/discovery/linux-prefs.js"),
            None,
        ),
    ] {
        let sandbox = Sandbox::new();
        let appdata = sandbox.0.join("roaming");
        let root = profile_root(platform, &sandbox.0, &appdata);
        sandbox.write(&root.join("profiles.ini"), "[General]\nStartWithLastProfile=1\n[Profile0]\nName=synthetic\nIsRelative=1\nPath=Profiles/test.default\nDefault=1\n");
        sandbox.write(&root.join("Profiles/test.default/prefs.js"), prefs);
        let result = discover(&DiscoveryRoots::new(platform, sandbox.0.clone(), Some(appdata)), None);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(
            result.directories[0].path(),
            expected
                .map(PathBuf::from)
                .as_deref()
                .unwrap_or(&sandbox.0.join("Zotero"))
        );
        assert_eq!(
            result.directories[0].origin(),
            if expected.is_some() {
                Origin::Preferences
            } else {
                Origin::Default
            }
        );
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
fn windows_manual_override_requires_a_full_drive_or_unc_path() {
    let roots = DiscoveryRoots::new(Platform::Windows, PathBuf::from("/synthetic"), None);
    for text in [
        "\\\\",
        "\\\\server",
        "\\\\server\\",
        "Z:relative",
        "\\rooted",
        "\\\\?\\Z:relative",
        "\\\\?\\UNC\\server",
        "UNC\\server\\share",
    ] {
        let result = discover(&roots, Some(Path::new(text)));
        assert!(result.directories.is_empty());
        assert_eq!(result.warnings, [Warning::InvalidPath]);
    }
    for text in [
        "Z:\\synthetic",
        "\\\\server\\share\\synthetic",
        "\\\\?\\Z:\\synthetic",
        "\\\\?\\UNC\\server\\share\\synthetic",
    ] {
        let result = discover(&roots, Some(Path::new(text)));
        assert!(result.warnings.is_empty());
        assert_eq!(result.directories[0].path(), Path::new(text));
    }
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

#[test]
fn all_relative_and_absolute_profiles_are_considered_and_shared_directories_deduplicated() {
    let sandbox = Sandbox::new();
    let root = sandbox.0.join(".zotero/zotero");
    let external = sandbox.0.join("external");
    let custom = sandbox.0.join("data");
    sandbox.write(&root.join("profiles.ini"), &format!("[Profile0]\nIsRelative=1\nPath=first\nDefault=1\n[Profile1]\nIsRelative=0\nPath={}\n[Profile2]\nIsRelative=1\nPath=third\n", external.display()));
    let prefs = format!(
        "user_pref(\"extensions.zotero.useDataDir\", true);\nuser_pref(\"extensions.zotero.dataDir\", \"{}\");",
        custom.to_string_lossy().replace('\\', "\\\\")
    );
    sandbox.write(&root.join("first/prefs.js"), &prefs);
    sandbox.write(&external.join("prefs.js"), &prefs);
    sandbox.write(&root.join("third/prefs.js"), "// default");
    let result = discover(&DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None), None);
    assert!(result.warnings.is_empty());
    assert_eq!(
        result.directories.iter().map(|d| d.path()).collect::<Vec<_>>(),
        [custom.as_path(), &sandbox.0.join("Zotero")]
    );
}

#[test]
fn malformed_preferences_do_not_guess_another_library_and_other_profiles_still_load() {
    let sandbox = Sandbox::new();
    let root = sandbox.0.join(".zotero/zotero");
    sandbox.write(
        &root.join("profiles.ini"),
        "[Profile0]\nIsRelative=1\nPath=broken\n[Profile1]\nIsRelative=1\nPath=good\n",
    );
    sandbox.write(
        &root.join("broken/prefs.js"),
        "user_pref(\"extensions.zotero.useDataDir\", true);",
    );
    sandbox.write(&root.join("good/prefs.js"), "// empty defaults");
    let roots = DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None);
    let result = discover(&roots, None);
    assert_eq!(result.warnings, [Warning::UnsupportedPreferences]);
    assert_eq!(result.directories.len(), 1);
    assert_eq!(result.directories[0].origin(), Origin::Default);
    sandbox.write(&root.join("good/prefs.js"), "eval('never execute preferences')");
    let result = discover(&roots, None);
    assert_eq!(result.warnings, [Warning::UnsupportedPreferences]);
    assert!(result.directories.is_empty());
}

#[test]
fn profile_registry_rejects_ambiguous_paths_and_over_limit_profile_counts() {
    let sandbox = Sandbox::new();
    let registry = sandbox.0.join(".zotero/zotero/profiles.ini");
    let roots = DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None);
    for text in [
        "",
        "[General]\nStartWithLastProfile=1\n",
        "[Profile0]\nPath=first\n",
        "[Profile0]\nIsRelative=1\nPath=../escape\n",
        "[Profile0]\nIsRelative=0\nPath=relative\n",
        "[Profile0]\nIsRelative=1\nPath=first\nPath=second\n",
    ] {
        sandbox.write(&registry, text);
        let result = discover(&roots, None);
        assert_eq!(result.warnings, [Warning::InvalidProfiles]);
        assert!(result.directories.is_empty());
    }
    let text = (0..65)
        .map(|i| format!("[Profile{i}]\nIsRelative=1\nPath=profile{i}\n"))
        .collect::<String>();
    sandbox.write(&registry, &text);
    assert_eq!(discover(&roots, None).warnings, [Warning::TooManyProfiles]);
}

#[test]
fn oversized_and_nonregular_configuration_is_rejected_with_fixed_diagnostics() {
    let sandbox = Sandbox::new();
    let root = sandbox.0.join(".zotero/zotero");
    let roots = DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None);
    sandbox.write(&root.join("profiles.ini"), &"x".repeat(64 * 1024 + 1));
    assert_eq!(discover(&roots, None).warnings, [Warning::ConfigurationTooLarge]);
    sandbox.write(&root.join("profiles.ini"), "[Profile0]\nIsRelative=1\nPath=first\n");
    sandbox.write(&root.join("first/prefs.js"), &"x".repeat(1024 * 1024 + 1));
    assert_eq!(discover(&roots, None).warnings, [Warning::ConfigurationTooLarge]);
    std::fs::remove_file(root.join("first/prefs.js")).unwrap_or_else(|_| panic!("remove owned fixture"));
    std::fs::create_dir(root.join("first/prefs.js")).unwrap_or_else(|_| panic!("create owned fixture directory"));
    let result = discover(&roots, None);
    assert_eq!(result.warnings, [Warning::NotRegularFile]);
    assert!(!format!("{} {result:?}", result.warnings[0]).contains(&sandbox.0.to_string_lossy().to_string()));
}

#[cfg(unix)]
#[test]
fn symlinked_preferences_are_rejected_before_opening_the_target() {
    let sandbox = Sandbox::new();
    let root = sandbox.0.join(".zotero/zotero");
    sandbox.write(&root.join("profiles.ini"), "[Profile0]\nIsRelative=1\nPath=first\n");
    std::fs::create_dir(root.join("first")).unwrap_or_else(|_| panic!("create owned fixture profile"));
    std::os::unix::fs::symlink(sandbox.0.join("absent"), root.join("first/prefs.js"))
        .unwrap_or_else(|_| panic!("create owned fixture link"));
    let result = discover(&DiscoveryRoots::new(Platform::Linux, sandbox.0.clone(), None), None);
    assert_eq!(result.warnings, [Warning::NotRegularFile]);
    assert!(result.directories.is_empty());
}

#[test]
fn cancelled_discovery_returns_no_partial_candidate_list() {
    let sandbox = Sandbox::new();
    let appdata = sandbox.0.join("roaming");
    let roots = DiscoveryRoots::new(Platform::Windows, sandbox.0.clone(), Some(appdata.clone()));
    sandbox.write(
        &profile_root(Platform::Windows, &sandbox.0, &appdata).join("profiles.ini"),
        "[Profile0]\nIsRelative=1\nPath=profile\n",
    );
    let checks = AtomicUsize::new(0);
    let found =
        ibis_zotero_local::discovery::discover_until(&roots, None, &|| checks.fetch_add(1, Ordering::SeqCst) >= 2);
    assert!(found.is_none());
    assert_eq!(checks.load(Ordering::SeqCst), 3);
}
