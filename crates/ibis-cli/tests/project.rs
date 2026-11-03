//! `ibis project` checks exact keys against one read-only `.bib`, and
//! `ibis entry` gives a missing entry with its key (#1377).
use ibis_engine::Engine;
use serde_json::Value;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const PROJECT: &str = "@book{taylor2026,title={Synthetic signals},year={2026}}\n\
@book{twice2024,title={Repeated}}\n@book{twice2024,title={Repeated again}}\n\
@book{lee2020,title={Quite another work},year={2020}}\n";
const LIBRARY: &str = "@string{press = {Imaginary Press}}\n\
@book{taylor2026,title={Synthetic signals},year={2026}}\n\
@book{okafor2018,title={Invented seas},year={2018},publisher=press}\n\
@book{lee2020,title={Clockwork clouds},year={2020}}\n";

fn run(args: &[&str]) -> Result<(u8, String, String)> {
    let (mut output, mut errors) = (vec![], vec![]);
    let mut all = vec!["ibis"];
    all.extend_from_slice(args);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        all.into_iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok((code, String::from_utf8(output)?, String::from_utf8(errors)?))
}

struct Files {
    dir: PathBuf,
}
impl Files {
    fn new(test: &str) -> Result<Self> {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("project-{test}"));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("thesis.bib"), PROJECT)?;
        std::fs::write(dir.join("refs.bib"), LIBRARY)?;
        Ok(Self { dir })
    }
    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

#[test]
fn every_state_reads_as_the_app_words_it_and_names_only_the_file() -> Result {
    let files = Files::new("states")?;
    let thesis = files.path("thesis.bib");
    let (code, text, _) = run(&[
        "project",
        &thesis,
        "taylor2026",
        "okafor2018",
        "twice2024",
        "lee2020",
        "--file",
        &files.path("refs.bib"),
    ])?;
    assert_eq!(code, 1, "anything but In is a finding");
    assert_eq!(
        text,
        "taylor2026\tIn thesis.bib (line 1)\n\
         okafor2018\tMissing from thesis.bib\n\
         twice2024\tKey appears more than once (lines 2, 3)\n\
         lee2020\tKey may name another reference (line 4): this entry and the library differ\n"
    );
    assert!(!text.contains(files.dir.to_string_lossy().as_ref()));

    let (code, text, _) = run(&["project", &thesis, "taylor2026"])?;
    assert_eq!((code, text.as_str()), (0, "taylor2026\tIn thesis.bib (line 1)\n"));
    Ok(())
}

#[test]
fn the_project_never_becomes_a_search_library_or_changes() -> Result {
    let files = Files::new("separate")?;
    let thesis = files.path("thesis.bib");
    let refs = files.path("refs.bib");
    // lee2020 is in both files; a second owner would refuse the citation.
    let (code, text, _) = run(&["project", &thesis, "lee2020", "--file", &refs, "--json"])?;
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&text)?;
    assert_eq!(value["data"]["name"], "thesis.bib");
    assert_eq!(value["data"]["keys"][0]["check"]["status"], "may-name-another");
    assert_eq!(std::fs::read_to_string(&thesis)?, PROJECT);
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_is_never_missing() -> Result {
    let files = Files::new("unreadable")?;
    for (name, contents, why) in [
        ("damaged.bib", &b"@book{a,title={Never closed\n"[..], "check line 1"),
        ("binary.bib", &[0xff, 0xfe][..], "not UTF-8"),
    ] {
        std::fs::write(files.dir.join(name), contents)?;
        let (code, text, _) = run(&["project", &files.path(name), "okafor2018"])?;
        assert_eq!(code, 1, "{name}");
        assert!(text.contains(why), "{text}");
        assert!(text.contains(&format!("okafor2018\tCouldn't check {name}")), "{text}");
        assert!(!text.contains("Missing"), "{text}");
    }
    let (code, text, _) = run(&["project", &files.path("absent.bib"), "okafor2018"])?;
    assert_eq!(code, 1);
    assert!(
        text.contains("Couldn't check absent.bib") && !text.contains("Missing"),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_missing_entry_comes_as_written_with_its_key() -> Result {
    let files = Files::new("entry")?;
    let refs = files.path("refs.bib");
    let (code, text, _) = run(&["entry", "okafor2018", "--file", &refs])?;
    assert_eq!(code, 0);
    assert_eq!(
        text,
        "@string{press = {Imaginary Press}}\n\n@book{okafor2018,title={Invented seas},year={2018},publisher=press}\n"
    );
    let (code, text, _) = run(&["entry", "okafor2018", "--file", &refs, "--json"])?;
    assert_eq!(code, 0);
    let value: Value = serde_json::from_str(&text)?;
    assert_eq!(value["data"]["outcome"], "entry");
    assert_eq!(value["data"]["key"], "okafor2018");
    Ok(())
}

#[test]
fn an_entry_that_cannot_come_says_how_to_get_it() -> Result {
    let files = Files::new("refused")?;
    let json = files.dir.join("refs.json");
    std::fs::write(
        &json,
        r#"[{"id":"seas2018","type":"book","title":"Invented seas","citation-key":"seas2018"}]"#,
    )?;
    let (code, text, _) = run(&["entry", "seas2018", "--file", &json.to_string_lossy()])?;
    assert_eq!(code, 1);
    assert!(text.contains("Export it from your reference manager"), "{text}");
    let (code, _, errors) = run(&["entry", "nobody", "--file", &files.path("refs.bib")])?;
    assert_eq!(code, 4, "{errors}");
    Ok(())
}

#[test]
fn an_entry_is_never_given_from_a_file_that_changed_after_the_read() -> Result {
    let files = Files::new("changed")?;
    let path = files.path("refs.bib");
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    let cancel = ibis_engine::Cancellation::default();
    let dispatch = |method, params: Value| engine.dispatch(method, &params, &cancel);
    dispatch(
        ibis_engine::Method::Libraries,
        serde_json::json!({"action":"open-file","path":path}),
    )
    .map_err(|_| "open")?;
    std::fs::write(Path::new(&path), LIBRARY.replace("Invented seas", "A different work"))?;
    let result = dispatch(
        ibis_engine::Method::Entry,
        serde_json::json!({"item":{"by":"key","value":"okafor2018"}}),
    )
    .map_err(|_| "entry")?;
    assert_eq!(result.data["outcome"], "refused");
    assert_eq!(result.data["reason"], "changed");
    Ok(())
}
