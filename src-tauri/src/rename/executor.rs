use super::{display, Row, Stamp};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

pub(super) fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.is_symlink()
    }
}
pub(super) fn is_hidden(m: &fs::Metadata, name: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        name.starts_with('.') || m.file_attributes() & 2 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = m;
        name.starts_with('.')
    }
}
pub(super) fn identity(path: &Path) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            Storage::FileSystem::{
                CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            },
        };
        let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error().to_string());
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        let queried = unsafe { GetFileInformationByHandle(handle, &mut info) } != 0;
        let query_error = (!queried).then(io::Error::last_os_error);
        let closed = unsafe { CloseHandle(handle) } != 0;
        if let Some(error) = query_error {
            return Err(error.to_string());
        }
        if !closed {
            return Err(io::Error::last_os_error().to_string());
        }
        Ok(format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        Ok(format!("{}:{}", m.dev(), m.ino()))
    }
}
pub(super) fn stamp(path: &Path) -> Result<Stamp, String> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || is_link(&m) {
        return Err("文件不是普通文件或已变为链接".into());
    }
    Ok(Stamp {
        identity: identity(path)?,
        len: m.len(),
        modified: m.modified().map_err(|e| e.to_string())?,
    })
}
// Windows 明确不传 REPLACE_EXISTING；Unix 用不覆盖的 link/unlink，不能用会覆盖目标的 rename。
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(unix)]
    {
        fs::hard_link(from, to)?;
        if let Err(error) = fs::remove_file(from) {
            fs::remove_file(to).map_err(|cleanup| {
                io::Error::other(format!("{error}；清理目标失败 {}：{cleanup}", to.display()))
            })?;
            return Err(error);
        }
        Ok(())
    }
}
pub(super) fn execute(rows: &[Row], stamps: &HashMap<String, Stamp>) -> Result<(), String> {
    execute_with(rows, stamps, move_no_replace)
}
fn execute_with(
    rows: &[Row],
    stamps: &HashMap<String, Stamp>,
    mut mover: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), String> {
    let temporary: Vec<PathBuf> = rows
        .iter()
        .map(|r| {
            Path::new(&r.from)
                .parent()
                .unwrap()
                .join(format!(".toolbox-rename-{}.tmp", uuid::Uuid::new_v4()))
        })
        .collect();
    let (mut staged, mut published) = (0, 0);
    let result = (|| {
        for (i, row) in rows.iter().enumerate() {
            if stamp(Path::new(&row.from))? != stamps[&row.from] {
                return Err(format!("文件已改变：{}", row.from));
            }
            mover(Path::new(&row.from), &temporary[i])
                .map_err(|e| format!("{} → {}：{e}", row.from, temporary[i].display()))?;
            staged += 1;
        }
        for (i, row) in rows.iter().enumerate() {
            if stamp(&temporary[i])? != stamps[&row.from] {
                return Err(format!("执行期间文件已改变：{}", temporary[i].display()));
            }
            mover(&temporary[i], Path::new(&row.to))
                .map_err(|e| format!("{} → {}：{e}", temporary[i].display(), row.to))?;
            published += 1;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = vec![];
        for i in (0..published).rev() {
            let source = Path::new(&rows[i].to);
            match identity(source) {
                Ok(id) if id == stamps[&rows[i].from].identity => {
                    if let Err(e) = mover(source, &temporary[i]) {
                        failures.push(format!(
                            "{} → {}：{e}",
                            source.display(),
                            temporary[i].display()
                        ));
                    }
                }
                _ => failures.push(format!("无法确认目标身份，保留 {}", source.display())),
            }
        }
        for i in (0..staged).rev() {
            match identity(&temporary[i]) {
                Ok(id) if id == stamps[&rows[i].from].identity => {
                    if let Err(e) = mover(&temporary[i], Path::new(&rows[i].from)) {
                        failures.push(format!(
                            "{} → {}：{e}",
                            temporary[i].display(),
                            rows[i].from
                        ));
                    }
                }
                _ => failures.push(format!(
                    "临时文件丢失或身份无法确认，请检查 {}",
                    display(&temporary[i])
                )),
            }
        }
        return Err(format!(
            "改名失败：{error}；{}",
            if failures.is_empty() {
                "回滚完成".into()
            } else {
                format!("回滚未完成，请保留并检查这些路径：{}", failures.join("；"))
            }
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_temporary_file_is_reported_as_incomplete_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a");
        fs::write(&from, "a").unwrap();
        let row = Row {
            from: display(&from),
            to: display(&dir.path().join("b")),
            from_name: "a".into(),
            to_name: "b".into(),
            error: None,
        };
        let stamps = HashMap::from([(row.from.clone(), stamp(&from).unwrap())]);
        let error = execute_with(&[row], &stamps, |from, to| {
            move_no_replace(from, to)?;
            // 模拟外部进程在暂存完成后移除临时文件。
            fs::remove_file(to)
        })
        .unwrap_err();
        assert!(error.contains("回滚未完成"), "{error}");
        assert!(error.contains("临时文件丢失"), "{error}");
        assert!(error.contains(".toolbox-rename-"), "{error}");
    }
    #[test]
    fn publication_failure_restores_all_sources_without_overwriting_external_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut rows = vec![];
        let mut stamps = HashMap::new();
        for n in ["a", "b"] {
            let from = dir.path().join(n);
            fs::write(&from, n).unwrap();
            let row = Row {
                from: display(&from),
                to: display(&dir.path().join(format!("new_{n}"))),
                from_name: n.into(),
                to_name: format!("new_{n}"),
                error: None,
            };
            stamps.insert(row.from.clone(), stamp(&from).unwrap());
            rows.push(row);
        }
        let mut calls = 0;
        let error = execute_with(&rows, &stamps, |from, to| {
            calls += 1;
            if calls == 4 {
                fs::write(to, "external").unwrap();
            }
            move_no_replace(from, to)
        })
        .unwrap_err();
        assert!(error.contains("回滚完成"), "{error}");
        assert_eq!(fs::read_to_string(dir.path().join("a")).unwrap(), "a");
        assert_eq!(fs::read_to_string(dir.path().join("b")).unwrap(), "b");
        assert_eq!(
            fs::read_to_string(dir.path().join("new_b")).unwrap(),
            "external"
        );
        assert!(!dir.path().join("new_a").exists());
    }
    #[test]
    fn staging_failure_restores_already_staged_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut rows = vec![];
        let mut stamps = HashMap::new();
        for n in ["a", "b"] {
            let from = dir.path().join(n);
            fs::write(&from, n).unwrap();
            let row = Row {
                from: display(&from),
                to: display(&dir.path().join(format!("new_{n}"))),
                from_name: n.into(),
                to_name: format!("new_{n}"),
                error: None,
            };
            stamps.insert(row.from.clone(), stamp(&from).unwrap());
            rows.push(row);
        }
        let mut calls = 0;
        let error = execute_with(&rows, &stamps, |a, b| {
            calls += 1;
            if calls == 2 {
                return Err(io::Error::other("injected"));
            }
            move_no_replace(a, b)
        })
        .unwrap_err();
        assert!(error.contains("回滚完成"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
