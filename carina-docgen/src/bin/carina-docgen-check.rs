use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use carina_docgen::{PageViolationKind, check_page};
use flate2::read::GzDecoder;
use tar::Archive;

#[derive(Default)]
struct CheckSummary {
    pages: usize,
    unresolved_fragments: usize,
    links_inside_code_spans: usize,
    duplicated_section_headings: usize,
    reports: Vec<String>,
}

impl CheckSummary {
    fn check(&mut self, name: &str, markdown: &str) {
        self.pages += 1;
        if let Err(error) = check_page(markdown) {
            for violation in error.violations() {
                match violation.kind {
                    PageViolationKind::UnresolvedFragment => self.unresolved_fragments += 1,
                    PageViolationKind::LinkInsideCodeSpan => self.links_inside_code_spans += 1,
                    PageViolationKind::DuplicateSectionHeading => {
                        self.duplicated_section_headings += 1;
                    }
                }
            }
            self.reports.push(format!("{name}:\n{error}"));
        }
    }

    fn finish(self) -> Result<(), String> {
        if self.pages == 0 {
            return Err("input contains no Markdown pages".to_string());
        }
        let violations = self.unresolved_fragments
            + self.links_inside_code_spans
            + self.duplicated_section_headings;
        if violations != 0 {
            return Err(format!(
                "{}\nchecked {} Markdown page(s): {} unresolved fragments, {} links inside code spans, {} duplicated section headings",
                self.reports.join("\n"),
                self.pages,
                self.unresolved_fragments,
                self.links_inside_code_spans,
                self.duplicated_section_headings
            ));
        }
        println!(
            "checked {} Markdown page(s): 0 unresolved fragments, 0 links inside code spans, 0 duplicated section headings",
            self.pages
        );
        Ok(())
    }
}

fn main() -> ExitCode {
    let mut args = env::args_os();
    let program = args
        .next()
        .and_then(|value| PathBuf::from(value).file_name().map(OsStr::to_owned))
        .unwrap_or_else(|| OsStr::new("carina-docgen-check").to_owned());
    let Some(input) = args.next() else {
        eprintln!("usage: {} <dir-or-tar.gz>", program.to_string_lossy());
        return ExitCode::FAILURE;
    };
    if args.next().is_some() {
        eprintln!("usage: {} <dir-or-tar.gz>", program.to_string_lossy());
        return ExitCode::FAILURE;
    }

    match check_input(Path::new(&input)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn check_input(input: &Path) -> Result<(), String> {
    let mut summary = CheckSummary::default();
    if input.is_dir() {
        check_directory(input, input, &mut summary)?;
    } else if input.is_file()
        && input
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".tar.gz"))
    {
        check_tarball(input, &mut summary)?;
    } else {
        return Err(format!(
            "input must be a directory or .tar.gz file: {}",
            input.display()
        ));
    }
    summary.finish()
}

fn check_directory(
    root: &Path,
    directory: &Path,
    summary: &mut CheckSummary,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    let mut paths = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("cannot read {}: {error}", directory.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();

    for path in paths {
        let file_type = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
            .file_type();
        if file_type.is_dir() {
            check_directory(root, &path, summary)?;
        } else if file_type.is_file() && path.extension() == Some(OsStr::new("md")) {
            let markdown = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {} as UTF-8: {error}", path.display()))?;
            let name = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
            summary.check(&name, &markdown);
        }
    }
    Ok(())
}

fn check_tarball(path: &Path, summary: &mut CheckSummary) -> Result<(), String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open bundle {}: {error}", path.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let entries = archive
        .entries()
        .map_err(|error| format!("cannot read bundle {}: {error}", path.display()))?;

    for entry in entries {
        let mut entry = entry
            .map_err(|error| format!("cannot read an entry in {}: {error}", path.display()))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let entry_path = entry
            .path()
            .map_err(|error| format!("bundle entry has an invalid path: {error}"))?
            .into_owned();
        if entry_path.extension() != Some(OsStr::new("md")) {
            continue;
        }
        let mut markdown = String::new();
        entry
            .read_to_string(&mut markdown)
            .map_err(|error| format!("cannot read {} as UTF-8: {error}", entry_path.display()))?;
        summary.check(&entry_path.to_string_lossy(), &markdown);
    }
    Ok(())
}
