//! `ibis check` through the real `ibis` executable: the manuscript is read by
//! its document child and the attachments by its PDF child (decision 0060).
//! Every manuscript, record and PDF here is invented.
#![cfg(test)]

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("check-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("papers")).unwrap();
        // Printed pages i, 1, 2: the lantern is on page 2.
        let pdf = ibis_sources::pdf::read::invented_pdf(
            &[
                "Invented preface.",
                "Invented chapter opening.",
                "Invented page that holds a brass lantern by the door.",
            ],
            Some(&[(0, Some("r"), "", 1), (1, Some("D"), "", 1)]),
        );
        std::fs::write(root.join("papers/atlas.pdf"), pdf).unwrap();
        let project = Self { root };
        project.library("Invented seas");
        project
    }

    fn library(&self, title: &str) {
        std::fs::write(
            self.root.join("refs.bib"),
            format!(
                "@book{{atlas2026, title = {{An Invented Atlas}}, author = {{Lee, Ada}}, year = {{2026}}, \
                 file = {{Invented atlas:papers/atlas.pdf:application/pdf}}}}\n\
                 @book{{okafor2018, title = {{{title}}}, author = {{Okafor, Ada}}, year = {{2018}}}}\n"
            ),
        )
        .unwrap();
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.root.join(name), text).unwrap();
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .current_dir(&self.root)
            .arg("--file")
            .arg(self.root.join("refs.bib"))
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env("APPDATA", self.root.join("AppData"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

const DRAFT: &str = "\
Lee finds \u{201c}a brass lantern\u{201d} [@atlas2026, p. 2].

Lee also finds \u{201c}a brass lantern by the door\u{201d} [@atlas2026, p. 1].

Okafor writes \u{201c}invented seas\u{201d} [@okafor2018], as [@ghost] does.
";

#[test]
fn a_manuscript_is_checked_line_by_line_and_fails_the_build_on_findings() {
    let project = Project::new("findings");
    project.write("draft.md", DRAFT);

    let (status, stdout, stderr) = project.run(&["check", "draft.md"]);
    assert_eq!(status, 1, "{stdout}{stderr}");
    assert_eq!(
        stdout,
        "5:49\tghost\tNo open library has this key\n\
         Citations read: 4. Keys: 3. Problems: 1.\n"
    );

    let (status, stdout, stderr) = project.run(&["check", "draft.md", "--quotes"]);
    assert_eq!(status, 1, "{stdout}{stderr}");
    assert_eq!(
        stdout,
        "3:46\tatlas2026\tThe quotation is not on the cited page; it is on 2\n\
         5:31\tokafor2018\tThe cited reference has no attachment whose text could be read\n\
         5:49\tghost\tNo open library has this key\n\
         Citations read: 4. Keys: 3. Problems: 3.\n"
    );
    // The manuscript's text and the attachment's never reach the report.
    assert!(!stdout.contains("door") && !stdout.contains("Invented page"));

    let (status, stdout, _) = project.run(&["check", "draft.md", "--quotes", "--json"]);
    assert_eq!(status, 1);
    let answer: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["data"]["outcome"], "checked");
    assert_eq!(answer["data"]["complete"], true);
    let found = &answer["data"]["findings"][0];
    assert_eq!(
        (&found["problem"], &found["pages"], &found["place"]["line"]),
        (
            &Value::from("quote-on-other-page"),
            &serde_json::json!(["2"]),
            &Value::from(3)
        )
    );

    project.write(
        "clean.md",
        "Lee finds \u{201c}a brass lantern\u{201d} [@atlas2026, p. 2] and [@okafor2018].\n",
    );
    let (status, stdout, stderr) = project.run(&["check", "clean.md", "--quotes"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "Citations read: 2. Keys: 2. Problems: 0.\n"),
        "{stderr}"
    );
}

#[test]
fn a_reference_set_locks_the_cited_records_for_co_authors() {
    let project = Project::new("set");
    project.write("draft.md", "As [@atlas2026] and [@okafor2018] show.\n");
    let (status, set, stderr) = project.run(&["cite", "atlas2026", "okafor2018", "--format", "csl-json"]);
    assert_eq!(status, 0, "{stderr}");
    project.write("references.json", &set);
    let (status, stdout, stderr) = project.run(&["check", "draft.md", "--set", "references.json"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "Citations read: 2. Keys: 2. Problems: 0.\n"),
        "{stderr}"
    );

    // A co-author's library has changed one record since the set was written.
    project.library("Invented seas, revised");
    let (status, stdout, _) = project.run(&["check", "draft.md", "--set", "references.json"]);
    assert_eq!(status, 1);
    assert_eq!(
        stdout,
        "1:21\tokafor2018\tThe record changed since the reference set was written\n\
         Citations read: 2. Keys: 2. Problems: 1.\n"
    );

    let (status, stdout, _) = project.run(&["check", "draft.md", "--set", "missing.json"]);
    assert_eq!(
        (status, stdout.as_str()),
        (1, "Couldn’t read the reference set: nothing is at that path.\n")
    );
    let (status, stdout, _) = project.run(&["check", "missing.md"]);
    assert_eq!(
        (status, stdout.as_str()),
        (1, "Couldn’t read the manuscript: nothing is at that path.\n")
    );
}

#[test]
fn a_following_page_or_typst_supplement_is_read_and_nocite_is_named() {
    let project = Project::new("pages");
    project.write(
        "draft.md",
        "Lee finds \u{201c}a brass lantern\u{201d} [@atlas2026, p. 1f].\n",
    );
    let (status, stdout, stderr) = project.run(&["check", "draft.md", "--quotes"]);
    assert_eq!(
        (status, stdout.as_str()),
        (0, "Citations read: 1. Keys: 1. Problems: 0.\n"),
        "{stderr}"
    );
    project.write("draft.typ", "Lee finds \"a brass lantern\" @atlas2026[p. 1].\n");
    let (status, stdout, _) = project.run(&["check", "draft.typ", "--quotes"]);
    assert_eq!(status, 1);
    assert!(stdout.contains("not on the cited page; it is on 2"), "{stdout}");
    project.write("draft.tex", "\\nocite{*} See \\cite{atlas2026}.\n");
    let (status, stdout, _) = project.run(&["check", "draft.tex"]);
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "Citations read: 1. Keys: 1. Problems: 0.\n\\nocite{*} cites the whole bibliography; only the keys cited by name were checked.\n"
        )
    );
}
