use super::types::*;

pub fn catalog() -> Vec<ToolDefinition> {
    [
        (
            "node",
            "Node.js",
            "JavaScript 运行时",
            "node",
            "https://nodejs.org",
            "nodejs/node",
            Some("fnm"),
            "OpenJS.NodeJS.LTS",
            "nodejs-lts",
            "node",
        ),
        (
            "fnm",
            "fnm",
            "快速 Node.js 版本管理器",
            "fnm",
            "https://github.com/Schniz/fnm",
            "Schniz/fnm",
            None,
            "Schniz.fnm",
            "fnm",
            "fnm",
        ),
        (
            "uv",
            "uv",
            "Python 项目、包与解释器管理",
            "uv",
            "https://docs.astral.sh/uv",
            "astral-sh/uv",
            None,
            "astral-sh.uv",
            "uv",
            "uv",
        ),
        (
            "python",
            "Python",
            "Python 解释器",
            "python,python3",
            "https://www.python.org",
            "python/cpython",
            Some("uv"),
            "",
            "python",
            "python",
        ),
        (
            "git",
            "Git",
            "分布式版本控制",
            "git",
            "https://git-scm.com",
            "git/git",
            None,
            "Git.Git",
            "git",
            "git",
        ),
        (
            "pnpm",
            "pnpm",
            "Node.js 包管理器",
            "pnpm",
            "https://pnpm.io",
            "pnpm/pnpm",
            Some("mise"),
            "pnpm.pnpm",
            "pnpm",
            "pnpm",
        ),
        (
            "bun",
            "Bun",
            "JavaScript 工具链和运行时",
            "bun",
            "https://bun.sh",
            "oven-sh/bun",
            Some("mise"),
            "Oven-sh.Bun",
            "bun",
            "oven-sh/bun/bun",
        ),
        (
            "deno",
            "Deno",
            "JavaScript 与 TypeScript 运行时",
            "deno",
            "https://deno.com",
            "denoland/deno",
            Some("mise"),
            "DenoLand.Deno",
            "deno",
            "deno",
        ),
        (
            "java",
            "Java",
            "Java 开发工具包（mise 可选择发行版）",
            "java",
            "https://adoptium.net",
            "adoptium/temurin-build",
            Some("mise"),
            "EclipseAdoptium.Temurin.25.JDK",
            "java/temurin25-jdk",
            "openjdk",
        ),
        (
            "maven",
            "Maven",
            "Java 构建工具",
            "mvn",
            "https://maven.apache.org",
            "apache/maven",
            Some("mise"),
            "",
            "maven",
            "maven",
        ),
        (
            "rust",
            "Rust",
            "Rust 编译器与工具链",
            "rustc",
            "https://www.rust-lang.org",
            "rust-lang/rust",
            Some("rustup"),
            "",
            "",
            "",
        ),
        (
            "rustup",
            "rustup",
            "Rust 工具链管理器",
            "rustup",
            "https://rustup.rs",
            "rust-lang/rustup",
            None,
            "Rustlang.Rustup",
            "rustup",
            "rustup",
        ),
        (
            "cargo",
            "Cargo",
            "Rust 包与构建工具（跟随 Rust 工具链）",
            "cargo",
            "https://doc.rust-lang.org/cargo",
            "rust-lang/cargo",
            Some("rustup"),
            "",
            "",
            "",
        ),
        (
            "mise",
            "mise",
            "多语言开发工具版本管理器",
            "mise",
            "https://mise.jdx.dev",
            "jdx/mise",
            None,
            "jdx.mise",
            "mise",
            "mise",
        ),
    ]
    .into_iter()
    .map(
        |(id, name, description, commands, homepage, repo, runtime, winget, scoop, brew)| {
            let packages = [("winget", winget), ("scoop", scoop), ("brew", brew)]
                .into_iter()
                .filter(|(_, p)| !p.is_empty())
                .map(|(provider, package)| PackageMapping {
                    provider: provider.into(),
                    package: package.into(),
                })
                .collect();
            ToolDefinition {
                id: id.into(),
                name: name.into(),
                description: description.into(),
                commands: commands.split(',').map(str::to_string).collect(),
                homepage: homepage.into(),
                repository: format!("https://github.com/{repo}"),
                runtime_provider: runtime.map(str::to_string),
                packages,
                registered: false,
            }
        },
    )
    .collect()
}

pub fn provider_guide(id: &str) -> &'static str {
    match id {
        "fnm" => "https://github.com/Schniz/fnm#shell-setup",
        "uv" => "https://docs.astral.sh/uv/getting-started/installation/",
        "mise" => "https://mise.jdx.dev/getting-started.html",
        "rustup" => "https://rust-lang.github.io/rustup/installation/index.html",
        "winget" => "https://learn.microsoft.com/windows/package-manager/winget/",
        "scoop" => "https://scoop.sh/",
        "brew" => "https://brew.sh/",
        "cargo" => "https://doc.rust-lang.org/cargo/commands/cargo-install.html",
        _ => "",
    }
}

pub fn provider_ids() -> Vec<&'static str> {
    let mut ids = vec!["fnm", "uv", "mise", "rustup", "cargo"];
    if cfg!(windows) {
        ids.extend(["winget", "scoop"]);
    } else {
        ids.push("brew");
    }
    ids
}
