use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod windows;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    User,
    System,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    String,
    ExpandString,
    Unsupported,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub kind: ValueKind,
    pub revision: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSnapshot {
    pub scope: Scope,
    pub variables: Vec<Variable>,
    pub writable: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub supported: bool,
    pub scopes: Vec<ScopeSnapshot>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub scope: Scope,
    pub name: String,
    // None 表示删除，Some("") 是有效的空字符串。
    pub value: Option<String>,
    pub kind: ValueKind,
    pub expected_revision: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeResult {
    pub variable: Option<Variable>,
    pub warning: Option<String>,
}

pub fn list() -> Snapshot {
    #[cfg(windows)]
    return Snapshot {
        supported: true,
        scopes: [Scope::User, Scope::System]
            .into_iter()
            .map(windows::snapshot)
            .collect(),
    };
    #[cfg(not(windows))]
    Snapshot {
        supported: false,
        scopes: vec![],
    }
}

pub fn apply(change: Change) -> Result<ChangeResult, String> {
    validate(&change)?;
    #[cfg(windows)]
    return windows::apply(change);
    #[cfg(not(windows))]
    Err("环境变量编辑目前仅支持 Windows 桌面版".into())
}

fn validate(change: &Change) -> Result<(), String> {
    if change.name.is_empty() || change.name.contains(['=', '\0']) {
        return Err("变量名不能为空，也不能包含等号或 NUL 字符".into());
    }
    if let Some(value) = &change.value {
        if change.name.chars().any(char::is_control) {
            return Err("变量名不能包含控制字符".into());
        }
        if value.contains('\0') {
            return Err("变量值不能包含 NUL 字符".into());
        }
        // name=value\0 采用 UTF-16 计数，与 Windows 环境块一致。
        if change.name.encode_utf16().count() + value.encode_utf16().count() + 2 > 32_767 {
            return Err("变量名与值的总长度超出 Windows 的 32767 个 UTF-16 单元限制".into());
        }
    } else if change.expected_revision.is_none() {
        return Err("删除变量必须提供读取时的版本，请刷新后重试".into());
    }
    if change.kind == ValueKind::Unsupported {
        return Err("此注册表值类型不支持编辑或删除".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names_nul_and_utf16_budget_without_rejecting_empty_values() {
        let mut c = Change {
            scope: Scope::User,
            name: "TEST".into(),
            value: Some("".into()),
            kind: ValueKind::String,
            expected_revision: None,
        };
        assert!(validate(&c).is_ok());
        for name in ["", "A=B", "A\0B", "A\nB"] {
            c.name = name.into();
            assert!(validate(&c).is_err());
        }
        c.name = "TEST".into();
        c.value = Some("x\0y".into());
        assert!(validate(&c).is_err());
        c.value = Some("😀".repeat(16_381));
        assert!(validate(&c).is_err());
        c.value = None;
        assert!(validate(&c).is_err());
    }
}
