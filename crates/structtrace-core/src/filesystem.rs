//! Cross-platform private-storage policy for local evidence.

use std::path::Path;

/// Restrict a directory to the current account.
pub fn make_private_directory(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    restrict_windows_acl(path, true)?;
    Ok(())
}

/// Restrict a file to the current account.
pub fn make_private_file(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    restrict_windows_acl(path, false)?;
    Ok(())
}

/// Recursively reject symlinks and enforce the private-storage policy.
pub fn make_private_tree(root: &Path) -> anyhow::Result<()> {
    fn visit(path: &Path) -> anyhow::Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "private artifact must not be a symlink: {}",
            path.display()
        );
        if metadata.is_dir() {
            make_private_directory(path)?;
            for entry in std::fs::read_dir(path)? {
                visit(&entry?.path())?;
            }
        } else {
            make_private_file(path)?;
        }
        Ok(())
    }
    visit(root)
}

#[cfg(windows)]
fn restrict_windows_acl(path: &Path, directory: bool) -> anyhow::Result<()> {
    let identity = std::process::Command::new("whoami").output()?;
    anyhow::ensure!(
        identity.status.success(),
        "whoami failed while securing local evidence"
    );
    let identity = String::from_utf8(identity.stdout)?.trim().to_owned();
    anyhow::ensure!(
        !identity.is_empty(),
        "Windows account identity is unavailable"
    );
    let grant = if directory {
        format!("{identity}:(OI)(CI)F")
    } else {
        format!("{identity}:F")
    };
    let result = std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &grant])
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "icacls could not restrict local evidence: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(())
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn private_files_and_directories_disable_acl_inheritance() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("private");
        std::fs::create_dir(&directory).unwrap();
        let file = directory.join("evidence.json");
        std::fs::write(&file, b"{}\n").unwrap();
        make_private_directory(&directory).unwrap();
        make_private_file(&file).unwrap();
        for path in [&directory, &file] {
            let output = std::process::Command::new("icacls")
                .arg(path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "icacls inspection failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let acl = String::from_utf8(output.stdout).unwrap();
            assert!(
                !acl.contains("(I)"),
                "private path retained an inherited access-control entry: {acl}"
            );
        }
    }
}
