use ibis_core::atomic_file::{AtomicFile, create_numbered, create_private, open_private};
use std::fs;
use std::io::{self, Write};

#[test]
fn atomic_create_refuses_a_destination_that_appears_during_staging() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invented.json");
    let staged = AtomicFile::new(&path, b"replacement")?;
    fs::write(&path, b"keep")?;
    assert_eq!(
        staged.create().err().map(|error| error.kind()),
        Some(io::ErrorKind::AlreadyExists)
    );
    assert_eq!(fs::read(&path)?, b"keep");
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    Ok(())
}

#[test]
fn private_creation_refuses_existing_files_and_reopening_keeps_bytes_and_permissions() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invented.lock");
    create_private(&path)?.write_all(b"keep")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
    }
    assert_eq!(
        create_private(&path).err().map(|error| error.kind()),
        Some(io::ErrorKind::AlreadyExists)
    );
    drop(open_private(&path)?);
    assert_eq!(fs::read(&path)?, b"keep");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o640);
    }
    Ok(())
}

#[test]
fn numbered_creation_skips_existing_files_without_overwriting() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invented.json");
    fs::write(path.with_extension("json.invalid-1"), b"keep")?;
    fs::write(path.with_extension("json.invalid-2"), b"also keep")?;
    let (chosen, mut file) = create_numbered(&path, ".invalid")?;
    assert_eq!(chosen, path.with_extension("json.invalid-3"));
    file.write_all(b"new")?;
    assert_eq!(fs::read(path.with_extension("json.invalid-1"))?, b"keep");
    assert_eq!(fs::read(path.with_extension("json.invalid-2"))?, b"also keep");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn private_files_and_locks_support_long_paths() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut parent = directory.path().to_path_buf();
    for _ in 0..3 {
        parent.push("invented".repeat(12));
    }
    fs::create_dir_all(&parent)?;
    let path = parent.join("settings.lock");
    create_private(&path)?.write_all(b"keep")?;
    drop(open_private(&path)?);
    assert_eq!(fs::read(&path)?, b"keep");
    AtomicFile::new(&path, b"replacement")?.replace()?;
    assert_eq!(fs::read(&path)?, b"replacement");
    Ok(())
}
