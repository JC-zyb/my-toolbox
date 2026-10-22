use super::*;
use sha2::{Digest, Sha256};
use std::{
    ptr::{null, null_mut},
    sync::Mutex,
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS,
        ERROR_SUCCESS,
    },
    System::Registry::*,
    UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    },
};

static WRITE_LOCK: Mutex<()> = Mutex::new(());
const USER_KEY: &str = "Environment";
const SYSTEM_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";
const MAX_VALUE_BYTES: usize = 1_048_576;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn error(operation: &str, status: u32) -> String {
    if status == ERROR_ACCESS_DENIED {
        format!("{operation}失败：没有写入或读取权限。系统变量需以管理员身份启动工具箱")
    } else {
        format!(
            "{operation}失败：{}（{status}）",
            std::io::Error::from_raw_os_error(status as i32)
        )
    }
}
fn location(scope: Scope) -> (HKEY, &'static str) {
    match scope {
        Scope::User => (HKEY_CURRENT_USER, USER_KEY),
        Scope::System => (HKEY_LOCAL_MACHINE, SYSTEM_KEY),
    }
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
impl Key {
    fn open(root: HKEY, path: &str, access: u32) -> Result<Option<Self>, String> {
        let mut handle = null_mut();
        let status = unsafe { RegOpenKeyExW(root, wide(path).as_ptr(), 0, access, &mut handle) };
        match status {
            ERROR_SUCCESS => Ok(Some(Self(handle))),
            ERROR_FILE_NOT_FOUND => Ok(None),
            _ => Err(error("打开环境变量注册表键", status)),
        }
    }
    fn create(root: HKEY, path: &str) -> Result<Self, String> {
        let mut handle = null_mut();
        let status = unsafe {
            RegCreateKeyExW(
                root,
                wide(path).as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                null(),
                &mut handle,
                null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(error("创建环境变量注册表键", status));
        }
        Ok(Self(handle))
    }
    fn read(&self, name: &str) -> Result<Option<RawValue>, String> {
        let name = wide(name);
        // 注册表大小探测与读取之间可能发生外部修改；限次重读，不能静默截断。
        for _ in 0..3 {
            let (mut kind, mut size) = (0, 0);
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    null(),
                    &mut kind,
                    null_mut(),
                    &mut size,
                )
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                return Err(error("读取变量大小", status));
            }
            if size as usize > MAX_VALUE_BYTES {
                return Err("注册表值超过 1 MiB，无法安全展示".into());
            }
            let mut bytes = vec![0; size as usize];
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    null(),
                    &mut kind,
                    bytes.as_mut_ptr(),
                    &mut size,
                )
            };
            if status == ERROR_MORE_DATA {
                continue;
            }
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                return Err(error("读取环境变量", status));
            }
            bytes.truncate(size as usize);
            return Ok(Some(RawValue { kind, bytes }));
        }
        Err("变量正在被外部程序修改，请稍后刷新".into())
    }
    fn variables(&self) -> Result<Vec<Variable>, String> {
        let mut variables = vec![];
        let mut index = 0;
        loop {
            let mut name = vec![0u16; 32_768];
            let mut len = name.len() as u32;
            let status = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    name.as_mut_ptr(),
                    &mut len,
                    null(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS {
                return Err(error("枚举环境变量", status));
            }
            let name = String::from_utf16(&name[..len as usize])
                .map_err(|_| "变量名包含无效 UTF-16，无法安全编辑")?;
            if let Some(raw) = self.read(&name)? {
                variables.push(raw.variable(name));
            }
            index += 1;
        }
        variables.sort_by_key(|v| v.name.to_lowercase());
        Ok(variables)
    }
    fn apply(&self, change: &Change) -> Result<Option<Variable>, String> {
        let old = self.read(&change.name)?;
        if old.as_ref().map(RawValue::revision) != change.expected_revision {
            return Err("变量已被外部程序修改或同名变量已存在，请关闭编辑窗口并刷新后重试".into());
        }
        if old
            .as_ref()
            .is_some_and(|v| v.variable(change.name.clone()).kind == ValueKind::Unsupported)
        {
            return Err("原注册表值不是可编辑字符串，操作已取消".into());
        }
        let name = wide(&change.name);
        if let Some(value) = &change.value {
            let raw = RawValue::from_text(value, change.kind);
            let status = unsafe {
                RegSetValueExW(
                    self.0,
                    name.as_ptr(),
                    0,
                    raw.kind,
                    raw.bytes.as_ptr(),
                    raw.bytes.len() as u32,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(error("保存环境变量", status));
            }
            Ok(Some(raw.variable(change.name.clone())))
        } else {
            let status = unsafe { RegDeleteValueW(self.0, name.as_ptr()) };
            if status != ERROR_SUCCESS {
                return Err(error("删除环境变量", status));
            }
            Ok(None)
        }
    }
}

struct RawValue {
    kind: u32,
    bytes: Vec<u8>,
}
impl RawValue {
    fn revision(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.kind.to_le_bytes());
        hash.update(&self.bytes);
        format!("{:x}", hash.finalize())
    }
    fn from_text(value: &str, kind: ValueKind) -> Self {
        Self {
            kind: if kind == ValueKind::ExpandString {
                REG_EXPAND_SZ
            } else {
                REG_SZ
            },
            bytes: wide(value).iter().flat_map(|c| c.to_le_bytes()).collect(),
        }
    }
    fn variable(&self, name: String) -> Variable {
        let mut kind = match self.kind {
            REG_SZ => ValueKind::String,
            REG_EXPAND_SZ => ValueKind::ExpandString,
            _ => ValueKind::Unsupported,
        };
        let mut units: Vec<u16> = self
            .bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        if units.last() == Some(&0) {
            units.pop();
        }
        let text = String::from_utf16(&units);
        if self.bytes.len() % 2 != 0 || units.contains(&0) || text.is_err() || name.is_empty() {
            kind = ValueKind::Unsupported;
        }
        let value = if kind == ValueKind::Unsupported {
            format!("不支持编辑的注册表值（类型 {}）", self.kind)
        } else {
            text.expect("已验证 UTF-16")
        };
        Variable {
            name,
            value,
            kind,
            revision: self.revision(),
        }
    }
}

pub(super) fn snapshot(scope: Scope) -> ScopeSnapshot {
    let (root, path) = location(scope);
    let result = Key::open(root, path, KEY_QUERY_VALUE)
        .and_then(|key| key.map(|k| k.variables()).unwrap_or_else(|| Ok(vec![])));
    let writable = match Key::open(root, path, KEY_SET_VALUE) {
        Ok(Some(_)) => true,
        Ok(None) if scope == Scope::User => {
            Key::open(root, "", KEY_CREATE_SUB_KEY).is_ok_and(|k| k.is_some())
        }
        _ => false,
    };
    match result {
        Ok(variables) => ScopeSnapshot {
            scope,
            variables,
            writable,
            error: None,
        },
        Err(error) => ScopeSnapshot {
            scope,
            variables: vec![],
            writable: false,
            error: Some(error),
        },
    }
}

pub(super) fn apply(change: Change) -> Result<ChangeResult, String> {
    let variable = {
        let _guard = WRITE_LOCK.lock().map_err(|e| e.to_string())?;
        let (root, path) = location(change.scope);
        let key = match Key::open(root, path, KEY_QUERY_VALUE | KEY_SET_VALUE)? {
            Some(key) => key,
            None if change.scope == Scope::User => Key::create(root, path)?,
            None => return Err("系统环境变量注册表键不存在".into()),
        };
        key.apply(&change)?
    };
    let area = wide("Environment");
    let delivered = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            1000,
            null_mut(),
        )
    };
    Ok(ChangeResult {
        variable,
        warning: (delivered == 0).then(|| {
            "已保存，但环境变更通知未完成。请重新启动需要使用新变量的程序；必要时注销后登录。"
                .into()
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Sandbox {
        path: String,
        key: Key,
    }
    impl Sandbox {
        fn new() -> Self {
            let path = format!("Software\\MyToolboxTests\\{}", uuid::Uuid::new_v4());
            let key = Key::create(HKEY_CURRENT_USER, &path).unwrap();
            Self { path, key }
        }
        fn change(&self, name: &str, value: Option<&str>, kind: ValueKind) -> Change {
            Change {
                scope: Scope::User,
                name: name.into(),
                value: value.map(str::to_string),
                kind,
                expected_revision: self.key.read(name).unwrap().map(|r| r.revision()),
            }
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            // 仅删除本测试创建的 UUID 子键，绝不访问真实 Environment 键。
            assert!(self.path.starts_with("Software\\MyToolboxTests\\"));
            let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.path).as_ptr()) };
            assert_eq!(status, ERROR_SUCCESS);
        }
    }
    #[test]
    fn isolated_registry_roundtrip_preserves_raw_expansion_unicode_empty_and_long_values() {
        let sandbox = Sandbox::new();
        for (name, value, kind) in [
            (
                "Path",
                "C:\\工具;;%JAVA_HOME%\\bin;",
                ValueKind::ExpandString,
            ),
            ("EMPTY", "", ValueKind::String),
            ("QUOTES", "'\"`$();中文😀", ValueKind::String),
            ("LONG", &"路径😀;".repeat(2000), ValueKind::ExpandString),
        ] {
            let c = sandbox.change(name, Some(value), kind);
            validate(&c).unwrap();
            let saved = sandbox.key.apply(&c).unwrap().unwrap();
            let read = sandbox
                .key
                .read(name)
                .unwrap()
                .unwrap()
                .variable(name.into());
            assert_eq!(read.value, value);
            assert_eq!(read.kind, kind);
            assert_eq!(read.revision, saved.revision);
        }
        assert_eq!(sandbox.key.variables().unwrap().len(), 4);
        let delete = sandbox.change("EMPTY", None, ValueKind::String);
        sandbox.key.apply(&delete).unwrap();
        assert!(sandbox.key.read("EMPTY").unwrap().is_none());
        assert!(sandbox.key.apply(&delete).is_err());
    }
    #[test]
    fn rejects_case_insensitive_duplicate_and_external_value_or_type_change() {
        let sandbox = Sandbox::new();
        sandbox
            .key
            .apply(&sandbox.change("Path", Some("old"), ValueKind::String))
            .unwrap();
        let duplicate = Change {
            expected_revision: None,
            ..sandbox.change("PATH", Some("bad"), ValueKind::String)
        };
        assert!(sandbox.key.apply(&duplicate).is_err());
        let stale = sandbox.change("Path", Some("stale"), ValueKind::String);
        sandbox
            .key
            .apply(&sandbox.change("Path", Some("new"), ValueKind::String))
            .unwrap();
        assert!(sandbox.key.apply(&stale).is_err());
        let stale_type = sandbox.change("Path", None, ValueKind::String);
        sandbox
            .key
            .apply(&sandbox.change("Path", Some("new"), ValueKind::ExpandString))
            .unwrap();
        assert!(sandbox.key.apply(&stale_type).is_err());
        assert_eq!(
            sandbox
                .key
                .read("Path")
                .unwrap()
                .unwrap()
                .variable("Path".into())
                .value,
            "new"
        );
    }
    #[test]
    fn unexpected_registry_types_and_invalid_utf16_are_visible_but_not_writable() {
        for raw in [
            RawValue {
                kind: REG_DWORD,
                bytes: vec![1, 0, 0, 0],
            },
            RawValue {
                kind: REG_SZ,
                bytes: vec![0, 0xd8, 0, 0],
            },
        ] {
            assert_eq!(raw.variable("TEST".into()).kind, ValueKind::Unsupported);
        }
    }
}
