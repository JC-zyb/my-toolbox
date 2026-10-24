use regex::{NoExpand, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, path::Path};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Replace,
    Template,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    #[default]
    Stem,
    Full,
    Extension,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LetterCase {
    #[default]
    Keep,
    Lower,
    Upper,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    Natural,
    Name,
    Modified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Rules {
    pub mode: Mode,
    pub scope: Scope,
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub replace_all: bool,
    pub prefix: String,
    pub suffix: String,
    pub letter_case: LetterCase,
    pub trim: bool,
    pub template: String,
    pub start: u64,
    pub step: u64,
    pub padding: usize,
    pub name_filter: String,
    pub extensions: String,
    pub include_hidden: bool,
    pub sort: Sort,
    pub descending: bool,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            mode: Mode::Replace,
            scope: Scope::Stem,
            find: String::new(),
            replace: String::new(),
            regex: false,
            case_sensitive: true,
            replace_all: true,
            prefix: String::new(),
            suffix: String::new(),
            letter_case: LetterCase::Keep,
            trim: false,
            template: "{name}_{n}".into(),
            start: 1,
            step: 1,
            padding: 3,
            name_filter: String::new(),
            extensions: String::new(),
            include_hidden: false,
            sort: Sort::Natural,
            descending: false,
        }
    }
}

pub struct Transformer<'a> {
    rules: &'a Rules,
    pattern: Option<Regex>,
    template: Vec<Part>,
}
enum Part {
    Text(String),
    Name,
    Ext,
    Number,
}
impl<'a> Transformer<'a> {
    pub fn new(rules: &'a Rules) -> Result<Self, String> {
        if [
            &rules.find,
            &rules.replace,
            &rules.prefix,
            &rules.suffix,
            &rules.template,
            &rules.name_filter,
            &rules.extensions,
        ]
        .iter()
        .any(|s| s.len() > 4096)
        {
            return Err("规则文本不能超过 4096 字节".into());
        }
        let pattern = if rules.mode == Mode::Replace && !rules.find.is_empty() {
            Some(
                RegexBuilder::new(&if rules.regex {
                    rules.find.clone()
                } else {
                    regex::escape(&rules.find)
                })
                .case_insensitive(!rules.case_sensitive)
                .size_limit(2 * 1024 * 1024)
                .build()
                .map_err(|e| format!("查找规则无效：{e}"))?,
            )
        } else {
            None
        };
        if rules.mode == Mode::Replace && rules.regex && rules.find.is_empty() {
            return Err("正则表达式不能为空".into());
        }
        let mut template = vec![];
        if rules.mode == Mode::Template {
            if rules.step == 0
                || rules.padding > 18
                || rules.start > 9_007_199_254_740_991
                || rules.step > 9_007_199_254_740_991
            {
                return Err("编号起始值/步长超出范围，步长须大于零，补零位数为 0–18".into());
            }
            let mut rest = rules.template.as_str();
            while !rest.is_empty() {
                if let Some(i) = rest.find(['{', '}']) {
                    if i > 0 {
                        template.push(Part::Text(rest[..i].into()));
                    }
                    rest = &rest[i..];
                    let (token, part) = if rest.starts_with("{name}") {
                        ("{name}", Part::Name)
                    } else if rest.starts_with("{ext}") {
                        ("{ext}", Part::Ext)
                    } else if rest.starts_with("{n}") {
                        ("{n}", Part::Number)
                    } else {
                        return Err("模板仅支持 {name}、{ext} 和 {n}，请检查花括号".into());
                    };
                    template.push(part);
                    rest = &rest[token.len()..];
                } else {
                    template.push(Part::Text(rest.into()));
                    break;
                }
            }
            if template.is_empty() {
                return Err("命名模板不能为空".into());
            }
        }
        Ok(Self {
            rules,
            pattern,
            template,
        })
    }
    pub fn transform(&self, filename: &str, index: usize) -> Result<String, String> {
        let r = self.rules;
        let (stem, ext) = split_name(filename);
        let input = match r.scope {
            Scope::Stem => stem,
            Scope::Full => filename,
            Scope::Extension => ext,
        };
        let mut name = if r.mode == Mode::Template {
            let n = (index as u64)
                .checked_mul(r.step)
                .and_then(|v| v.checked_add(r.start))
                .filter(|v| *v <= 9_007_199_254_740_991)
                .ok_or("编号超出安全整数范围")?;
            self.template
                .iter()
                .map(|p| match p {
                    Part::Text(s) => s.clone(),
                    Part::Name => stem.into(),
                    Part::Ext => ext.into(),
                    Part::Number => format!("{n:0width$}", width = r.padding),
                })
                .collect::<String>()
        } else if let Some(re) = &self.pattern {
            let limit = if r.replace_all { 0 } else { 1 };
            if r.regex {
                re.replacen(input, limit, r.replace.as_str()).into_owned()
            } else {
                re.replacen(input, limit, NoExpand(&r.replace)).into_owned()
            }
        } else {
            input.into()
        };
        if r.trim {
            name = name.trim().into();
        }
        name = match r.letter_case {
            LetterCase::Keep => name,
            LetterCase::Lower => name.to_lowercase(),
            LetterCase::Upper => name.to_uppercase(),
        };
        name = format!("{}{name}{}", r.prefix, r.suffix);
        Ok(match r.scope {
            Scope::Stem if !ext.is_empty() => format!("{name}.{ext}"),
            Scope::Extension if !name.is_empty() => format!("{stem}.{name}"),
            Scope::Extension => stem.into(),
            _ => name,
        })
    }
}
pub fn split_name(name: &str) -> (&str, &str) {
    let p = Path::new(name);
    (
        p.file_stem().and_then(|s| s.to_str()).unwrap_or(name),
        p.extension().and_then(|s| s.to_str()).unwrap_or(""),
    )
}
pub fn invalid_name(name: &str) -> Option<String> {
    if name.is_empty() || name == "." || name == ".." || name.ends_with(['.', ' ']) {
        return Some("文件名不能为空或以点、空格结尾".into());
    }
    if name
        .chars()
        .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Some("包含 Windows 不允许的字符".into());
    }
    if name.encode_utf16().count() > 255 || name.len() > 255 && !cfg!(windows) {
        return Some("文件名超过文件系统长度限制".into());
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&base.as_str())
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|s| {
                ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].contains(&s)
            })
    {
        return Some("Windows 保留名称不能用于文件名".into());
    }
    None
}
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let (mut x, mut y) = (a.as_str(), b.as_str());
    while !x.is_empty() && !y.is_empty() {
        let cx = x.chars().next().unwrap();
        let cy = y.chars().next().unwrap();
        let order = if cx.is_ascii_digit() && cy.is_ascii_digit() {
            let nx = x.bytes().take_while(u8::is_ascii_digit).count();
            let ny = y.bytes().take_while(u8::is_ascii_digit).count();
            let dx = x[..nx].trim_start_matches('0');
            let dy = y[..ny].trim_start_matches('0');
            let c = dx.len().cmp(&dy.len()).then(dx.cmp(dy)).then(nx.cmp(&ny));
            x = &x[nx..];
            y = &y[ny..];
            c
        } else {
            x = &x[cx.len_utf8()..];
            y = &y[cy.len_utf8()..];
            cx.cmp(&cy)
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    x.len().cmp(&y.len())
}
