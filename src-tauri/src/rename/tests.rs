use super::rules::{LetterCase, Mode, Scope};
use super::*;
fn setup(names: &[&str]) -> (tempfile::TempDir, RenameService) {
    let dir = tempfile::tempdir().unwrap();
    for name in names {
        fs::write(dir.path().join(name), name).unwrap();
    }
    (dir, RenameService::default())
}
fn preview(service: &mut RenameService, dir: &Path, rules: Rules) -> Plan {
    service
        .preview(Request {
            directory: display(dir),
            rules,
        })
        .unwrap()
}
fn selected(p: &Plan) -> Vec<String> {
    p.rows
        .iter()
        .filter(|r| r.from != r.to)
        .map(|r| r.from.clone())
        .collect()
}
#[test]
fn literal_and_regex_replace_respect_scope_occurrence_and_unicode() {
    let (dir, mut s) = setup(&["照片_A_A.JPG"]);
    let mut r = Rules {
        find: "a".into(),
        replace: "$1".into(),
        case_sensitive: false,
        replace_all: false,
        ..Rules::default()
    };
    let p = preview(&mut s, dir.path(), r.clone());
    assert_eq!(p.rows[0].to_name, "照片_$1_A.JPG");
    r.find = "(照片)_(A)".into();
    r.replace = "${2}_${1}".into();
    r.regex = true;
    r.replace_all = true;
    assert_eq!(
        preview(&mut s, dir.path(), r.clone()).rows[0].to_name,
        "A_照片_A.JPG"
    );
    r = Rules {
        scope: Scope::Extension,
        letter_case: LetterCase::Lower,
        ..Rules::default()
    };
    assert_eq!(
        preview(&mut s, dir.path(), r).rows[0].to_name,
        "照片_A_A.jpg"
    );
    r = Rules {
        scope: Scope::Full,
        letter_case: LetterCase::Lower,
        ..Rules::default()
    };
    assert_eq!(
        preview(&mut s, dir.path(), r).rows[0].to_name,
        "照片_a_a.jpg"
    );
}
#[test]
fn template_natural_sort_filter_numbering_and_hidden_files() {
    let (dir, mut s) = setup(&[
        "pic10.jpg",
        "pic2.jpg",
        "pic1.JPG",
        ".hidden.jpg",
        "other.txt",
    ]);
    fs::create_dir(dir.path().join("child")).unwrap();
    let r = Rules {
        mode: Mode::Template,
        template: "相片_{n}".into(),
        start: 8,
        step: 2,
        padding: 3,
        extensions: "jpg".into(),
        ..Rules::default()
    };
    let p = preview(&mut s, dir.path(), r.clone());
    assert_eq!(
        p.rows
            .iter()
            .map(|r| r.from_name.as_str())
            .collect::<Vec<_>>(),
        ["pic1.JPG", "pic2.jpg", "pic10.jpg"]
    );
    assert_eq!(p.rows[2].to_name, "相片_012.jpg");
    assert_eq!(p.skipped, 3);
    let mut r = r;
    r.descending = true;
    r.include_hidden = true;
    r.name_filter = "pic".into();
    let p = preview(&mut s, dir.path(), r);
    assert_eq!(p.rows[0].from_name, "pic10.jpg");
}
#[test]
fn template_full_name_extensionless_dotfiles_and_trim() {
    let (dir, mut s) = setup(&["  alpha .txt", "README", ".config"]);
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            trim: true,
            prefix: "new_".into(),
            include_hidden: true,
            ..Rules::default()
        },
    );
    assert!(p.rows.iter().any(|r| r.to_name == "new_alpha.txt"));
    assert!(p.rows.iter().any(|r| r.to_name == "new_.config"));
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            scope: Scope::Full,
            template: "{n}_{name}.{ext}".into(),
            ..Rules::default()
        },
    );
    assert!(p
        .rows
        .iter()
        .any(|r| r.to_name.ends_with("README.") && r.error.is_some()));
}
#[test]
fn invalid_rules_names_and_reserved_devices_are_explicit() {
    let (dir, mut s) = setup(&["a.txt"]);
    for r in [
        Rules {
            regex: true,
            find: "(".into(),
            ..Rules::default()
        },
        Rules {
            mode: Mode::Template,
            template: "{unknown}".into(),
            ..Rules::default()
        },
        Rules {
            mode: Mode::Template,
            step: 0,
            ..Rules::default()
        },
    ] {
        assert!(s
            .preview(Request {
                directory: display(dir.path()),
                rules: r
            })
            .is_err());
    }
    for name in [
        "CON.txt",
        "LPT¹.txt",
        "nul",
        "../x.txt",
        "x.",
        "x ",
        "a\u{0}.txt",
        "",
    ] {
        assert!(rules::invalid_name(name).is_some(), "{name}");
    }
    for name in ["照片.txt", ".gitignore", "COM10.txt", "a..txt"] {
        assert!(rules::invalid_name(name).is_none(), "{name}");
    }
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            template: "CON".into(),
            ..Rules::default()
        },
    );
    assert!(p.rows[0].error.is_some());
    assert!(s.apply(&p.id, &selected(&p)).is_err());
}
#[test]
fn duplicate_targets_mark_whole_batch_and_allow_selecting_one() {
    let (dir, mut s) = setup(&["a.txt", "b.txt"]);
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            template: "same".into(),
            ..Rules::default()
        },
    );
    assert!(s.apply(&p.id, &selected(&p)).unwrap_err().contains("重复"));
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            template: "same".into(),
            ..Rules::default()
        },
    );
    assert_eq!(s.apply(&p.id, &[p.rows[0].from.clone()]).unwrap().count, 1);
    assert!(dir.path().join("b.txt").exists());
}
#[test]
fn chain_rename_case_change_and_undo_preserve_contents() {
    let (dir, mut s) = setup(&["1.txt", "2.txt"]);
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            template: "{n}".into(),
            start: 2,
            padding: 0,
            ..Rules::default()
        },
    );
    // 1→2 依赖所选的 2→3，取消后一项就不能腾出 2。
    assert!(s
        .apply(&p.id, &[p.rows[0].from.clone()])
        .unwrap_err()
        .contains("占用"));
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            template: "{n}".into(),
            start: 2,
            padding: 0,
            ..Rules::default()
        },
    );
    let result = s.apply(&p.id, &selected(&p)).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("2.txt")).unwrap(),
        "1.txt"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("3.txt")).unwrap(),
        "2.txt"
    );
    assert_eq!(s.undo_preview(&result.undo.id).unwrap().rows.len(), 2);
    assert_eq!(s.undo(&result.undo.id).unwrap(), 2);
    assert!(s.undo(&result.undo.id).is_err());
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            scope: Scope::Extension,
            letter_case: LetterCase::Upper,
            ..Rules::default()
        },
    );
    let result = s.apply(&p.id, &selected(&p)).unwrap();
    assert_eq!(result.count, 2);
    s.undo(&result.undo.id).unwrap();
}
#[test]
fn cyclic_swap_uses_temporary_names_and_undo() {
    let (dir, mut s) = setup(&["1.txt", "2.txt"]);
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            mode: Mode::Template,
            template: "{n}".into(),
            padding: 0,
            descending: true,
            ..Rules::default()
        },
    );
    let result = s.apply(&p.id, &selected(&p)).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("1.txt")).unwrap(),
        "2.txt"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("2.txt")).unwrap(),
        "1.txt"
    );
    s.undo(&result.undo.id).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("1.txt")).unwrap(),
        "1.txt"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn changed_files_new_occupants_and_expired_or_replayed_plans_are_rejected() {
    let (dir, mut s) = setup(&["a.txt"]);
    let r = Rules {
        prefix: "new_".into(),
        ..Rules::default()
    };
    let p = preview(&mut s, dir.path(), r.clone());
    fs::write(dir.path().join("a.txt"), "edited").unwrap();
    assert!(s
        .apply(&p.id, &selected(&p))
        .unwrap_err()
        .contains("已改变"));
    let p = preview(&mut s, dir.path(), r.clone());
    fs::create_dir(dir.path().join("new_a.txt")).unwrap();
    assert!(s.apply(&p.id, &selected(&p)).unwrap_err().contains("占用"));
    fs::remove_dir(dir.path().join("new_a.txt")).unwrap();
    let p = preview(&mut s, dir.path(), r.clone());
    s.plan.as_mut().unwrap().created = Instant::now() - PREVIEW_TTL - Duration::from_secs(1);
    assert!(s.apply(&p.id, &selected(&p)).unwrap_err().contains("过期"));
    let p = preview(&mut s, dir.path(), r);
    let result = s.apply(&p.id, &selected(&p)).unwrap();
    assert!(s.apply(&p.id, &selected(&p)).is_err());
    fs::write(dir.path().join("new_a.txt"), "later edit").unwrap();
    assert!(s.undo(&result.undo.id).unwrap_err().contains("已改变"));
    assert_eq!(
        fs::read_to_string(dir.path().join("new_a.txt")).unwrap(),
        "later edit"
    );
}
#[test]
fn replaced_source_or_directory_is_not_the_previewed_identity() {
    let (dir, mut s) = setup(&["a.txt"]);
    let r = Rules {
        prefix: "new_".into(),
        ..Rules::default()
    };
    let p = preview(&mut s, dir.path(), r.clone());
    fs::rename(dir.path().join("a.txt"), dir.path().join("old.txt")).unwrap();
    fs::write(dir.path().join("a.txt"), "a.txt").unwrap();
    assert!(s
        .apply(&p.id, &selected(&p))
        .unwrap_err()
        .contains("已改变"));
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), "a").unwrap();
    let p = preview(&mut s, &root, r);
    fs::rename(&root, parent.path().join("old")).unwrap();
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), "replacement").unwrap();
    assert!(s
        .apply(&p.id, &selected(&p))
        .unwrap_err()
        .contains("目录已改变"));
}
#[test]
fn paths_outside_preview_and_repeated_selection_cannot_be_applied() {
    let (dir, mut s) = setup(&["a.txt"]);
    let r = Rules {
        prefix: "new_".into(),
        ..Rules::default()
    };
    let p = preview(&mut s, dir.path(), r.clone());
    assert!(s.apply(&p.id, &["C:/outside".into()]).is_err());
    let p = preview(&mut s, dir.path(), r);
    assert!(s
        .apply(&p.id, &[p.rows[0].from.clone(), p.rows[0].from.clone()])
        .is_err());
}
#[test]
fn undo_does_not_overwrite_recreated_original_name() {
    let (dir, mut s) = setup(&["a.txt"]);
    let p = preview(
        &mut s,
        dir.path(),
        Rules {
            prefix: "new_".into(),
            ..Rules::default()
        },
    );
    let result = s.apply(&p.id, &selected(&p)).unwrap();
    fs::write(dir.path().join("a.txt"), "external").unwrap();
    assert!(s
        .undo_preview(&result.undo.id)
        .unwrap_err()
        .contains("占用"));
    assert!(s.undo(&result.undo.id).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "external"
    );
}
