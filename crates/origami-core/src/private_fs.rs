use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(crate) fn secure_existing_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if path.exists() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Tighten an existing directory tree: dirs `0700`, files `0600`.
pub(crate) fn secure_existing_tree(root: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        if !root.exists() {
            return Ok(());
        }
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            // Symlink targets may live outside the tree; never chmod through them.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                secure_existing_tree(&entry.path())?;
            } else {
                fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o600))?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = root;
    Ok(())
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)
}

pub(crate) fn write_new_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn private_paths_use_private_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("mail");
        let file = directory.join("message");

        create_private_dir(&directory).unwrap();
        write_private(&file, b"private mail").unwrap();

        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        create_private_dir(&directory).unwrap();
        secure_existing_file(&file).unwrap();

        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn secure_existing_tree_skips_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside_file = temp.path().join("outside-file");
        let outside_dir = temp.path().join("outside-dir");
        fs::write(&outside_file, b"keep my mode").unwrap();
        fs::create_dir(&outside_dir).unwrap();
        fs::set_permissions(&outside_file, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&outside_dir, fs::Permissions::from_mode(0o755)).unwrap();

        let tree = temp.path().join("tree");
        fs::create_dir(&tree).unwrap();
        symlink(&outside_file, tree.join("link-to-file")).unwrap();
        symlink(&outside_dir, tree.join("link-to-dir")).unwrap();

        secure_existing_tree(&tree).unwrap();

        assert_eq!(
            fs::metadata(&outside_file).unwrap().permissions().mode() & 0o777,
            0o755,
            "symlink target file mode must be untouched"
        );
        assert_eq!(
            fs::metadata(&outside_dir).unwrap().permissions().mode() & 0o777,
            0o755,
            "symlink target dir mode must be untouched"
        );
        assert_eq!(
            fs::metadata(&tree).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
