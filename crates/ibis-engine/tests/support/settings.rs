use ibis_engine::settings::{self, Settings, SettingsError};
use std::path::{Path, PathBuf};

pub fn file() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = directory.path().join("settings.json");
    (directory, path)
}

pub fn read(path: &Path) -> Settings {
    settings::load(path)
        .cites()
        .unwrap_or_else(|error| panic!("{error}"))
        .clone()
}

pub fn edit(path: &Path, change: impl FnOnce(&mut Settings)) -> Result<Settings, SettingsError> {
    settings::Owner::open(path.to_path_buf())
        .change(|settings| {
            change(settings);
            Ok::<_, SettingsError>(())
        })
        .map(|(settings, ())| settings)
}
