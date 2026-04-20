#[cfg(not(unix))]
use super::resolve_workspace_write_path;
use super::{
    Tool, ToolResult, ToolSpec, canonical_workspace_root, validate_workspace_relative_path,
};
use serde_json::{Value, json};
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)]
const DIRECTORY_OPEN_FLAGS: i32 =
    nix::libc::O_RDONLY | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC | nix::libc::O_NOFOLLOW;
#[cfg(unix)]
const TEMP_FILE_OPEN_FLAGS: i32 = nix::libc::O_WRONLY
    | nix::libc::O_CREAT
    | nix::libc::O_EXCL
    | nix::libc::O_CLOEXEC
    | nix::libc::O_NOFOLLOW;
#[cfg(unix)]
const CREATED_DIRECTORY_MODE: nix::libc::mode_t = 0o755;
#[cfg(unix)]
const CREATED_FILE_MODE: nix::libc::mode_t = 0o644;

pub struct WriteFileTool {
    workspace_root: PathBuf,
}

impl WriteFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

impl Tool for WriteFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".into(),
            description: "Write a UTF-8 file inside the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(|value| value.as_str()) else {
            return ToolResult::failure("write_file requires a string path");
        };
        let Some(content) = args.get("content").and_then(|value| value.as_str()) else {
            return ToolResult::failure("write_file requires string content");
        };

        let full_path = self.workspace_root.join(path);

        #[cfg(unix)]
        let write_result = write_utf8_within_workspace(&self.workspace_root, path, content);
        #[cfg(not(unix))]
        let write_result = write_utf8_with_fallback(&self.workspace_root, path, content);

        match write_result {
            Ok(()) => ToolResult::success(format!("wrote {} bytes to {}", content.len(), path)),
            Err(error) => {
                ToolResult::failure(format!("cannot write {}: {error}", full_path.display()))
            }
        }
    }
}

#[cfg(unix)]
fn write_utf8_within_workspace(
    workspace_root: &Path,
    requested: &str,
    content: &str,
) -> std::io::Result<()> {
    validate_workspace_relative_path(requested)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    let canonical_root = canonical_workspace_root(workspace_root).map_err(std::io::Error::other)?;

    let components = Path::new(requested)
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some((file_name, parent_components)) = components.split_last() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "write_file path must include a file name",
        ));
    };

    let mut current_dir = open_root_directory(&canonical_root)?;
    for component in parent_components {
        current_dir = open_or_create_directory(&current_dir, component)?;
    }

    reject_symlink_target(&current_dir, file_name)?;
    let (temp_name, mut file) = create_temp_file_in_directory(&current_dir, file_name)?;
    let write_result = (|| -> std::io::Result<()> {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        rename_in_directory(&current_dir, &temp_name, file_name)
    })();
    if let Err(error) = write_result {
        let _ = unlink_in_directory(&current_dir, &temp_name);
        return Err(error);
    }

    Ok(())
}

#[cfg(not(unix))]
fn write_utf8_with_fallback(
    workspace_root: &Path,
    requested: &str,
    content: &str,
) -> std::io::Result<()> {
    let (full_path, canonical_root) =
        resolve_workspace_write_path(workspace_root, requested).map_err(std::io::Error::other)?;
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
        let canonical_parent = std::fs::canonicalize(parent)?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "path must stay inside workspace root",
            ));
        }
    }

    write_utf8_without_following_symlink(&full_path, content)
}

#[cfg(not(unix))]
fn write_utf8_without_following_symlink(
    path: &std::path::Path,
    content: &str,
) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(unix)]
fn open_root_directory(path: &Path) -> std::io::Result<OwnedFd> {
    let path = c_string(path.as_os_str())?;
    open_at(nix::libc::AT_FDCWD, &path, DIRECTORY_OPEN_FLAGS, 0)
}

#[cfg(unix)]
fn open_or_create_directory(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<OwnedFd> {
    match open_directory_in(parent, name) {
        Ok(directory) => Ok(directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            mkdir_in(parent, name)?;
            open_directory_in(parent, name)
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn open_directory_in(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<OwnedFd> {
    let name = c_string(name)?;
    open_at(parent.as_raw_fd(), &name, DIRECTORY_OPEN_FLAGS, 0)
}

#[cfg(unix)]
fn open_file_in_directory(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<File> {
    let name = c_string(name)?;
    let fd = open_at(
        parent.as_raw_fd(),
        &name,
        TEMP_FILE_OPEN_FLAGS,
        CREATED_FILE_MODE,
    )?;
    Ok(File::from(fd))
}

#[cfg(unix)]
fn reject_symlink_target(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<()> {
    let name = c_string(name)?;
    let mut stat = std::mem::MaybeUninit::<nix::libc::stat>::uninit();
    let result = unsafe {
        nix::libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            nix::libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        let stat = unsafe { stat.assume_init() };
        if (stat.st_mode & nix::libc::S_IFMT) == nix::libc::S_IFLNK {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "refusing to replace a symlink target",
            ));
        }
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(unix)]
fn create_temp_file_in_directory(
    parent: &OwnedFd,
    target_name: &std::ffi::OsStr,
) -> std::io::Result<(std::ffi::OsString, File)> {
    let pid = std::process::id();
    let target_name = target_name.to_string_lossy();
    for attempt in 0..128u32 {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        let temp_name = std::ffi::OsString::from(format!(
            ".{target_name}.blazar-tmp-{pid}-{timestamp}-{attempt}"
        ));
        match open_file_in_directory(parent, &temp_name) {
            Ok(file) => return Ok((temp_name, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to allocate a unique temporary file name",
    ))
}

#[cfg(unix)]
fn rename_in_directory(
    parent: &OwnedFd,
    source: &std::ffi::OsStr,
    destination: &std::ffi::OsStr,
) -> std::io::Result<()> {
    let source = c_string(source)?;
    let destination = c_string(destination)?;
    let result = unsafe {
        nix::libc::renameat(
            parent.as_raw_fd(),
            source.as_ptr(),
            parent.as_raw_fd(),
            destination.as_ptr(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn unlink_in_directory(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<()> {
    let name = c_string(name)?;
    let result = unsafe { nix::libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn mkdir_in(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<()> {
    let name = c_string(name)?;
    let result =
        unsafe { nix::libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), CREATED_DIRECTORY_MODE) };
    if result == 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(unix)]
fn open_at(
    parent_fd: std::os::fd::RawFd,
    name: &CString,
    flags: i32,
    mode: nix::libc::mode_t,
) -> std::io::Result<OwnedFd> {
    let fd = unsafe { nix::libc::openat(parent_fd, name.as_ptr(), flags, mode) };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

#[cfg(unix)]
fn c_string(name: &std::ffi::OsStr) -> std::io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains an interior NUL byte",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{open_file_in_directory, open_root_directory, write_utf8_within_workspace};
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::fs as unix_fs};

    fn fresh_workspace(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(format!("blazar-write-file-{label}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn write_utf8_within_workspace_rejects_final_symlink_target() {
        let workspace = fresh_workspace("nofollow");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(OsString::from(format!("write-file-outside-{suffix}")));
        fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        let symlink_path = workspace.join("target.txt");
        unix_fs::symlink(&outside_file, &symlink_path).unwrap();

        let error = write_utf8_within_workspace(&workspace, "target.txt", "updated")
            .expect_err("final symlink target should be rejected");

        assert_eq!(fs::read_to_string(outside_file).unwrap(), "secret");
        assert!(error.kind() != std::io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn write_utf8_within_workspace_replaces_hard_link_without_touching_outside_inode() {
        use std::os::unix::fs::MetadataExt;

        let workspace = fresh_workspace("hard-link-replace");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(OsString::from(format!(
                "write-file-hard-link-outside-{suffix}"
            )));
        fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("shared.txt");
        fs::write(&outside_file, "secret").unwrap();

        let workspace_file = workspace.join("shared.txt");
        fs::hard_link(&outside_file, &workspace_file).unwrap();
        let outside_inode_before = fs::metadata(&outside_file).unwrap().ino();

        write_utf8_within_workspace(&workspace, "shared.txt", "updated").unwrap();

        let outside_metadata = fs::metadata(&outside_file).unwrap();
        let workspace_metadata = fs::metadata(&workspace_file).unwrap();
        assert_eq!(fs::read_to_string(&outside_file).unwrap(), "secret");
        assert_eq!(fs::read_to_string(&workspace_file).unwrap(), "updated");
        assert_eq!(outside_metadata.ino(), outside_inode_before);
        assert_ne!(workspace_metadata.ino(), outside_metadata.ino());
    }

    #[cfg(unix)]
    #[test]
    fn write_utf8_within_workspace_rejects_symlinked_ancestor_directory() {
        let workspace = fresh_workspace("ancestor-nofollow");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(OsString::from(format!(
                "write-file-ancestor-outside-{suffix}"
            )));
        fs::create_dir_all(&outside).unwrap();
        unix_fs::symlink(&outside, workspace.join("redirect")).unwrap();

        let error = write_utf8_within_workspace(&workspace, "redirect/escape.txt", "escape")
            .expect_err("symlinked ancestor should be rejected");

        assert!(!outside.join("escape.txt").exists());
        assert!(error.kind() != std::io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn open_root_directory_rejects_symlink_replacement_of_canonical_root() {
        let workspace = fresh_workspace("root-open-race");
        let canonical_root = fs::canonicalize(&workspace).unwrap();
        let renamed_workspace = workspace.with_extension("renamed");
        fs::rename(&workspace, &renamed_workspace).unwrap();

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-workspaces")
            .join(OsString::from(format!("write-file-root-outside-{suffix}")));
        fs::create_dir_all(&outside).unwrap();
        unix_fs::symlink(&outside, &workspace).unwrap();

        let error = open_root_directory(&canonical_root)
            .and_then(|root| open_file_in_directory(&root, OsStr::new("escape.txt")).map(|_| ()))
            .expect_err("symlink replacement of canonical root should be rejected");

        assert!(!outside.join("escape.txt").exists());
        assert!(error.kind() != std::io::ErrorKind::NotFound);
    }
}
