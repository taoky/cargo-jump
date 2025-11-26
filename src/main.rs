use std::path::{Path, PathBuf};

use anyhow::Result;
use cargo_metadata::MetadataCommand;
use clap::Parser;
use toml_edit::DocumentMut;
use tracing::{debug, info, warn};

#[derive(Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
enum CargoCli {
    Jump(JumpArgs),
}

#[derive(clap::Args)]
#[command(version, about, long_about = None)]
struct JumpArgs {
    /// New version to set
    new_version: String,

    /// Old git tag for comparison
    #[arg(long)]
    old_tag: Option<String>,

    /// Don't modify anything
    #[arg(long)]
    dry_run: bool,
}

fn git_toplevel() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get git toplevel directory");
    }

    let path = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(path))
}

fn git_changed_files(toplevel: &Path, old_tag: &str) -> Result<Vec<PathBuf>> {
    let output = std::process::Command::new("git")
        .args([
            "-C",
            toplevel
                .as_os_str()
                .to_str()
                .expect("shall be a valid UTF-8 path"),
        ])
        .args(["diff", "--name-only", old_tag, "HEAD"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get changed files from git");
    }

    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(|line| toplevel.join(line))
        .collect())
}

fn git_all_files(toplevel: &Path) -> Result<Vec<PathBuf>> {
    let output = std::process::Command::new("git")
        .args([
            "-C",
            toplevel
                .as_os_str()
                .to_str()
                .expect("shall be a valid UTF-8 path"),
        ])
        .args(["ls-files"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get all files from git");
    }

    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(|line| toplevel.join(line))
        .collect())
}

fn main() {
    tracing_subscriber::fmt::init();
    let CargoCli::Jump(args) = CargoCli::parse();
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .expect("cannot get cargo metadata");
    let toplevel = git_toplevel().expect("cannot get git toplevel directory");

    if !metadata.workspace_root.starts_with(&toplevel) {
        panic!("workspace root is not inside git toplevel");
    }

    let changed_files = if let Some(old_tag) = &args.old_tag {
        git_changed_files(&toplevel, old_tag).expect("cannot get changed files from git")
    } else {
        warn!("old_tag not provided, considering all files as changed");
        git_all_files(&toplevel).expect("cannot get all files from git")
    };

    let workspace_member_ids = metadata.workspace_members;
    let members: Vec<_> = metadata
        .packages
        .iter()
        .filter(|p| workspace_member_ids.contains(&p.id))
        .collect();

    let mut all_affected_packages = Vec::new();
    for package in members {
        let manifest_path = package.manifest_path.as_std_path();
        let manifest_dir = manifest_path
            .parent()
            .expect("manifest path shall have a parent directory");
        let is_affected = changed_files
            .iter()
            .any(|changed_file| changed_file.starts_with(manifest_dir));
        if is_affected {
            debug!("Package '{}' is affected", package.name);
            all_affected_packages.push(package);
        } else {
            debug!("Package '{}' is not affected", package.name);
        }
    }

    if all_affected_packages.is_empty() {
        info!("No affected packages found.");
        return;
    }

    let mut has_change = false;

    for package in all_affected_packages {
        info!(
            "Setting version of package '{}' to '{}'",
            package.name, args.new_version
        );
        let manifest_path = package.manifest_path.as_std_path();
        let mut manifest_content: DocumentMut = std::fs::read_to_string(manifest_path)
            .expect("cannot read manifest file")
            .parse()
            .expect("cannot parse manifest file as TOML document");
        let package_table = manifest_content
            .get_mut("package")
            .and_then(|it| it.as_table_mut())
            .expect("missing [package]");
        let version_item = package_table
            .get_mut("version")
            .expect("missing package.version");
        *version_item = toml_edit::value(args.new_version.clone());
        if args.dry_run {
            info!("Dry run: not updating {}", manifest_path.display());
        } else {
            std::fs::write(manifest_path, manifest_content.to_string())
                .expect("cannot write updated manifest file");
            has_change = true;
        }
    }

    if has_change {
        info!("Updating Cargo.lock...");
        let output = std::process::Command::new("cargo")
            .args(["fetch"])
            .output()
            .expect("failed to execute cargo fetch");
        if !output.status.success() {
            panic!("cargo fetch failed");
        }
    }
}
