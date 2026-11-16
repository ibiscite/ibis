use std::ffi::OsStr;
#[cfg(not(target_os = "macos"))]
use std::path::Path;
#[cfg(not(windows))]
use std::process::{Command, Stdio};

pub(crate) fn open(target: &OsStr, reveal: bool) -> Result<(), ()> {
    #[cfg(not(windows))]
    return run(&mut command(target, reveal)?);
    #[cfg(windows)]
    {
        use windows::{
            Foundation::Uri,
            Storage::{IStorageItem, StorageFile, StorageFolder},
            System::{FolderLauncherOptions, Launcher},
            core::{HSTRING, Interface},
        };
        let fault = |_| ();
        let opened = if Path::new(target).is_absolute() {
            let path = dunce::simplified(Path::new(target));
            let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.to_str().ok_or(())?))
                .map_err(fault)?
                .join()
                .map_err(fault)?;
            if reveal {
                let parent = path.parent().and_then(Path::to_str).ok_or(())?;
                let folder = StorageFolder::GetFolderFromPathAsync(&HSTRING::from(parent))
                    .map_err(fault)?
                    .join()
                    .map_err(fault)?;
                let options = FolderLauncherOptions::new().map_err(fault)?;
                options
                    .ItemsToSelect()
                    .map_err(fault)?
                    .Append(&file.cast::<IStorageItem>().map_err(fault)?)
                    .map_err(fault)?;
                Launcher::LaunchFolderWithOptionsAsync(&folder, &options)
                    .map_err(fault)?
                    .join()
                    .map_err(fault)?
            } else {
                Launcher::LaunchFileAsync(&file).map_err(fault)?.join().map_err(fault)?
            }
        } else {
            let uri = Uri::CreateUri(&HSTRING::from(target.to_str().ok_or(())?)).map_err(fault)?;
            Launcher::LaunchUriAsync(&uri).map_err(fault)?.join().map_err(fault)?
        };
        if opened { Ok(()) } else { Err(()) }
    }
}

#[cfg(not(windows))]
fn command(target: &OsStr, reveal: bool) -> Result<Command, ()> {
    let zotero = target.to_str().is_some_and(|uri| uri.starts_with("zotero:"));
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        if zotero {
            command.args(["-b", "org.zotero.zotero"]);
        } else if reveal {
            command.arg("-R");
        }
        command.arg("--");
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = Command::new("xdg-open");
    #[cfg(not(target_os = "macos"))]
    let target = if reveal && !zotero {
        Path::new(target).parent().ok_or(())?.as_os_str()
    } else {
        target
    };
    command.arg(target);
    Ok(command)
}

#[cfg(not(windows))]
fn run(command: &mut Command) -> Result<(), ()> {
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| ())?;
    status.success().then_some(()).ok_or(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn failed_launches_return_only_failure() {
        assert!(run(&mut Command::new("true")).is_ok());
        for mut command in [Command::new("false"), Command::new("/invented/unavailable-launcher")] {
            assert_eq!(run(&mut command), Err(()));
        }
    }

    #[test]
    fn launcher_suppresses_diagnostics_and_preserves_failure() -> Result<(), Box<dyn std::error::Error>> {
        const CHILD: &str = "IBIS_TEST_SYSTEM_OPEN_CHILD";
        if std::env::var_os(CHILD).is_some() {
            assert_eq!(
                run(Command::new("/bin/sh").args([
                    "-c",
                    "printf synthetic-opener-output; printf synthetic-opener-detail >&2; exit 7"
                ])),
                Err(())
            );
            return Ok(());
        }
        let output = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "system_open::tests::launcher_suppresses_diagnostics_and_preserves_failure",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()?;
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-opener"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-opener"));
        Ok(())
    }

    #[test]
    fn paths_and_urls_are_literal_and_reveal_keeps_the_native_behavior() {
        let path = Path::new("/invented/-reference ; $(touch nope) & 'library'.bib");
        let open = command(path.as_os_str(), false).unwrap_or_else(|()| panic!("file"));
        assert_eq!(open.get_args().last(), Some(path.as_os_str()));
        let url = "https://example.invalid/?a=1&b=$(touch%20nope)";
        let browser = command(OsStr::new(url), false).unwrap_or_else(|()| panic!("url"));
        assert_eq!(browser.get_args().last(), Some(OsStr::new(url)));
        let reveal = command(path.as_os_str(), true).unwrap_or_else(|()| panic!("reveal"));
        let zotero = command(OsStr::new("zotero://select/library"), true).unwrap_or_else(|()| panic!("Zotero"));
        #[cfg(target_os = "macos")]
        {
            for command in [&open, &browser, &reveal, &zotero] {
                assert_eq!(command.get_program(), "/usr/bin/open");
                let args = command.get_args().collect::<Vec<_>>();
                assert_eq!(args[args.len() - 2], "--");
            }
            assert_eq!(
                reveal.get_args().collect::<Vec<_>>(),
                [OsStr::new("-R"), OsStr::new("--"), path.as_os_str()]
            );
            assert_eq!(
                zotero.get_args().collect::<Vec<_>>(),
                [
                    OsStr::new("-b"),
                    OsStr::new("org.zotero.zotero"),
                    OsStr::new("--"),
                    OsStr::new("zotero://select/library")
                ]
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(reveal.get_program(), "xdg-open");
            assert_eq!(reveal.get_args().last(), path.parent().map(Path::as_os_str));
            assert_eq!(zotero.get_args().last(), Some(OsStr::new("zotero://select/library")));
        }
    }
}
