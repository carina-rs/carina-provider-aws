use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{Builder, Header};

const VALID_PAGE: &str = concat!(
    "# aws.fixture.Widget\n\n",
    "## Argument Reference\n\n",
    "### `name`\n\n",
    "See [details](#details).\n\n",
    "## Details\n",
);

fn run_checker(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_carina-docgen-check"))
        .arg(path)
        .output()
        .expect("run carina-docgen-check")
}

fn write_tarball(path: &Path, page: &str) {
    let encoder = GzEncoder::new(
        File::create(path).expect("create tarball"),
        Compression::default(),
    );
    let mut archive = Builder::new(encoder);
    let mut header = Header::new_gnu();
    header.set_size(page.len().try_into().expect("page length fits in u64"));
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "resources/widget.md", page.as_bytes())
        .expect("append Markdown page");
    let encoder = archive.into_inner().expect("finish tar archive");
    encoder.finish().expect("finish gzip stream");
}

#[test]
fn checker_reads_markdown_recursively_from_a_directory() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let page = temp.path().join("resources/widget.md");
    fs::create_dir_all(page.parent().expect("page parent")).expect("create page directory");
    File::create(&page)
        .expect("create page")
        .write_all(VALID_PAGE.as_bytes())
        .expect("write page");

    let output = run_checker(temp.path());

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 Markdown page"), "{stdout}");
    assert!(stdout.contains("0 unresolved fragments"), "{stdout}");
    assert!(stdout.contains("0 links inside code spans"), "{stdout}");
    assert!(stdout.contains("0 duplicated section headings"), "{stdout}");
}

#[test]
fn checker_reads_the_shipped_tarball_and_reports_violations() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let valid_bundle = temp.path().join("valid.tar.gz");
    write_tarball(&valid_bundle, VALID_PAGE);

    let output = run_checker(&valid_bundle);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let invalid_bundle = temp.path().join("invalid.tar.gz");
    write_tarball(
        &invalid_bundle,
        "# aws.fixture.Widget\n\n[missing](#not-a-heading)\n",
    );

    let output = run_checker(&invalid_bundle);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("resources/widget.md"), "{stderr}");
    assert!(stderr.contains("#not-a-heading"), "{stderr}");
}
