//! End to end tests against a throwaway workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn generic_negation_suppresses_defaults_and_obeys_argument_order() {
    let base = root("generic-negation-defaults");
    for title in ["Alpha", "Beta"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    let workspace = format!(
        "{WORKSPACE}\n[commands.\"tasks.list\"]\nfilter = 'title = Alpha'\nsort = 'title'\ncolumns = ['title']\njson = true\ndesc = true\n[commands.\"tasks.new\"]\nevidence = 'configured.md'\n"
    );
    write(&base, ".clerk/workspace.toml", &workspace);
    let initial = run(&base, &["tasks", "list"]);
    assert_eq!(initial.0, 0, "{}", initial.2);
    assert!(initial.1.contains("Alpha") && !initial.1.contains("Beta"));
    let result = run(
        &base,
        &[
            "tasks",
            "list",
            "--no",
            "filter",
            "-Nsort",
            "--no=columns",
            "-N=json",
            "--no",
            "desc",
        ],
    );
    assert_eq!(result.0, 0, "{}", result.2);
    assert!(
        result.1.contains("Alpha") && result.1.contains("Beta") && result.1.contains("APP-001"),
        "{}",
        result.1
    );
    assert!(result.1.find("Alpha") < result.1.find("Beta"));
    assert!(!result.1.starts_with('{'));
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "test baseline"]);
    let before = read(&base, ".clerk/data/tasks.csv");
    assert_eq!(
        run(
            &base,
            &[
                "tasks", "new", "--title", "Captured", "--no", "evidence", "--batch", "work"
            ]
        )
        .0,
        0
    );
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    let applied = run(&base, &["batch", "apply", "work"]);
    assert_eq!(applied.0, 0, "{}", applied.2);
    let record = run(&base, &["tasks", "show", "APP-003", "--json"]);
    let data: serde_json::Value = serde_json::from_str(&record.1).unwrap();
    assert_eq!(data["data"][0]["evidence"], "");
    for (flags, expected) in [
        (vec!["--color", "always", "--no", "color"], false),
        (vec!["--no", "color", "--color", "always"], true),
        (vec!["--no-color", "--color", "always"], true),
        (vec!["--color", "always", "--no-color"], false),
    ] {
        let mut args = vec!["tasks", "list", "--no", "json"];
        args.extend(flags);
        let result = run(&base, &args);
        assert_eq!(result.0, 0, "{}", result.2);
        assert_eq!(result.1.contains('\x1b'), expected, "{args:?}");
    }
    assert_eq!(workspace, read(&base, ".clerk/workspace.toml"));
}

#[test]
fn generic_negation_candidates_are_contextual_and_share_help_completion_and_schema() {
    let base = root("generic-negation-discovery");
    let ambiguous = run(&base, &["tasks", "list", "-N", "c"]);
    assert_eq!(ambiguous.0, 2);
    assert!(
        ambiguous.2.contains("ambiguous")
            && ambiguous.2.contains("color")
            && ambiguous.2.contains("columns"),
        "{}",
        ambiguous.2
    );
    for value in ["file", "unknown", ""] {
        assert_eq!(run(&base, &["tasks", "list", "--no", value]).0, 2);
    }
    let result = run(&base, &["tas", "lis", "-N", "colo"]);
    assert_eq!(result.0, 0, "{}", result.2);
    let result = run(&base, &["tasks", "list", "--no", "plain", "--json"]);
    assert_eq!(result.0, 0, "{}", result.2);
    assert!(serde_json::from_str::<serde_json::Value>(&result.1).is_ok());
    assert_eq!(run(&base, &["tasks", "new", "--no", "filter"]).0, 2);
    let completion = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            "[\"tasks\",\"list\",\"-N\"]",
            "--current",
            "colo",
        ],
    );
    assert_eq!(completion.0, 0, "{}", completion.2);
    assert!(completion.1.contains("color") && !completion.1.contains("columns"));
    let result = run(&base, &["explain", "tasks", "--json"]);
    let schema: serde_json::Value = serde_json::from_str(&result.1).unwrap();
    let options = schema["command_options"].as_array().unwrap();
    let no = options
        .iter()
        .find(|option| option["name"] == "no")
        .unwrap();
    assert_eq!(no["short"], "N");
    assert_eq!(no["cli_prefix"], true);
    assert!(
        no["no_targets"]["list"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "filter")
    );
    assert!(
        !no["no_targets"]["new"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "filter")
    );
    assert!(
        no["no_targets"]["new"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "evidence")
    );
    let color = options
        .iter()
        .find(|option| option["name"] == "color")
        .unwrap();
    assert_eq!(color["negation"]["value"], "never");
    let help = run(&base, &["tasks", "list", "--help"]);
    assert!(help.1.contains("-N") && help.1.contains("--no <option>"));
    assert!(help.1.contains("configured default"));
}

#[test]
fn structured_help_fits_every_command_and_is_read_only() {
    let base = root("help-viewport-boundaries");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Preserved"]).0, 0);
    let before = read(&base, ".clerk/data/tasks.csv");
    let mut commands = clerk::interface::command::BUILTINS
        .iter()
        .map(|builtin| builtin.name)
        .collect::<Vec<_>>();
    commands.extend(["tasks", "contracts", "reviews", "logs"]);
    for command in commands {
        for width in ["20", "48", "100"] {
            let result = run(
                &base,
                &[command, "--help", "--width", width, "--color", "never"],
            );
            assert_eq!(result.0, 0, "{command} {width}: {}", result.2);
            let width: usize = width.parse().unwrap();
            assert!(
                result
                    .1
                    .lines()
                    .all(|line| clerk::terminal::text::display_width(line) <= width),
                "{command} {width}: {}",
                result.1
            );
            assert!(!result.1.contains('\x1b'));
        }
    }
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    let overview = run(&base, &["help"]);
    assert!(overview.1.contains("Common options:") && overview.1.contains("Workflows:"));
    assert!(
        !overview.1.contains("--filter")
            && !overview.1.contains("--message")
            && !overview.1.contains("--columns")
    );
    let session = run(&base, &["session", "scope", "--help"]);
    assert_eq!(session.0, 0, "{}", session.2);
    assert!(session.1.contains("clerk session scope <id>"));
}

#[test]
fn contextual_help_matches_batch_validation_completion_and_effective_defaults() {
    let base = root("help-contextual-options");
    let help = run(&base, &["batch", "--help"]);
    assert_eq!(help.0, 0, "{}", help.2);
    assert!(!help.1.contains("--request"));
    assert!(help.1.contains("read a workflow from a file"));
    let show = run(&base, &["batch", "show", "--help"]);
    for absent in ["--file", "--template", "--message", "--sync", "--preview"] {
        assert!(!show.1.contains(absent), "{absent}: {}", show.1);
    }
    let completion = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["batch"]"#,
            "--current=--request",
            "--json",
        ],
    );
    assert_eq!(completion.0, 0, "{}", completion.2);
    assert!(
        serde_json::from_str::<serde_json::Value>(&completion.1)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let completion = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["batch"]"#,
            "--current=--file",
            "--json",
        ],
    );
    assert!(completion.1.contains("read a workflow from a file"));
    assert_eq!(run(&base, &["batch", "--request", "bad"]).0, 2);
    write(
        &base,
        ".clerk/workspace.toml",
        &(read(&base, ".clerk/workspace.toml") + "\n[commands.\"tasks.list\"]\nheaders = 'hide'\n"),
    );
    let help = run(&base, &["tasks", "list", "--help"]);
    assert_eq!(help.0, 0, "{}", help.2);
    assert!(help.1.contains("Default: hide."));
    assert!(help.1.contains("--data-scope <scope>") && !help.1.contains("<current|archived|all>"));
    let schema = run(&base, &["explain", "tasks", "--json"]);
    let schema: serde_json::Value = serde_json::from_str(&schema.1).unwrap();
    let scope = schema["command_options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "data-scope")
        .unwrap();
    assert_eq!(scope["argument"], "<scope>");
    assert_eq!(scope["initial"], "current");
    assert!(scope["description"].as_str().unwrap().contains("archived"));
}

#[test]
fn short_alias_contract_is_unique_and_formats_are_long_only() {
    use clerk::interface::{cli::Args, command};
    let expected = [
        ('b', "batch"),
        ('t', "template"),
        ('i', "file"),
        ('m', "message"),
        ('M', "message-file"),
        ('C', "root"),
        ('c', "columns"),
        ('w', "width"),
        ('D', "details"),
        ('F', "full"),
        ('n', "dry-run"),
        ('P', "preview"),
        ('N', "no"),
        ('s', "sort"),
        ('r', "desc"),
        ('l', "long"),
        ('f', "filter"),
        ('d', "display"),
        ('o', "overflow"),
        ('p', "paging"),
        ('S', "data-scope"),
        ('q', "quiet"),
        ('h', "help"),
        ('v', "version"),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for option in command::OPTIONS {
        if let Some(short) = option.short {
            assert!(seen.insert(short), "duplicate short alias {short}");
            assert!(
                expected.contains(&(short, option.name)),
                "unexpected short alias {short}"
            );
        }
    }
    assert_eq!(seen.len(), expected.len());
    for (short, name) in expected {
        assert_eq!(command::long_of_short(short), Some(name));
    }
    for format in ["json", "tsv", "csv", "md"] {
        assert_eq!(command::option(format).unwrap().short, None);
        assert!(
            Args::parse(&[format!("--{format}")])
                .unwrap()
                .switch(format)
        );
    }
    assert!(Args::parse(&["-j".into()]).is_err());
    let args = Args::parse(&[
        "-mUpdate records".into(),
        "-tstart".into(),
        "-cid,title".into(),
    ])
    .unwrap();
    assert_eq!(args.value("message"), Some("Update records"));
    assert_eq!(args.value("template"), Some("start"));
    assert_eq!(args.value("columns"), Some("id,title"));
    assert_eq!(args.format(), clerk::terminal::output::Format::Table);
}

#[test]
fn interactive_short_aliases_execute_with_the_same_governance() {
    let base = root("human-short-aliases");
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &format!("{TASK_KIND}\n[context]\nattachments = ['brief']\n"),
    );
    write(
        &base,
        "body.md",
        &format!(
            "# Body\n\n{}\nFULL-BODY-END\n",
            "Complete input body.\n".repeat(500)
        ),
    );
    write(
        &base,
        "message.txt",
        "feat(tasks): apply captured work\n\nApply the reviewed task.\n\nVerified by the alias regression.\n",
    );
    write(
        &base,
        ".clerk/config/workflows/create.toml",
        "version = 1\ntitle = 'Create'\n[[param]]\nname = 'subject'\ntype = 'text'\nrequired = true\n[[step]]\nkind = 'tasks'\nverb = 'new'\nvalues = { title = '${params.subject}' }\n",
    );
    enable_auto_commit(&base);
    let captured = run(
        &base,
        &["tasks", "new", "--title", "Captured", "-b", "work"],
    );
    assert_eq!(captured.0, 0, "{}", captured.2);
    let preview = run(&base, &["batch", "apply", "work", "-P"]);
    assert_eq!(preview.0, 0, "{}", preview.2);
    let before = read(&base, ".clerk/data/tasks.csv");
    let dry = run(&base, &["batch", "apply", "work", "-n", "-m", "Dry run"]);
    assert_eq!(dry.0, 0, "{}", dry.2);
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    let applied = run(
        &base,
        &[
            "batch",
            "apply",
            "work",
            "-M",
            base.join("message.txt").to_str().unwrap(),
        ],
    );
    assert_eq!(applied.0, 0, "{}", applied.2);
    assert!(git(&base, &["log", "-1", "--format=%B"]).contains("apply captured work"));
    let attached = run(
        &base,
        &[
            "tasks",
            "brief",
            "APP-001",
            "-i",
            base.join("body.md").to_str().unwrap(),
            "-m",
            "Attach task brief",
        ],
    );
    assert_eq!(attached.0, 0, "{}", attached.2);
    assert_eq!(read(&base, "tasks/APP-001.md"), read(&base, "body.md"));
    assert!(git(&base, &["log", "-1", "--format=%B"]).contains("Attach task brief"));
    let template = run(
        &base,
        &[
            "batch",
            "-t",
            "create",
            "--subject",
            "Templated",
            "-n",
            "-m",
            "Rehearse template",
        ],
    );
    assert_eq!(template.0, 0, "{}", template.2);
    assert!(!read(&base, ".clerk/data/tasks.csv").contains("Templated"));
    let context = run(&base, &["context", "tasks", "APP-001", "-F", "--json"]);
    let complete = run(&base, &["context", "tasks", "APP-001", "--full", "--json"]);
    assert_eq!(context.0, 0, "{}", context.2);
    let mut short_json: serde_json::Value = serde_json::from_str(&context.1).unwrap();
    let mut full_json: serde_json::Value = serde_json::from_str(&complete.1).unwrap();
    short_json.as_object_mut().unwrap().remove("observed_at");
    full_json.as_object_mut().unwrap().remove("observed_at");
    assert_eq!(short_json, full_json);
    assert!(context.1.contains("FULL-BODY-END"));
    assert!(
        !run(&base, &["context", "tasks", "APP-001", "--json"])
            .1
            .contains("FULL-BODY-END")
    );
    let short = run(&base, &["tasks", "list", "-D", "-w100"]);
    let long = run(&base, &["tasks", "list", "--details", "--width", "100"]);
    assert_eq!(short.0, 0, "{}", short.2);
    assert_eq!(short.1, long.1);
    let columns = run(&base, &["tasks", "list", "-c", "id,title"]);
    assert_eq!(columns.0, 0, "{}", columns.2);
    assert!(columns.1.contains("Captured") && !columns.1.contains("state"));
    let rooted = run(
        &base,
        &["-C", base.to_str().unwrap(), "tasks", "list", "--json"],
    );
    assert_eq!(rooted.0, 0, "{}", rooted.2);
    assert_eq!(rooted.1, run(&base, &["tasks", "list", "--json"]).1);
    assert_eq!(git(&base, &["status", "--porcelain"]).trim(), "");
}

#[test]
fn help_completion_and_schema_reflect_reassigned_short_aliases() {
    let base = root("alias-help-completion-schema");
    let help = run(&base, &["tasks", "list", "--help"]);
    assert_eq!(help.0, 0, "{}", help.2);
    assert!(help.1.contains("-c --columns") && help.1.contains("-D --details"));
    assert!(
        !help.1.contains("-j, --json")
            && !help.1.contains("-t, --tsv")
            && !help.1.contains("-c, --csv")
            && !help.1.contains("-m, --md")
    );
    for (words, current, expected) in [
        (r#"["tasks","list"]"#, "-c", "-c, --columns"),
        (r#"["tasks","new"]"#, "-m", "-m, --message"),
        (r#"["batch"]"#, "-t", "-t, --template"),
    ] {
        let result = run(
            &base,
            &[
                "completion",
                "complete",
                "--words-json",
                words,
                &format!("--current={current}"),
                "--json",
            ],
        );
        assert_eq!(result.0, 0, "{}", result.2);
        assert!(result.1.contains(expected), "{}", result.1);
    }
    let result = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","list"]"#,
            "--current=-j",
            "--json",
        ],
    );
    assert_eq!(result.0, 0, "{}", result.2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result.1)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let schema = run(&base, &["explain", "tasks", "--json"]);
    let schema: serde_json::Value = serde_json::from_str(&schema.1).unwrap();
    for option in schema["command_options"].as_array().unwrap() {
        if ["json", "tsv", "csv", "md"].contains(&option["name"].as_str().unwrap()) {
            assert!(option["short"].is_null());
        }
        if option["name"] == "message" {
            assert_eq!(option["short"], "m");
        }
        if option["name"] == "columns" {
            assert_eq!(option["short"], "c");
        }
    }
}

#[test]
fn option_syntax_supports_attached_values_clusters_and_literal_arguments() {
    use clerk::interface::cli::Args;
    let parse =
        |words: &[&str]| Args::parse(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    for words in [
        vec![
            "tasks",
            "list",
            "--json",
            "--data-scope",
            "all",
            "--overflow=pager",
            "--paging=always",
        ],
        vec!["tasks", "list", "--json", "-rSall", "-op", "-pal"],
        vec!["tasks", "list", "--json", "-S=al", "-o=p", "-p=al"],
    ] {
        let args = parse(&words).unwrap();
        assert!(args.switch("json"));
        assert_eq!(args.value("data-scope"), Some("all"));
        assert_eq!(args.value("overflow"), Some("pager"));
        assert_eq!(args.value("paging"), Some("always"));
    }
    let args = parse(&[
        "tasks",
        "new",
        "--title=-hello world",
        "-s中文路径",
        "--",
        "--color",
        "-z",
        "--",
    ])
    .unwrap();
    assert_eq!(args.value("title"), Some("-hello world"));
    assert_eq!(args.value("sort"), Some("中文路径"));
    assert_eq!(args.positional, ["tasks", "new", "--color", "-z", "--"]);
    assert!(parse(&["-rq"]).unwrap().switch("quiet"));
    for words in [
        vec!["-j=true"],
        vec!["-rq=false"],
        vec!["--json=false"],
        vec!["-S"],
        vec!["-z"],
        vec!["--color="],
        vec!["--data-scope=a"],
    ] {
        assert!(parse(&words).is_err(), "{words:?}");
    }
    let args = parse(&["--json", "--no-json"]).unwrap();
    assert!(!args.switch("json"));
}

#[test]
fn all_builtin_enums_resolve_unique_prefixes_but_config_stays_exact() {
    use clerk::interface::{cli::Args, command};
    for spec in command::OPTIONS {
        let choices = command::choices(spec.name);
        for value in choices {
            let full = Args::parse(&[format!("--{}={value}", spec.name)]).unwrap();
            assert_eq!(full.value(spec.name), Some(*value));
            let prefix = (1..=value.len())
                .map(|len| &value[..len])
                .find(|prefix| {
                    choices
                        .iter()
                        .filter(|choice| choice.starts_with(prefix))
                        .count()
                        == 1
                })
                .unwrap();
            let args = Args::parse(&[format!("--{}={prefix}", spec.name)]).unwrap();
            assert_eq!(args.value(spec.name), Some(*value));
            assert!(spec.validate(prefix).is_ok() == (prefix == *value));
        }
    }
    let err = Args::parse(&["--color=a".into()])
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("ambiguous") && err.contains("auto") && err.contains("always"));
    let base = root("enum-config-exact");
    write(
        &base,
        ".clerk/workspace.toml",
        &(read(&base, ".clerk/workspace.toml")
            + "\n[commands.\"tasks.list\"]\ndata-scope = 'ar'\n"),
    );
    assert_ne!(run(&base, &["tasks", "list"]).0, 0);
}

#[test]
fn field_enum_prefixes_are_canonical_before_writes_and_local_capture() {
    let base = root("field-prefix-capture");
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &(TASK_KIND.to_string()
            + "\n[[field]]\nname = 'types'\ntype = 'enum'\nregistry = 'type'\nmulti = true\n[[field]]\nname = 'enabled'\ntype = 'bool'\n"),
    );
    let completed = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","new"]"#,
            "--current=--types=a|d",
            "--json",
        ],
    );
    assert_eq!(completed.0, 0, "{}", completed.2);
    assert!(completed.1.contains("--types=a|defect"), "{}", completed.1);
    let capture = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Captured",
            "--types",
            "a|d",
            "--enabled",
            "t",
            "--prefix",
            "A",
            "--batch",
            "enum",
        ],
    );
    assert_eq!(capture.0, 0, "{}", capture.2);
    let draft = read(&base, ".git/clerk/batches/enum.toml");
    assert!(
        draft.contains("ability|defect") && draft.contains("true") && draft.contains("APP"),
        "{draft}"
    );
    assert!(!read(&base, ".clerk/data/tasks.csv").contains("Captured"));
    let direct = run(
        &base,
        &[
            "tasks",
            "new",
            "--title=Direct",
            "--types=a|d",
            "--enabled=f",
            "--prefix=A",
        ],
    );
    assert_eq!(direct.0, 0, "{}", direct.2);
    assert!(read(&base, ".clerk/data/tasks.csv").contains("ability|defect"));
    let filtered = run(&base, &["tasks", "list", "--types=a", "--json"]);
    assert_eq!(filtered.0, 0, "{}", filtered.2);
    let query = run(
        &base,
        &["tasks", "list", "--filter", "state = 't'", "--json"],
    );
    assert_eq!(query.0, 3, "{}", query.2);
    assert!(query.2.contains("`t` is not registered"));
    let requested = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Requested",
            "--types",
            "d",
            "--request",
            "enum",
        ],
    );
    assert_eq!(requested.0, 0, "{}", requested.2);
    assert!(read(&base, ".clerk/requests/pending/tasks-enum.toml").contains("defect"));
    write(
        &base,
        ".clerk/config/workflows/enums.toml",
        "version = 1\ntitle = 'Enums'\n[[param]]\nname = 'types'\ntype = 'enum'\nregistry = 'type'\nmulti = true\nrequired = true\n[[step]]\nkind = 'tasks'\nverb = 'new'\nvalues = { title = 'Template', types = '${params.types}' }\n",
    );
    let preview = run(&base, &["workflow", "preview", "enums", "--types", "a|d"]);
    assert_eq!(preview.0, 0, "{}", preview.2);
    assert!(preview.1.contains("ability|defect"), "{}", preview.1);
    let preview = run(
        &base,
        &[
            "batch",
            "--template",
            "enums",
            "--types",
            "a|d",
            "--preview",
        ],
    );
    assert_eq!(preview.0, 0, "{}", preview.2);
    assert!(preview.1.contains("ability|defect"), "{}", preview.1);
    let before = read(&base, ".clerk/data/tasks.csv");
    assert_ne!(
        run(
            &base,
            &["tasks", "new", "--title", "Bad", "--types", "unknown"]
        )
        .0,
        0
    );
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    write(
        &base,
        ".clerk/config/registry/type.csv",
        "key,display\na,A\nability,Ability\nabstract,Abstract\nactive,Active\ndefect,Defect\n",
    );
    assert_eq!(
        run(&base, &["tasks", "new", "--title", "Exact", "--types", "a"]).0,
        0
    );
    let ambiguous = run(
        &base,
        &["tasks", "new", "--title", "Ambiguous", "--types", "ab"],
    );
    assert_eq!(ambiguous.0, 3, "{}", ambiguous.2);
    assert!(ambiguous.2.contains("ambiguous"));
    let ambiguous = run(
        &base,
        &["tasks", "new", "--title", "Ambiguous", "--types", "a|ac"],
    );
    assert_eq!(ambiguous.0, 0, "{}", ambiguous.2);
}

#[test]
fn completion_understands_attached_clustered_and_scoped_enum_values() {
    let base = root("completion-option-syntax");
    for (current, expected) in [
        ("-Sal", "-Sall"),
        ("-rS=ar", "-rS=archived"),
        ("--color=n", "--color=never"),
        ("-op", "-opager"),
    ] {
        let result = run(
            &base,
            &[
                "completion",
                "complete",
                "--words-json",
                r#"["tasks","list"]"#,
                &format!("--current={current}"),
                "--json",
            ],
        );
        assert_eq!(result.0, 0, "{}", result.2);
        assert!(result.1.contains(expected), "{current}: {}", result.1);
    }
    let result = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","show","-S","ar"]"#,
            "--current=",
            "--json",
        ],
    );
    assert_eq!(result.0, 0, "{}", result.2);
    let result = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","show","--"]"#,
            "--current=--",
            "--json",
        ],
    );
    assert_eq!(result.0, 0, "{}", result.2);
    assert!(!result.1.contains("--color"));
    let schema = run(&base, &["explain", "tasks", "--json"]);
    let schema: serde_json::Value = serde_json::from_str(&schema.1).unwrap();
    for option in schema["command_options"].as_array().unwrap() {
        assert_eq!(
            option["cli_prefix"],
            !option["choices"].as_array().unwrap().is_empty()
        );
        if option["name"] == "data-scope" {
            assert_eq!(option["short"], "S");
        }
    }
}

#[test]
fn global_terminal_defaults_do_not_block_direct_or_batch_writes() {
    let base = root("terminal-defaults-writes");
    let config = read(&base, ".clerk/workspace.toml")
        + "\n[terminal]\noverflow='pager'\npaging='always'\ncolor='always'\n";
    write(&base, ".clerk/workspace.toml", &config);
    let direct = run(&base, &["tasks", "new", "--title", "Direct"]);
    assert_eq!(direct.0, 0, "{}", direct.2);
    write(
        &base,
        "flow.toml",
        "version = 1\n[[step]]\nkind = 'tasks'\nverb = 'new'\nvalues = { title = 'Batched' }\n",
    );
    initial_commit(&base);
    let batch = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(batch.0, 0, "{}", batch.2);
    let data = read(&base, ".clerk/data/tasks.csv");
    assert!(data.contains("Direct") && data.contains("Batched"));
    let explicit = run(
        &base,
        &["tasks", "new", "--title", "Rejected", "--overflow", "full"],
    );
    assert_ne!(explicit.0, 0);
    assert!(explicit.2.contains("does not apply"));
    assert!(!read(&base, ".clerk/data/tasks.csv").contains("Rejected"));
}

#[test]
fn document_filters_share_regex_and_invalid_terminal_declarations_fail_before_writes() {
    let base = root("document-match-terminal");
    assert_eq!(
        run(&base, &["contracts", "new", "--title", "Clerk contract"]).0,
        0
    );
    assert_eq!(
        run(&base, &["contracts", "new", "--title", "Other contract"]).0,
        0
    );
    let found = run(
        &base,
        &[
            "contracts",
            "list",
            "--filter",
            "title matches '^clerk'",
            "--json",
        ],
    );
    assert_eq!(found.0, 0, "{}", found.2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&found.1).unwrap()["count"],
        1
    );
    assert!(
        run(&base, &["contracts", "show", "con-01"])
            .1
            .contains("Clerk contract")
    );
    let workspace = read(&base, ".clerk/workspace.toml");
    for section in [
        "[terminal]\ncolor='bad'",
        "[terminal]\nunknown='full'",
        "[terminal]\ncolor=1",
        "[terminal]\npaging='always'\noverflow='full'",
    ] {
        write(
            &base,
            ".clerk/workspace.toml",
            &format!("{workspace}\n{section}\n"),
        );
        assert_ne!(run(&base, &["tasks", "new", "--title", "Rejected"]).0, 0);
        assert!(!read(&base, ".clerk/data/tasks.csv").contains("Rejected"));
    }
    write(&base, ".clerk/workspace.toml", &workspace);
    let declaration = read(&base, ".clerk/config/kinds/tasks.toml");
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &declaration.replace(
            "name = \"state\"",
            "name = \"state\"\nterminal_style = { nonexistent='warning' }",
        ),
    );
    assert_ne!(run(&base, &["tasks", "new", "--title", "Rejected"]).0, 0);
}

#[test]
fn shared_matching_selection_terminal_defaults_and_completion_work_together() {
    let base = root("reading-suite");
    for title in ["Clerk Alpha", "clerk Beta"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    assert!(
        run(&base, &["tasks", "show", "app-001"])
            .1
            .contains("Clerk Alpha")
    );
    assert_ne!(
        run(&base, &["tasks", "show", "app-001", "--case", "sensitive"]).0,
        0
    );
    assert!(
        run(&base, &["tasks", "show", "001", "--match", "contains"])
            .1
            .contains("APP-001")
    );
    let ambiguous = run(&base, &["tasks", "show", "app-00", "--match", "prefix"]);
    assert_ne!(ambiguous.0, 0);
    assert!(ambiguous.2.contains("APP-001") && ambiguous.2.contains("APP-002"));
    assert_ne!(run(&base, &["tasks", "start", "app-001"]).0, 0);
    for (filter, count) in [
        ("title ~ 'CLERK'", 2),
        ("title matches '^clerk'", 2),
        ("title not matches 'Alpha'", 1),
        ("id matches '^APP-001$'", 1),
    ] {
        let result = run(&base, &["tasks", "list", "--filter", filter, "--json"]);
        assert_eq!(result.0, 0, "{}", result.2);
        let value: serde_json::Value = serde_json::from_str(&result.1).unwrap();
        assert_eq!(value["count"], count);
    }
    let result = run(
        &base,
        &[
            "tasks",
            "list",
            "--filter",
            "title matches '^clerk'",
            "--case",
            "sensitive",
            "--json",
        ],
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result.1).unwrap()["count"],
        1
    );
    assert_ne!(
        run(&base, &["tasks", "list", "--filter", "title matches '['"]).0,
        0
    );
    let insensitive = run(
        &base,
        &[
            "tasks",
            "list",
            "--filter",
            "state = 'TODO'",
            "--case",
            "ignore",
            "--json",
        ],
    );
    assert_eq!(insensitive.0, 0, "{}", insensitive.2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&insensitive.1).unwrap()["count"],
        2
    );
    let declaration=read(&base,".clerk/config/kinds/tasks.toml").replace("name = \"state\"", "name = \"state\"\nterminal_style = { todo = 'warning', active = 'info', done = 'success' }");
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    let config = read(&base, ".clerk/workspace.toml")
        + "\n[terminal]\noverflow='pager'\npaging='auto'\ncolor='always'\n";
    write(&base, ".clerk/workspace.toml", &config);
    let styled = run(&base, &["tasks", "list"]);
    assert_eq!(styled.0, 0, "{}", styled.2);
    assert!(styled.1.contains("\x1b["));
    let plain = run(&base, &["tasks", "list", "--color", "never"]);
    assert!(!plain.1.contains("\x1b["));
    let machine = run(
        &base,
        &[
            "tasks", "list", "--json", "--color", "always", "--paging", "always",
        ],
    );
    assert_eq!(machine.0, 0, "{}", machine.2);
    assert!(!machine.1.contains("\x1b["));
    let candidates = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","show"]"#,
            "--current=app-",
            "--json",
        ],
    );
    assert_eq!(candidates.0, 0, "{}", candidates.2);
    assert!(candidates.1.contains("APP-001"));
    assert!(!candidates.1.contains("\x1b["));
    let options = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","list","-o"]"#,
            "--current=",
            "--json",
        ],
    );
    assert!(options.1.contains("pager") && options.1.contains("full"));
    let filter = run(
        &base,
        &[
            "completion",
            "complete",
            "--words-json",
            r#"["tasks","list","--filter"]"#,
            "--current=state = ",
            "--json",
        ],
    );
    assert!(filter.1.contains("todo"), "{}", filter.1);
    let before = read(&base, ".clerk/data/tasks.csv");
    assert_ne!(
        run(
            &base,
            &[
                "tasks", "new", "--title", "Invalid", "-o", "full", "--paging", "always"
            ]
        )
        .0,
        0
    );
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
}

#[test]
fn overflow_policies_keep_redirected_results_and_machine_formats_complete() {
    let base = root("overflow-policies");
    let mut data = String::from("id,title,state,evidence,commit,updated\n");
    for index in 1..=100 {
        data.push_str(&format!(
            "APP-{index:03},Task {index},todo,,,20261102-000000\n"
        ));
    }
    write(&base, ".clerk/data/tasks.csv", &data);
    for flags in [
        vec![],
        vec!["--details"],
        vec!["--long"],
        vec!["--json"],
        vec!["--csv"],
        vec!["--tsv"],
        vec!["--md"],
    ] {
        let mut command = vec!["tasks", "list"];
        command.extend(flags);
        let (code, complete, err) = run(&base, &command);
        assert_eq!(code, 0, "{err}");
        assert!(complete.contains("APP-100"), "{complete}");
        for mode in ["truncate", "more", "pager", "full"] {
            let mut options = command.clone();
            options.extend(["--overflow", mode]);
            assert_eq!(run(&base, &options), (0, complete.clone(), String::new()));
        }
    }
    let help = run(&base, &["help", "tasks", "--overflow", "pager"]);
    assert_eq!(help.0, 0, "{}", help.2);
    assert!(help.1.contains("--overflow"));
    let before = read(&base, ".clerk/data/tasks.csv");
    let invalid = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Invalid",
            "--overflow",
            "unknown",
        ],
    );
    assert_eq!(invalid.0, 2, "{}", invalid.2);
    assert!(invalid.2.contains("full, truncate, pager, more"));
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    let workspace = read(&base, ".clerk/workspace.toml")
        + "\n[commands.\"tasks.list\"]\noverflow = \"pager\"\n";
    write(&base, ".clerk/workspace.toml", &workspace);
    let configured = run(&base, &["tasks", "list"]);
    assert_eq!(configured.0, 0, "{}", configured.2);
    assert!(configured.1.contains("APP-100"));
}

#[test]
fn summary_lists_fit_narrow_windows_and_details_preserve_complete_fields() {
    let base = root("summary-list");
    let declaration = TASK_KIND.replace(
        "columns = [\"id\", \"title\", \"state\", \"updated\"]",
        "layout = \"summary\"\ncolumns = [\"id\", \"title\", \"state\", \"publisher\", \"claimant\", \"updated\"]\nempty = { claimant = \"unclaimed\" }",
    ) + "\n[[field]]\nname = \"publisher\"\nrole = \"status\"\nauto = \"git_name\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    for title in ["完整中文标题和长内容不会被丢弃", "Second"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    assert_eq!(
        run(&base, &["tasks", "brief", "APP-001", "--body", "Details"]).0,
        0
    );
    let raw = run(&base, &["tasks", "list", "--json"]).1;
    let data: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(data["data"][0]["publisher"], "tester");
    assert_eq!(data["data"][0]["claimant"], "");
    assert!(!raw.contains("unclaimed"));
    for width in ["20", "40", "80", "140"] {
        let (code, text, err) = run(&base, &["tasks", "list", "--width", width]);
        assert_eq!(code, 0, "{err}");
        assert!(
            text.lines()
                .all(|line| clerk::terminal::text::display_width(line) <= width.parse().unwrap()),
            "{text}"
        );
        assert!(
            text.lines().nth(2).unwrap().starts_with("APP-001"),
            "{text}"
        );
        assert!(
            text.lines().nth(3).unwrap().starts_with("APP-002"),
            "{text}"
        );
        assert!(!text.contains("attachments:"), "{text}");
        if width == "20" {
            assert!(text.contains("--details"), "{text}");
        }
        let (code, details, err) = run(&base, &["tasks", "list", "--details", "--width", width]);
        assert_eq!(code, 0, "{err}");
        assert!(
            details
                .lines()
                .all(|line| clerk::terminal::text::display_width(line) <= width.parse().unwrap()),
            "{details}"
        );
        let joined = details
            .chars()
            .filter(|c| !c.is_ascii())
            .collect::<String>();
        assert!(
            joined.contains("完整中文标题和长内容不会被丢弃"),
            "{details}"
        );
        assert!(details.contains("claimant_email"), "{details}");
        assert!(!details.contains("attachments:"), "{details}");
    }
    let plain = run(&base, &["tasks", "list", "--plain"]).1;
    assert_eq!(plain.lines().count(), 4, "{plain}");
    assert!(
        plain.contains("unclaimed") && plain.contains("tester"),
        "{plain}"
    );
    let compact = run(&base, &["tasks", "list", "--compact", "--width", "80"]).1;
    assert!(!compact.contains("-------"));
    for mode in ["--long", "--details"] {
        assert_eq!(raw, run(&base, &["tasks", "list", mode, "--json"]).1);
    }
}

#[test]
fn long_uses_a_declared_projection_and_details_matches_show() {
    let base = root("list-modes");
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &(TASK_KIND.to_owned()
            + "\n[[view]]\nname = \"long\"\ncolumns = [\"id\", \"title\", \"claimant\"]\nflex = \"title\"\n"),
    );
    assert_eq!(run(&base, &["tasks", "new", "--title", "First"]).0, 0);
    assert_eq!(run(&base, &["tasks", "claim", "APP-001"]).0, 0);
    let details = run(&base, &["tasks", "list", "--details"]).1;
    let shown = run(&base, &["tasks", "show", "APP-001"]).1;
    assert_eq!(details.trim_end(), shown.trim_end());
    assert!(details.contains("tester@example.com"), "{details}");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Second"]).0, 0);
    let table = run(&base, &["tasks", "list", "--long"]).1;
    assert_eq!(table.lines().count(), 4, "{table}");
    assert!(
        table.lines().next().unwrap().contains("claimant"),
        "{table}"
    );
    assert!(table.lines().nth(2).unwrap().contains("tester"), "{table}");
    for mode in ["--long", "--details"] {
        let custom = run(&base, &["tasks", "list", mode, "--columns", "id,title"]).1;
        assert!(!custom.contains("claimant_email"), "{custom}");
    }
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, _, err) = run(&base, &["tasks", "list", "--long", "--details"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("mutually exclusive"), "{err}");
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
}

#[test]
fn adaptive_list_views_work_for_other_kinds_and_explicit_columns() {
    let base = root("adaptive-other-kind");
    let declaration = r#"title = "Cards"
table = ".clerk/data/cards.csv"
key = ["code"]
[[field]]
name = "code"
[[field]]
name = "caption"
[[field]]
name = "phase"
[[field]]
name = "team"
[[field]]
name = "memo"
[[view]]
name = "list"
columns = ["code", "caption", "phase"]
flex = "caption"
priority = ["code", "caption", "phase"]
sort = ["code"]
[[view]]
name = "long"
columns = ["code", "caption", "phase", "team"]
flex = "caption"
priority = ["code", "caption", "team", "phase"]
sort = ["caption"]
"#;
    write(&base, ".clerk/config/kinds/cards.toml", declaration);
    let memo = "A long acceptance note that belongs in vertical details only";
    for (code, caption) in [
        ("card-a", "Zulu中文标题和更多说明"),
        ("card-b", "Alpha中文标题和更多说明"),
    ] {
        let (exit, out, err) = run(
            &base,
            &[
                "cards",
                "new",
                "--code",
                code,
                "--caption",
                caption,
                "--phase",
                "active",
                "--team",
                "operators",
                "--memo",
                memo,
            ],
        );
        assert_eq!(exit, 0, "{out} {err}");
    }
    for width in ["20", "40", "60", "100", "180"] {
        for mode in [
            vec![],
            vec!["--long"],
            vec!["--long", "--compact"],
            vec!["--columns", "code,memo,caption,team"],
            vec!["--columns", "caption"],
            vec!["--long", "--headers", "hide"],
        ] {
            let mut args = vec!["cards", "list", "--width", width];
            args.extend(mode);
            let (code, out, err) = run(&base, &args);
            assert_eq!(code, 0, "{args:?}: {err}");
            assert!(
                out.lines().all(
                    |line| clerk::terminal::text::display_width(line) <= width.parse().unwrap()
                ),
                "{args:?}: {out}"
            );
        }
    }
    let long = run(&base, &["cards", "list", "--long", "--width", "180"]).1;
    assert_eq!(long.lines().count(), 4, "{long}");
    assert!(long.lines().nth(2).unwrap().starts_with("card-b"), "{long}");
    assert!(!long.contains("memo"), "{long}");
    let default = run(&base, &["cards", "list", "--width", "180"]).1;
    assert!(
        default.lines().nth(2).unwrap().starts_with("card-a"),
        "{default}"
    );
    let details = run(&base, &["cards", "list", "--details"]).1;
    assert!(details.contains(memo), "{details}");
    for format in ["--json", "--csv", "--tsv", "--md"] {
        assert_eq!(
            run(&base, &["cards", "list", format]).1,
            run(&base, &["cards", "list", "--long", format]).1
        );
    }
    let schema: serde_json::Value =
        serde_json::from_str(&run(&base, &["explain", "cards", "--json"]).1).unwrap();
    assert_eq!(schema["declaration"]["view"][1]["flex"], "caption");
    assert_eq!(schema["declaration"]["view"][1]["priority"][2], "team");
    // A generated page named long is not a terminal projection.
    let prefix = declaration
        .split("[[view]]\nname = \"long\"")
        .next()
        .unwrap();
    write(
        &base,
        ".clerk/config/kinds/cards.toml",
        &(prefix.to_owned()
            + "[[view]]\nname = \"long\"\nrenderer = \"lines\"\ntarget = \"status.md\"\nregion = \"cards\"\ncolumns = [\"memo\"]\ntemplate = \"{memo}\"\n"),
    );
    let (code, fallback, err) = run(&base, &["cards", "list", "--long", "--width", "180"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(fallback, default);
}

#[test]
fn terminal_view_layout_declarations_are_strict() {
    for (index, extra) in [
        "flex = 7",
        "flex = \"missing\"",
        "priority = 7",
        "priority = [\"id\", \"id\"]",
        "priority = [\"unknown\"]",
        "priority = [\"id\"]\ntarget = \"status.md\"",
    ]
    .iter()
    .enumerate()
    {
        let base = root(&format!("adaptive-invalid-{index}"));
        write(
            &base,
            ".clerk/config/kinds/tasks.toml",
            &TASK_KIND.replace("name = \"list\"", &format!("name = \"list\"\n{extra}")),
        );
        let (code, _, err) = run(&base, &["tasks", "list"]);
        assert_eq!(code, 2, "{extra}: {err}");
    }
}

#[test]
fn table_multi_values_are_compact_without_changing_text_or_machine_values() {
    let base = root("table-multi-spacing");
    let declaration = TASK_KIND.replace(
        "columns = [\"id\", \"title\", \"state\", \"updated\"]",
        "columns = [\"id\", \"title\", \"types\", \"labels\"]",
    ) + "\n[[field]]\nname = \"types\"\ntype = \"enum\"\nregistry = \"type\"\nmulti = true\n\n[[field]]\nname = \"labels\"\ntype = \"text\"\nmulti = true\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "comma, space",
                "--types",
                "ability|defect",
                "--labels",
                "a|b"
            ]
        )
        .0,
        0
    );
    for mode in [vec![], vec!["--long"], vec!["--compact"]] {
        let mut args = vec!["tasks", "list"];
        args.extend(mode);
        let (code, text, err) = run(&base, &args);
        assert_eq!(code, 0, "{err}");
        assert!(
            text.contains("ability,defect") && text.contains("a,b"),
            "{text}"
        );
        assert!(text.contains("comma, space"), "{text}");
    }
    let display = run(&base, &["tasks", "list", "--display"]).1;
    assert!(
        display.contains("能力,缺陷") && display.contains("a,b"),
        "{display}"
    );
    let raw: serde_json::Value =
        serde_json::from_str(&run(&base, &["tasks", "list", "--json"]).1).unwrap();
    assert_eq!(
        raw["data"][0]["types"],
        serde_json::json!(["ability", "defect"])
    );
    assert_eq!(raw["data"][0]["labels"], "a|b");
    let csv = run(&base, &["tasks", "list", "--csv"]).1;
    assert!(
        csv.contains("ability|defect") && csv.contains("a|b"),
        "{csv}"
    );
}

#[test]
fn scanning_rows_hide_historical_notes_and_keep_controls_on_one_line() {
    let base = root("summary-scanning");
    let declaration = TASK_KIND.replace(
        "columns = [\"id\", \"title\", \"state\", \"updated\"]",
        "layout = \"summary\"\ncolumns = [\"id\", \"title\", \"state\", \"updated\"]",
    ) + "\n[[field]]\nname = \"note\"\ntype = \"text\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    let title = "first line\nsecond\tpart\u{1b}[31m";
    let note = "Historic verification note repeated across older tasks";
    assert_eq!(
        run(&base, &["tasks", "new", "--title", title, "--note", note]).0,
        0
    );
    let (code, out, err) = run(&base, &["tasks", "list", "--plain"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(!out.contains(note) && !out.contains('\u{1b}'), "{out}");
    assert!(out.contains("first line second part"), "{out}");
    let details = run(&base, &["tasks", "list", "--details", "--width", "140"]).1;
    assert!(details.contains(note), "{details}");
    let raw: serde_json::Value =
        serde_json::from_str(&run(&base, &["tasks", "list", "--json"]).1).unwrap();
    assert_eq!(raw["data"][0]["title"], title);
}

#[test]
fn summary_declarations_reject_invalid_fields_and_options() {
    for (index, options) in [
        "layout = \"bogus\"",
        "layout = \"summary\"\nsecondary = [\"missing\"]",
        "layout = \"summary\"\npresence = [\"title\"]",
        "layout = \"summary\"\nrelations = [\"missing\"]",
        "layout = \"summary\"\nempty = { claimant = 4 }",
        "layout = \"summary\"\nsecondary = 7",
        "layout = \"summary\"\ntarget = \"status.md\"",
    ]
    .iter()
    .enumerate()
    {
        let base = root(&format!("summary-invalid-{index}"));
        write(
            &base,
            ".clerk/config/kinds/tasks.toml",
            &TASK_KIND.replace("name = \"list\"", &format!("name = \"list\"\n{options}")),
        );
        assert_eq!(run(&base, &["tasks", "list"]).0, 2, "{options}");
    }
}

#[test]
fn immutable_authorship_survives_edits_transitions_and_rejects_actions() {
    let base = root("immutable-authorship");
    let declaration = format!(
        "{TASK_KIND}\n[[field]]\nname = \"publisher\"\nrole = \"status\"\nauto = \"git_name\"\nimmutable = true\n\n[[field]]\nname = \"created\"\nrole = \"status\"\nauto = \"now\"\nimmutable = true\n\n[[action]]\nname = \"rewrite\"\nhandler = \"set\"\nvalues = {{ publisher = \"changed\" }}\n"
    );
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    assert_eq!(run(&base, &["tasks", "new", "--title", "Original"]).0, 0);
    // Seed a historic timestamp to distinguish preservation from refreshing within one second.
    let mut table =
        clerk::storage::store::Table::from_text(&read(&base, ".clerk/data/tasks.csv")).unwrap();
    let mut record = table.record(0);
    record.insert("created".into(), "20000101-010101".into());
    table.apply(0, &record);
    table.save_to(&base.join(".clerk/data/tasks.csv")).unwrap();
    assert_eq!(
        run(&base, &["seal", "--note", "Seed historical fixture"]).0,
        0
    );
    git(&base, &["config", "user.name", "another"]);
    for args in [
        vec!["tasks", "set", "APP-001", "--title", "Updated"],
        vec!["tasks", "claim", "APP-001"],
        vec!["tasks", "start", "APP-001"],
        vec!["tasks", "brief", "APP-001", "--body", "Body"],
    ] {
        let (code, out, err) = run(&base, &args);
        assert_eq!(code, 0, "{out} {err}");
        let record = clerk::storage::store::Table::from_text(&read(&base, ".clerk/data/tasks.csv"))
            .unwrap()
            .record(0);
        assert_eq!(record["publisher"], "tester");
        assert_eq!(record["created"], "20000101-010101");
    }
    let before = read(&base, ".clerk/data/tasks.csv");
    for args in [
        vec!["tasks", "rewrite", "APP-001"],
        vec!["tasks", "rewrite", "APP-001", "--dry-run"],
    ] {
        let (code, _, err) = run(&base, &args);
        assert_eq!(code, 3, "{err}");
        assert!(err.contains("immutable"), "{err}");
        assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    }
    let schema = run(&base, &["explain", "tasks", "--json"]).1;
    assert!(schema.contains("\"immutable\":true"), "{schema}");
}

#[test]
fn operator_migrates_to_git_metadata_and_conflicts_preserve_both_files() {
    let base = root("operator-migration");
    write(&base, ".work/identity", "legacy\n");
    let (code, out, err) = run(&base, &["whoami"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("legacy"), "{out}");
    assert_eq!(read(&base, ".git/clerk/identity"), "legacy\n");
    assert!(!base.join(".work/identity").exists());
    assert_eq!(run(&base, &["id", "current"]).0, 0);
    assert_eq!(read(&base, ".git/clerk/identity"), "current\n");
    assert_eq!(git(&base, &["config", "user.name"]).trim(), "tester");
    write(&base, ".work/identity", "different\n");
    let (code, _, err) = run(&base, &["whoami"]);
    assert_eq!(code, 4, "{err}");
    assert_eq!(read(&base, ".work/identity"), "different\n");
    assert_eq!(read(&base, ".git/clerk/identity"), "current\n");
}

#[test]
fn operator_isolated_by_git_worktree_and_custom_legacy_path_migrates() {
    let base = root("operator-worktree");
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[layout]\nidentity = \".work/custom-operator\"\n"),
    );
    write(&base, ".work/custom-operator", "legacy-custom\n");
    assert_eq!(run(&base, &["whoami"]).0, 0);
    assert_eq!(read(&base, ".git/clerk/identity"), "legacy-custom\n");
    initial_commit(&base);
    let checkout = base.join(".work/checkout");
    git(
        &base,
        &[
            "worktree",
            "add",
            "-b",
            "operator-check",
            checkout.to_str().unwrap(),
        ],
    );
    assert_eq!(run(&checkout, &["id", "worktree-only"]).0, 0);
    assert!(run(&checkout, &["whoami"]).1.contains("worktree-only"));
    assert_eq!(read(&base, ".git/clerk/identity"), "legacy-custom\n");
    assert!(!checkout.join(".git/clerk").exists());
    git(
        &base,
        &["worktree", "remove", "--force", checkout.to_str().unwrap()],
    );
}

#[test]
fn immutable_document_fields_and_batch_failures_preserve_original_data() {
    let base = root("immutable-document-batch");
    let declaration =
        CONTRACT_KIND.replace("name = \"title\"", "name = \"title\"\nimmutable = true");
    write(&base, ".clerk/config/kinds/contracts.toml", &declaration);
    assert_eq!(
        run(&base, &["contracts", "new", "--title", "Original"]).0,
        0
    );
    assert_eq!(run(&base, &["contracts", "start", "CON-01"]).0, 0);
    let original = read(&base, "contracts/CON-01.md");
    let (code, _, err) = run(&base, &["contracts", "set", "CON-01", "--title", "Changed"]);
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("immutable"), "{err}");
    assert_eq!(read(&base, "contracts/CON-01.md"), original);
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "contracts",
                "set",
                "CON-01",
                "--title",
                "Changed",
                "--batch",
                "invalid"
            ]
        )
        .0,
        0
    );
    let (code, _, err) = run(&base, &["batch", "apply", "invalid"]);
    assert_eq!(code, 3, "{err}");
    assert_eq!(read(&base, "contracts/CON-01.md"), original);
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn local_metadata_migrates_once_and_preserves_drafts_and_history() {
    let base = root("local-metadata-upgrade");
    enable_auto_commit(&base);
    assert_eq!(run(&base, &["tasks", "new", "--title", "First"]).0, 0);
    let original = run(&base, &["operation", "status", "--json"]).1;
    assert_eq!(run(&base, &["render", "--batch", "pending"]).0, 0);
    let before = read(&base, ".git/clerk/batches/pending.toml");
    for (new, old) in [
        ("operations", "clerk-operations"),
        ("batches", "clerk-batches"),
        ("write.lock", "clerk-write.lock"),
    ] {
        std::fs::rename(
            base.join(".git/clerk").join(new),
            base.join(".git").join(old),
        )
        .unwrap();
    }
    write(
        &base,
        ".git/clerk-sessions/sessions.toml",
        "version = 1\nsession = []\n",
    );
    write(&base, ".git/clerk-sync.stamp", "0");
    let (code, stdout, stderr) = run(&base, &["operation", "status", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(stdout, original);
    assert_eq!(read(&base, ".git/clerk/batches/pending.toml"), before);
    assert_eq!(
        read(&base, ".git/clerk/sessions/sessions.toml"),
        "version = 1\nsession = []\n"
    );
    assert_eq!(read(&base, ".git/clerk/sync.stamp"), "0");
    for old in [
        "clerk-operations",
        "clerk-batches",
        "clerk-write.lock",
        "clerk-sessions",
        "clerk-sync.stamp",
    ] {
        assert!(!base.join(".git").join(old).exists(), "{old}");
    }
    assert_eq!(run(&base, &["batch", "show", "pending"]).0, 0);
    assert_eq!(run(&base, &["session", "list"]).0, 0);
    assert!(!base.join(".clerk/operations").exists());
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn local_metadata_upgrade_respects_old_locks_and_preserves_collisions() {
    let base = root("local-metadata-locked");
    enable_auto_commit(&base);
    write(&base, ".git/clerk-batches/keep.toml", "keep me");
    let lock = std::fs::File::create(base.join(".git/clerk-write.lock")).unwrap();
    lock.try_lock().unwrap();
    let (code, _, stderr) = run(&base, &["batch", "list"]);
    assert_eq!(code, 4, "{stderr}");
    assert_eq!(read(&base, ".git/clerk-batches/keep.toml"), "keep me");
    drop(lock);
    write(&base, ".git/clerk/batches/keep.toml", "different content");
    let (code, _, stderr) = run(&base, &["batch", "list"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("collision"), "{stderr}");
    assert_eq!(read(&base, ".git/clerk-batches/keep.toml"), "keep me");
    assert_eq!(
        read(&base, ".git/clerk/batches/keep.toml"),
        "different content"
    );
}

#[test]
fn historical_shared_receipts_import_locally_and_keep_replay_protection() {
    let base = root("local-receipt-import");
    enable_auto_commit(&base);
    assert_eq!(
        run(
            &base,
            &["tasks", "new", "--title", "Only once", "--batch", "old"]
        )
        .0,
        0
    );
    let draft = read(&base, ".git/clerk/batches/old.toml");
    assert_eq!(run(&base, &["batch", "apply", "old"]).0, 0);
    let report: serde_json::Value =
        serde_json::from_str(&run(&base, &["operation", "status", "--json"]).1).unwrap();
    let id = report["id"].as_str().unwrap();
    let local = format!(".git/clerk/operations/{id}.toml");
    let mut table: toml::Table = read(&base, &local).parse().unwrap();
    let batch = table.remove("batch").unwrap();
    table.insert("data".into(), toml::Value::String("pending".into()));
    write(&base, &local, &table.to_string());
    write(
        &base,
        &format!(".clerk/operations/{id}.toml"),
        &batch.as_table().unwrap().to_string(),
    );
    write(&base, ".git/clerk/batches/old.toml", &draft);
    std::fs::rename(
        base.join(".git/clerk/operations"),
        base.join(".git/clerk-operations"),
    )
    .unwrap();
    let (code, _, stderr) = run(&base, &["batch", "apply", "old"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("already applied"), "{stderr}");
    let upgraded: toml::Table = read(&base, &local).parse().unwrap();
    assert_eq!(upgraded["batch"], batch);
    assert_eq!(upgraded["data"].as_str(), Some("saved"));
    for field in ["message", "commit", "push", "head", "files"] {
        assert_eq!(upgraded[field], table[field]);
    }
    let mut foreign = batch.as_table().unwrap().clone();
    foreign.insert(
        "id".into(),
        toml::Value::String("op-zzzzzzzzzzzzzzzz-1".into()),
    );
    write(
        &base,
        ".clerk/operations/op-zzzzzzzzzzzzzzzz-1.toml",
        &foreign.to_string(),
    );
    let (code, stdout, stderr) = run(&base, &["operation", "status", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap()["id"],
        id
    );
    std::fs::remove_file(base.join(format!(".clerk/operations/{id}.toml"))).unwrap();
    assert_eq!(run(&base, &["batch", "apply", "old"]).0, 4);
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 2);
}

#[test]
fn accepted_receipt_writer_child() {
    let Some(base) = std::env::var_os("CLERK_TEST_ACCEPT_ROOT") else {
        return;
    };
    let base = PathBuf::from(base);
    let mut context = clerk::execution::context::Context::open(base.to_str()).unwrap();
    let _guard = clerk::execution::pipeline::begin(&mut context).unwrap();
    let kind = context.catalog.require("tasks").unwrap().clone();
    let values = [("title".into(), "Accepted once".into())]
        .into_iter()
        .collect();
    let outcome = clerk::execution::application::execute(
        &mut context,
        &kind,
        "new",
        &[],
        &values,
        &Default::default(),
    )
    .unwrap();
    clerk::execution::pipeline::check_paths(&mut context, &outcome.affected, &[]).unwrap();
    let mut affected = outcome.affected;
    affected.push(context.workspace.layout.seal.clone());
    let draft: toml::Table = read(&base, ".git/clerk/batches/once.toml").parse().unwrap();
    let batch: toml::Table = [(
        "draft".into(),
        toml::Value::Table(
            [
                ("id".into(), draft["id"].clone()),
                ("name".into(), toml::Value::String("once".into())),
            ]
            .into_iter()
            .collect(),
        ),
    )]
    .into_iter()
    .collect();
    clerk::git::operation::change(|report| report.batch = Some(batch));
    clerk::git::commit_message::resolve(&context.workspace, &affected, "workflow").unwrap();
    clerk::git::operation::prepare(&context.workspace, &affected, "workflow").unwrap();
    let id = clerk::git::operation::current().unwrap().id;
    let mut report: toml::Table = read(&base, &format!(".git/clerk/operations/{id}.toml"))
        .parse()
        .unwrap();
    report.insert("data".into(), toml::Value::String("saved".into()));
    let staged: toml::Table = [
        ("name".into(), toml::Value::String(format!("{id}.toml"))),
        ("body".into(), toml::Value::String(report.to_string())),
    ]
    .into_iter()
    .collect();
    write(&base, ".git/clerk/undo/receipt.toml", &staged.to_string());
    if std::env::var("CLERK_TEST_ACCEPT_PHASE").unwrap() == "accepted" {
        write(&base, ".git/clerk/undo/accepted", "accepted\n");
    }
    write(&base, ".git/accepted-ready", &id);
    loop {
        std::thread::park();
    }
}

#[test]
fn local_receipt_acceptance_recovers_before_retry_or_replay() {
    for phase in ["staged", "accepted"] {
        let base = root(&format!("local-receipt-crash-{phase}"));
        enable_auto_commit(&base);
        assert_eq!(
            run(
                &base,
                &["tasks", "new", "--title", "Only once", "--batch", "once"]
            )
            .0,
            0
        );
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "accepted_receipt_writer_child", "--nocapture"])
            .env("CLERK_TEST_ACCEPT_ROOT", &base)
            .env("CLERK_TEST_ACCEPT_PHASE", phase)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !base.join(".git/accepted-ready").exists() && std::time::Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("child exited: {status}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready = base.join(".git/accepted-ready").exists();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(ready);
        let id = read(&base, ".git/accepted-ready");
        let (code, stdout, stderr) = run(&base, &["batch", "apply", "once"]);
        if phase == "accepted" {
            assert_eq!(code, 4, "{stdout} {stderr}");
            assert!(stderr.contains("already applied"), "{stderr}");
            let (code, stdout, stderr) = run(&base, &["operation", "retry", &id]);
            assert_eq!(code, 0, "{stdout} {stderr}");
            assert!(read(&base, ".clerk/data/tasks.csv").contains("Accepted once"));
        } else {
            assert_eq!(code, 0, "{stdout} {stderr}");
            assert!(read(&base, ".clerk/data/tasks.csv").contains("Only once"));
        }
        assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 2);
        assert_eq!(git(&base, &["rev-list", "--count", "HEAD"]).trim(), "2");
        assert!(git(&base, &["status", "--porcelain"]).is_empty());
        assert!(!base.join(".git/clerk/undo").exists());
        assert!(!base.join(".clerk/operations").exists());
    }
}

#[test]
fn local_metadata_uses_each_worktree_git_directory() {
    let base = root("local-worktree-state");
    enable_auto_commit(&base);
    let checkout = base.parent().unwrap().join("local-worktree-state-other");
    git(
        &base,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            checkout.to_str().unwrap(),
        ],
    );
    assert_eq!(run(&base, &["render", "--batch", "main"]).0, 0);
    let (code, stdout, stderr) = run(&checkout, &["render", "--batch", "other"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    let git_dir = git(&checkout, &["rev-parse", "--absolute-git-dir"]);
    assert!(
        Path::new(git_dir.trim())
            .join("clerk/batches/other.toml")
            .exists()
    );
    assert!(!base.join(".git/clerk/batches/other.toml").exists());
    assert!(base.join(".git/clerk/batches/main.toml").exists());
    git(
        &base,
        &["worktree", "remove", "--force", checkout.to_str().unwrap()],
    );
}

#[test]
fn local_metadata_uses_submodule_git_directory() {
    let source = root("local-module-source");
    enable_auto_commit(&source);
    let parent = root("local-module-parent");
    initial_commit(&parent);
    git(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            source.to_str().unwrap(),
            "module",
        ],
    );
    let module = parent.join("module");
    git(&module, &["config", "user.name", "tester"]);
    git(&module, &["config", "user.email", "tester@example.com"]);
    let (code, stdout, stderr) = run(&module, &["tasks", "new", "--title", "Submodule task"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    let git_dir = git(&module, &["rev-parse", "--absolute-git-dir"]);
    assert!(Path::new(git_dir.trim()).join("clerk/operations").is_dir());
    assert!(module.join(".git").is_file());
    assert!(!parent.join(".git/clerk/operations").exists());
    assert!(!module.join(".clerk/operations").exists());
}

#[test]
fn collected_batch_freezes_bodies_rehearses_and_commits_once() {
    let base = root("collected-batch-lifecycle");
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    let head = git(&base, &["rev-parse", "HEAD"]);
    for args in [
        vec![
            "tasks",
            "new",
            "--title",
            "预约校验",
            "--batch",
            "reservation",
            "--step-name",
            "task",
            "--json",
        ],
        vec!["focus", "add", "@task", "--batch", "reservation"],
        vec!["render", "--batch", "reservation"],
    ] {
        let (code, stdout, stderr) = run(&base, &args);
        assert_eq!(code, 0, "{stdout} {stderr}");
        assert!(
            stdout.contains("not executed") || stdout.contains("\"executed\":false"),
            "{stdout}"
        );
    }
    let body = base.join(".git/source-body.md");
    std::fs::write(&body, "# Original\n\nFrozen content\n").unwrap();
    let (code, stdout, stderr) = run(
        &base,
        &[
            "tasks",
            "brief",
            "@task",
            "--file",
            body.to_str().unwrap(),
            "--batch",
            "reservation",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    std::fs::write(&body, "Changed after capture").unwrap();
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    assert_eq!(git(&base, &["rev-parse", "HEAD"]), head);
    let (code, stdout, stderr) = run(&base, &["batch", "show", "reservation", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        shown["plan"]["step"][3]["body"],
        "# Original\n\nFrozen content\n"
    );
    assert_eq!(
        run(&base, &["batch", "apply", "reservation", "--preview"]).0,
        0
    );
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "apply",
            "reservation",
            "--dry-run",
            "--message",
            "Rehearse",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["results"][0]["keys"][0], "APP-001");
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    assert_eq!(git(&base, &["rev-parse", "HEAD"]), head);
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "apply",
            "reservation",
            "--message",
            "Create reservation work",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(
        read(&base, "tasks/APP-001.md"),
        "# Original\n\nFrozen content\n"
    );
    assert_eq!(
        read(&base, ".clerk/config/registry/focus.csv"),
        "key\nAPP-001\n"
    );
    assert_eq!(git(&base, &["rev-list", "--count", "HEAD"]).trim(), "2");
    assert!(!base.join(".git/clerk/batches/reservation.toml").exists());
    assert_ne!(
        run(
            &base,
            &[
                "batch",
                "apply",
                "reservation",
                "--message",
                "Must not replay"
            ]
        )
        .0,
        0
    );
    assert_eq!(run(&base, &["batch", "list", "--json"]).0, 0);
    assert_eq!(run(&base, &["render", "--batch", "discard"]).0, 0);
    assert_eq!(run(&base, &["batch", "drop", "discard"]).0, 0);
    assert!(!base.join(".git/clerk/batches/discard.toml").exists());
}

#[test]
fn collected_batch_tracks_target_changes_without_rejecting_unrelated_tasks() {
    let base = root("collected-batch-stale");
    enable_auto_commit(&base);
    for title in ["Target", "Other"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    assert_eq!(
        run(
            &base,
            &[
                "tasks", "set", "APP-001", "--title", "Queued", "--batch", "safe"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(&base, &["tasks", "set", "APP-002", "--title", "Unrelated"]).0,
        0
    );
    let (code, stdout, stderr) = run(&base, &["batch", "apply", "safe"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "set",
                "APP-001",
                "--title",
                "Old intent",
                "--batch",
                "stale"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &["tasks", "set", "APP-001", "--title", "Concurrent edit"]
        )
        .0,
        0
    );
    let (code, _, stderr) = run(&base, &["batch", "apply", "stale"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("changed since capture"), "{stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("Concurrent edit"));
    // Appending must not silently refresh an earlier stale target snapshot.
    assert_eq!(run(&base, &["render", "--batch", "stale"]).0, 4);
}

#[test]
fn collected_batch_failure_rolls_back_identifiers_and_keeps_draft_pending() {
    let base = root("collected-batch-rollback");
    enable_auto_commit(&base);
    for args in [
        vec!["tasks", "new", "--title", "Rollback", "--batch", "fail"],
        vec!["tasks", "start", "@step1", "--batch", "fail"],
        vec!["tasks", "start", "@step1", "--batch", "fail"],
    ] {
        let (code, stdout, stderr) = run(&base, &args);
        assert_eq!(code, 0, "{stdout} {stderr}");
    }
    for _ in 0..2 {
        let (code, _, stderr) = run(&base, &["batch", "apply", "fail"]);
        assert_eq!(code, 4, "{stderr}");
        assert!(stderr.contains("batch step 3"), "{stderr}");
        assert!(!stderr.contains("already applied"));
        assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
        assert!(git(&base, &["status", "--porcelain"]).is_empty());
    }
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "After rollback"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("APP-001"), "{stdout}");
}

#[test]
fn collected_batch_git_failure_is_consumed_and_retry_only_finalizes_git() {
    let base = root("collected-batch-git-retry");
    enable_auto_commit(&base);
    assert_eq!(
        run(
            &base,
            &["tasks", "new", "--title", "Only once", "--batch", "one"]
        )
        .0,
        0
    );
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
    let (code, stdout, stderr) = run(&base, &["batch", "apply", "one", "--json"]);
    assert_ne!(code, 0, "{stdout} {stderr}");
    assert!(base.join(".clerk/data/tasks.csv").exists());
    let (code, _, stderr) = run(&base, &["batch", "apply", "one"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("already applied"), "{stderr}");
    std::fs::remove_file(base.join(".git/hooks/pre-commit")).unwrap();
    let (code, stdout, stderr) = run(&base, &["operation", "retry"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(git(&base, &["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 2);
}

#[test]
fn batch_json_and_preview_use_the_strict_shared_plan() {
    let base = root("batch-json-preview");
    write(
        &base,
        "flow.json",
        r#"{"version":1,"step":[{"kind":"tasks","verb":"new","as":"created","values":{"title":"中文标题"}},{"kind":"tasks","verb":"start","keys":["@created"]}]}"#,
    );
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    let file = base.join("flow.json");
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "--file",
            file.to_str().unwrap(),
            "--input-format",
            "json",
            "--preview",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let plan: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(plan["step"][0]["values"]["title"], "中文标题");
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "--file",
            file.to_str().unwrap(),
            "--input-format",
            "json",
            "--message",
            "Apply JSON",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 2);
    for invalid in [
        r#"{"version":1,"step":[{"kind":"tasks","verb":"new","values":{"title":3}}]}"#,
        r#"{"version":1,"step":[{"kind":"tasks","verb":"new","extra":"x"}]}"#,
        r#"{"version":1,"step":[{"verb":"render","force":"false"}]}"#,
    ] {
        write(&base, "invalid.json", invalid);
        assert_eq!(
            run(
                &base,
                &[
                    "batch",
                    "--file",
                    base.join("invalid.json").to_str().unwrap(),
                    "--input-format",
                    "json",
                    "--preview"
                ]
            )
            .0,
            2
        );
    }
}

#[test]
fn batch_removes_old_commands_and_rejects_invalid_capture_without_mutation() {
    let base = root("batch-no-legacy");
    enable_auto_commit(&base);
    for args in [
        vec!["run", "--file", "missing"],
        vec!["batch", "set", "APP-001"],
        vec!["batch", "add", "APP-001"],
        vec!["tasks", "list", "--batch", "bad"],
        vec!["tasks", "new", "--title", "X", "--step-name", "name"],
        vec!["tasks", "new", "--title", "X", "--batch", "../bad"],
        vec!["tasks", "new", "--id", "APP-999", "--batch", "bad"],
        vec!["tasks", "start", "--batch", "bad"],
        vec!["focus", "show", "--batch", "bad"],
        vec!["render", "--batch", "bad", "--sync", "ff-only"],
    ] {
        let (code, stdout, stderr) = run(&base, &args);
        assert_eq!(code, 2, "{args:?} {stdout} {stderr}");
    }
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    assert!(!base.join(".git/clerk/batches/bad.toml").exists());
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn batch_auto_commit_is_bounded_deduplicated_and_omits_prose() {
    let base = root("batch-summary-bounded");
    let mut plan = String::from("version = 1\n");
    for n in 0..24 {
        plan.push_str(&format!("[[step]]\nkind = \"tasks\"\nverb = \"new\"\nas = \"t{n}\"\nvalues = {{ title = \"很长的中文说明不应进入提交\" }}\n[[step]]\nkind = \"tasks\"\nverb = \"start\"\nkeys = [\"@t{n}\"]\n"));
    }
    write(&base, "flow.toml", &plan);
    enable_auto_commit(&base);
    let (code, stdout, stderr) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let message = git(&base, &["log", "-1", "--format=%B"]);
    assert!(
        message.contains("Added tasks (24):") && message.contains("+19 more"),
        "{message}"
    );
    assert!(
        !message.contains("很长")
            && !message.contains("\\u{")
            && !message.contains("Updated tasks")
            && !message.contains("title="),
        "{message}"
    );
    assert!(message.len() < 1200, "{message}");
}

#[test]
fn batch_apply_recovers_a_journal_but_preview_keeps_it_untouched() {
    let base = root("batch-journal-recovery");
    write(&base, "original.md", "original body");
    enable_auto_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "Accepted batch",
                "--batch",
                "recover"
            ]
        )
        .0,
        0
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "interrupted_writer_child", "--nocapture"])
        .env("CLERK_TEST_CRASH_ROOT", &base)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !base.join(".git/ready").exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("child exited: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = base.join(".git/ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready);
    assert_eq!(run(&base, &["batch", "apply", "recover", "--preview"]).0, 4);
    assert!(base.join("interrupted.md").exists());
    let (code, stdout, stderr) = run(&base, &["batch", "apply", "recover"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("APP-001"), "{stdout}");
    assert_eq!(read(&base, "original.md"), "original body");
    assert!(!base.join("interrupted.md").exists());
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn batch_drafts_reject_changed_declarations_and_operator_identity() {
    for change in ["declaration", "identity"] {
        let base = root(&format!("batch-stale-{change}"));
        enable_auto_commit(&base);
        assert_eq!(
            run(
                &base,
                &["tasks", "new", "--title", "Captured", "--batch", "guard"]
            )
            .0,
            0
        );
        if change == "declaration" {
            let path = ".clerk/config/kinds/tasks.toml";
            write(
                &base,
                path,
                &format!("{}\n# Declaration changed\n", read(&base, path)),
            );
            initial_commit(&base);
        } else {
            git(&base, &["config", "user.name", "Another operator"]);
        }
        let (code, _, stderr) = run(&base, &["batch", "apply", "guard"]);
        assert_eq!(code, 4, "{stderr}");
        assert!(
            stderr.contains("declarations changed") || stderr.contains("another operator"),
            "{stderr}"
        );
        assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
        assert!(base.join(".git/clerk/batches/guard.toml").exists());
    }
}

#[test]
fn batch_focus_add_and_log_append_preserve_concurrent_contributions() {
    let base = root("batch-merge-append");
    enable_auto_commit(&base);
    for title in ["First", "Second"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    assert_eq!(
        run(&base, &["focus", "add", "APP-001", "--batch", "merge"]).0,
        0
    );
    assert_eq!(
        run(
            &base,
            &[
                "logs",
                "add",
                "--title",
                "Queued entry",
                "--body",
                "Queued body",
                "--batch",
                "merge"
            ]
        )
        .0,
        0
    );
    assert_eq!(run(&base, &["focus", "add", "APP-002"]).0, 0);
    assert_eq!(
        run(
            &base,
            &[
                "logs",
                "add",
                "--title",
                "Concurrent entry",
                "--body",
                "Concurrent body"
            ]
        )
        .0,
        0
    );
    let (code, stdout, stderr) = run(&base, &["batch", "apply", "merge"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(
        read(&base, ".clerk/config/registry/focus.csv"),
        "key\nAPP-002\nAPP-001\n"
    );
    let stdout = std::fs::read_dir(base.join("log/tester"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        stdout.contains("Queued body") && stdout.contains("Concurrent body"),
        "{stdout}"
    );
}

fn reading_fixture(name: &str) -> PathBuf {
    let base = root(name);
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &format!("{TASK_KIND}\n[context]\nattachments = [\"brief\"]\nlogs = \"logs\"\n"),
    );
    write(
        &base,
        ".clerk/config/kinds/logs.toml",
        &LOGS_KIND.replace(
            "handler = \"append\"",
            "handler = \"append\"\nentry_target = \"tasks\"",
        ),
    );
    assert_eq!(
        run(&base, &["tasks", "new", "--title", "Reading task"]).0,
        0
    );
    let content = "# Header\n\nA long source line with \u{4efb}\u{52a1}, e\u{301} and \u{1f469}\u{200d}\u{1f4bb} that wraps more than once across the display.\n\n```text\n| first | second |\n```\n";
    assert_eq!(
        run(&base, &["tasks", "brief", "APP-001", "--body", content]).0,
        0
    );
    base
}

#[test]
fn data_scope_merges_records_before_filtering_and_keeps_provenance_separate() {
    let base = reading_fixture("data-scope-records");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Alpha live"]).0, 0);
    assert_eq!(run(&base, &["tasks", "start", "APP-001"]).0, 0);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "complete",
                "APP-001",
                "--evidence",
                "tasks/APP-001.md",
                "--commit",
                "abc1234"
            ]
        )
        .0,
        0
    );
    assert_eq!(run(&base, &["tasks", "archive", "APP-001"]).0, 0);
    let (code, text, err) = run(
        &base,
        &[
            "tasks",
            "list",
            "--data-scope",
            "all",
            "--sort",
            "title",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let data: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["count"], 2);
    assert_eq!(data["data"][0]["id"], "APP-002");
    assert_eq!(data["provenance"][0]["archived"], false);
    assert_eq!(data["provenance"][1]["archived"], true);
    assert!(
        data["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("@archive").is_none())
    );
    let mixed = run(
        &base,
        &["tasks", "list", "--data-scope", "all", "--width", "100"],
    );
    assert_eq!(mixed.0, 0, "{}", mixed.2);
    assert!(mixed.1.contains("@archive") && mixed.1.contains("true") && mixed.1.contains("false"));
    let selected = run(
        &base,
        &[
            "tasks",
            "show",
            "app-001",
            "--data-scope",
            "archived",
            "--json",
        ],
    );
    assert_eq!(selected.0, 0, "{}", selected.2);
    assert!(selected.1.contains("APP-001"));
    assert_ne!(run(&base, &["tasks", "show", "APP-001"]).0, 0);
    let filtered = run(
        &base,
        &[
            "tasks",
            "list",
            "--data-scope",
            "all",
            "--filter",
            "state = done",
            "--json",
        ],
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&filtered.1).unwrap()["count"],
        1
    );
    let before = read(&base, ".clerk/data/tasks.csv");
    assert_ne!(
        run(
            &base,
            &["tasks", "new", "--title", "No write", "--data-scope", "all"]
        )
        .0,
        0
    );
    assert_ne!(run(&base, &["tasks", "list", "--archived"]).0, 0);
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    let archive = read(&base, ".clerk/archive/tasks.csv");
    let table = clerk::storage::store::Table::from_text(&archive).unwrap();
    let row = table.record(0);
    let mut live = clerk::storage::store::Table::from_text(&before).unwrap();
    live.rows.push(
        live.headers
            .iter()
            .map(|name| row.get(name).cloned().unwrap_or_default())
            .collect(),
    );
    let mut rows = vec![live.headers.clone()];
    rows.extend(live.rows);
    write(
        &base,
        ".clerk/data/tasks.csv",
        &clerk::foundation::csv::serialize(&rows),
    );
    let conflict = run(&base, &["tasks", "show", "APP-001", "--data-scope", "all"]);
    assert_ne!(conflict.0, 0);
    assert!(conflict.2.contains("duplicate") && conflict.2.contains("scope"));
}

#[test]
fn archive_scope_applies_to_document_kinds_and_preserves_business_scope() {
    let base = root("data-scope-documents");
    assert_eq!(
        run(&base, &["contracts", "new", "--title", "Archived document"]).0,
        0
    );
    assert_eq!(run(&base, &["contracts", "start", "CON-01"]).0, 0);
    assert_eq!(run(&base, &["contracts", "finish", "CON-01"]).0, 0);
    assert_eq!(run(&base, &["contracts", "archive", "CON-01"]).0, 0);
    assert_eq!(
        run(&base, &["contracts", "new", "--title", "Current document"]).0,
        0
    );
    let out = run(
        &base,
        &[
            "contracts",
            "list",
            "--data-scope",
            "all",
            "--sort",
            "title",
            "--json",
        ],
    );
    assert_eq!(out.0, 0, "{}", out.2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out.1).unwrap()["count"],
        2
    );
    assert_eq!(
        run(
            &base,
            &["contracts", "show", "con-01", "--data-scope", "archived"]
        )
        .0,
        0
    );
    let out = run(
        &base,
        &[
            "reviews",
            "new",
            "--date",
            "20261103",
            "--time",
            "121212",
            "--slug",
            "scope",
            "--title",
            "Review scope",
            "--scope",
            "business area",
        ],
    );
    assert_eq!(out.0, 0, "{}", out.2);
}

#[test]
fn read_option_prefixes_help_and_machine_metadata_share_one_contract() {
    let base = root("reader-options");
    for flags in [
        vec!["-o", "p", "-p", "al"],
        vec!["--overflow", "f", "--paging", "au"],
    ] {
        let mut args = vec!["tasks", "list"];
        args.extend(flags);
        assert_eq!(run(&base, &args).0, 0);
    }
    let ambiguous = run(&base, &["tasks", "list", "-p", "a", "-o", "p"]);
    assert_eq!(ambiguous.0, 2);
    assert!(
        ambiguous.2.contains("ambiguous")
            && ambiguous.2.contains("auto")
            && ambiguous.2.contains("always")
    );
    assert_ne!(run(&base, &["tasks", "list", "-p", "al", "-o", "f"]).0, 0);
    for args in [
        vec!["tas", "lis", "--help"],
        vec!["help", "tas", "lis"],
        vec!["contr", "--help"],
    ] {
        let result = run(&base, &args);
        assert_eq!(result.0, 0, "{}", result.2);
        assert!(result.1.contains("Options:"));
    }
    assert_ne!(run(&base, &["c", "--help"]).0, 0);
    let out = run(&base, &["explain", "tasks", "--json"]);
    assert_eq!(out.0, 0, "{}", out.2);
    let schema: serde_json::Value = serde_json::from_str(&out.1).unwrap();
    let fields = schema["command_options"].as_array().unwrap();
    let paging = fields
        .iter()
        .find(|field| field["name"] == "paging")
        .unwrap();
    assert_eq!(paging["short"], "p");
    assert_eq!(paging["group"], "Reading");
    assert_eq!(paging["cli_prefix"], true);
    assert!(
        !paging["verbs"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("new"))
    );
    let config = read(&base, ".clerk/workspace.toml");
    write(
        &base,
        ".clerk/workspace.toml",
        &(config + "\n[commands.\"tasks.list\"]\noverflow = 'p'\n"),
    );
    assert_ne!(
        run(&base, &["tasks", "list"]).0,
        0,
        "persistent declarations require complete values"
    );
}

#[test]
fn context_full_restores_unicode_bodies_and_keeps_machine_output_clean() {
    let base = reading_fixture("context-full-excerpts");
    let declaration = read(&base, ".clerk/config/kinds/tasks.toml")
        + "\n[[relation]]\nname = \"contract\"\ntarget = \"contracts\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    let contract = format!("{}\nEND-OF-CONTRACT", "contract content ".repeat(200));
    assert_eq!(
        run(
            &base,
            &[
                "contracts",
                "new",
                "--title",
                "Related",
                "--body",
                &contract
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "link",
                "APP-001",
                "contract",
                "contracts",
                "CON-01"
            ]
        )
        .0,
        0
    );
    let body = format!(
        "{}\nEND-OF-ATTACHMENT",
        "\u{4e2d}\u{6587} source line\n".repeat(450)
    );
    assert_eq!(
        run(&base, &["tasks", "brief", "APP-001", "--body", &body]).0,
        0
    );
    for index in 0..6 {
        let log = format!("{}\nEND-OF-LOG-{index}", "log content ".repeat(180));
        assert_eq!(
            run(
                &base,
                &[
                    "logs",
                    "add",
                    "--title",
                    &format!("Stage-{index}"),
                    "--tasks",
                    "APP-001",
                    "--body",
                    &log
                ]
            )
            .0,
            0
        );
    }
    let (code, short, stderr) = run(&base, &["context", "tasks", "APP-001", "--plain"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(short.contains("Truncated: true") && short.contains("use --full"));
    assert!(!short.contains("END-OF-ATTACHMENT") && !short.contains("END-OF-LOG"));
    assert!(!short.contains("END-OF-CONTRACT"));
    let (code, json, stderr) = run(
        &base,
        &[
            "context", "tasks", "app-001", "--full", "--json", "--color", "always",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(!json.contains('\x1b'));
    let bundle: serde_json::Value = serde_json::from_str(&json).unwrap();
    let parts = bundle["parts"].as_array().unwrap();
    let attachment = parts.iter().find(|part| part["role"] == "brief").unwrap();
    assert_eq!(attachment["text"].as_str(), Some(body.as_str()));
    assert!(parts.iter().all(|part| part["truncated"] == false));
    assert!(parts.iter().any(|part| part["role"] == "related document"
        && part["text"].as_str().unwrap().contains("END-OF-CONTRACT")));
    let logs = parts
        .iter()
        .filter(|part| part["role"] == "log explicit reference")
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 5, "--full must retain the log selection bound");
    assert!(
        logs.iter()
            .all(|part| part["text"].as_str().unwrap().contains("END-OF-LOG"))
    );
    let plain = run(
        &base,
        &[
            "context", "tasks", "APP-001", "--full", "--plain", "--color", "never",
        ],
    )
    .1;
    assert!(plain.contains("END-OF-ATTACHMENT") && !plain.contains("Truncated: true"));
    assert!(!plain.contains('\x1b'));
    let config = read(&base, ".clerk/workspace.toml") + "\n[commands.context]\nfull = true\n";
    write(&base, ".clerk/workspace.toml", &config);
    assert!(
        run(&base, &["context", "tasks", "APP-001", "--json"])
            .1
            .contains("END-OF-ATTACHMENT")
    );
    assert!(
        !run(
            &base,
            &["context", "tasks", "APP-001", "--no-full", "--json"]
        )
        .1
        .contains("END-OF-ATTACHMENT")
    );
}

#[test]
fn context_styles_use_declared_field_roles_and_preserve_literal_bodies() {
    let base = reading_fixture("context-semantic-colors");
    let kind = read(&base, ".clerk/config/kinds/tasks.toml");
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &kind.replace(
            "name = \"state\"",
            "name = \"state\"\nterminal_style = { todo = \"warning\" }",
        ),
    );
    let body = "Version: an ordinary source line long enough to wrap across multiple visual rows without becoming a heading.";
    assert_eq!(
        run(&base, &["tasks", "brief", "APP-001", "--body", body]).0,
        0
    );
    let (code, colored, stderr) = run(
        &base,
        &[
            "context", "tasks", "APP-001", "--width", "36", "--color", "always",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(colored.contains("\x1b[33mtodo\x1b[0m"), "{colored:?}");
    assert!(colored.contains("\x1b[36mAPP-001\x1b[0m"), "{colored:?}");
    assert!(
        colored.contains("\x1b[90mtasks/APP-001.md:1\x1b[0m"),
        "{colored:?}"
    );
    let ansi = regex::Regex::new("\x1b\\[[0-9;]*m").unwrap();
    for line in colored.lines().filter(|line| line.contains('|')) {
        assert!(
            line[..line.find('|').unwrap()].contains("\x1b[90m"),
            "{line:?}"
        );
        assert!(
            !line.contains("\x1b[1m"),
            "literal source was treated as metadata"
        );
        assert!(clerk::terminal::text::display_width(&ansi.replace_all(line, "")) <= 36);
    }
    let (_, plain, _) = run(
        &base,
        &[
            "context", "tasks", "APP-001", "--width", "36", "--color", "never",
        ],
    );
    let stable = |text: &str| {
        text.lines()
            .filter(|line| !line.starts_with("Observed:"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(stable(&ansi.replace_all(&colored, "")), stable(&plain));
    assert_eq!(read(&base, "tasks/APP-001.md"), body);
}

#[test]
fn context_reading_respects_total_width_and_plain_pipe_output() {
    let base = reading_fixture("context-reading-width");
    let before = read(&base, "tasks/APP-001.md");
    let (code, plain, stderr) = run(&base, &["context", "tasks", "APP-001"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(plain.contains("      | # Header") && !plain.contains("1 | # Header"));
    let explicit = run(&base, &["context", "tasks", "APP-001", "--plain"]).1;
    let stable = |text: &str| {
        text.lines()
            .filter(|line| !line.starts_with("Observed:"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(stable(&plain), stable(&explicit));
    let piped = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .args([
            "--root",
            base.to_str().unwrap(),
            "context",
            "tasks",
            "APP-001",
        ])
        .env("COLUMNS", "20")
        .output()
        .unwrap();
    assert!(piped.status.success());
    assert_eq!(
        stable(&plain),
        stable(&String::from_utf8(piped.stdout).unwrap())
    );
    for width in [20, 40, 80] {
        let (code, output, stderr) = run(
            &base,
            &["context", "tasks", "APP-001", "--width", &width.to_string()],
        );
        assert_eq!(code, 0, "{stderr}");
        assert!(
            output
                .lines()
                .all(|line| clerk::terminal::text::display_width(line) <= width),
            "{output}"
        );
        assert!(
            output.contains("1 | # Header") && output.contains("2 | \n"),
            "{output}"
        );
        assert!(
            output.lines().any(|line| line.starts_with("        | ")),
            "{output}"
        );
        assert_eq!(read(&base, "tasks/APP-001.md"), before);
    }
    let (_, json, _) = run(&base, &["context", "tasks", "APP-001", "--json"]);
    assert!(
        json.contains(&format!(
            "\"text\":{}",
            clerk::config::schema::quote(&before)
        )),
        "{json}"
    );
}

#[test]
fn context_reading_log_numbers_start_at_the_source_section() {
    let base = reading_fixture("context-reading-log-line");
    assert_eq!(
        run(
            &base,
            &[
                "logs",
                "add",
                "--title",
                "Earlier",
                "--body",
                "Unrelated entry"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &[
                "logs",
                "add",
                "--title",
                "Referenced",
                "--tasks",
                "APP-001",
                "--body",
                "Related entry"
            ]
        )
        .0,
        0
    );
    let (_, json, _) = run(&base, &["context", "tasks", "APP-001", "--json"]);
    let entry = json
        .split("\"role\":\"log explicit reference\"")
        .nth(1)
        .expect("explicit reference");
    let number = entry
        .split("\"line\":")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(number > 1);
    let (code, output, stderr) = run(&base, &["context", "tasks", "APP-001", "--width", "100"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(output.contains(&format!("{number} | ## [T")), "{output}");
}

#[test]
fn reading_options_reject_conflicts_and_keep_machine_outputs_unadorned() {
    let base = reading_fixture("reading-option-validation");
    for args in [
        vec!["context", "tasks", "APP-001", "--plain", "--width", "80"],
        vec!["context", "tasks", "APP-001", "--width", "0"],
        vec!["context", "tasks", "APP-001", "--width", "19"],
        vec!["context", "tasks", "APP-001", "--width", "10001"],
        vec!["context", "tasks", "APP-001", "--width", "bad"],
        vec!["context", "tasks", "APP-001", "--json", "--plain"],
        vec!["workflow", "preview", "start", "--width", "80"],
    ] {
        let (code, _, stderr) = run(&base, &args);
        assert_eq!(code, 2, "{args:?}: {stderr}");
    }
    let marker = read(&base, ".clerk/workspace.toml");
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{marker}\n[commands.context]\nwidth = 40\n"),
    );
    let (code, output, stderr) = run(&base, &["context", "tasks", "APP-001"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(output.contains("1 | # Header"));
    let (code, output, stderr) = run(&base, &["context", "tasks", "APP-001", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(output.starts_with('{') && !output.contains("1 | # Header"));
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{marker}\n[commands.context]\nwidth = 40\njson = true\n"),
    );
    let (code, output, stderr) = run(&base, &["context", "tasks", "APP-001"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(output.starts_with('{'));
    let (code, output, stderr) = run(&base, &["context", "tasks", "APP-001", "--no-json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(output.contains("1 | # Header"));
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{marker}\n[commands.context]\nwidth = 0\n"),
    );
    assert_eq!(run(&base, &["check"]).0, 2);
}

#[test]
fn workflow_reading_uses_original_file_lines_and_preserves_preview_toml() {
    let base = reading_fixture("workflow-reading-source");
    let template = "# Original comment\nversion = 1\ntitle = \"Reading workflow\"\n\n[[step]]\nkind = \"tasks\"\nverb = \"start\"\nkeys = [\"APP-001\"]\n";
    write(&base, ".clerk/config/workflows/start.toml", template);
    let (code, output, stderr) = run(&base, &["workflow", "show", "start", "--width", "40"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        output.contains("1 | # Original comment") && output.contains("2 | version = 1"),
        "{output}"
    );
    assert!(
        output
            .lines()
            .all(|line| clerk::terminal::text::display_width(line) <= 40)
    );
    let (code, output, stderr) = run(&base, &["workflow", "show", "start", "--plain"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(output.contains("| # Original comment") && !output.contains("1 | #"));
    let (code, preview, stderr) = run(&base, &["workflow", "preview", "start"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        clerk::config::decl::parse(&preview, "preview").is_ok(),
        "{preview}"
    );
    assert_eq!(read(&base, ".clerk/config/workflows/start.toml"), template);
}

fn commit_policy(base: &Path, policy: &str) {
    write(
        base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[policy]\n{policy}\n"),
    );
    initial_commit(base);
}

#[test]
fn commit_message_request_and_batch_share_the_policy() {
    let base = root("commit-message-request");
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    let (code, stdout, stderr) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Requested",
            "--request",
            "new-task",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    initial_commit(&base);
    let (code, stdout, stderr) = run(
        &base,
        &[
            "request",
            "apply",
            "--message",
            "feat(tasks): accept the requested task",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(
        git(&base, &["log", "-1", "--format=%B"])
            .starts_with("feat(tasks): accept the requested task")
    );
    assert_eq!(
        run(
            &base,
            &[
                "focus",
                "add",
                "APP-001",
                "--message",
                "chore(batch): schedule APP-001"
            ]
        )
        .0,
        0
    );
    let head = git(&base, &["rev-parse", "HEAD"]);
    assert_eq!(
        run(
            &base,
            &[
                "focus",
                "add",
                "APP-001",
                "--message",
                "chore(batch): keep the scheduled task"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        git(&base, &["rev-parse", "HEAD"]),
        head,
        "a no-op must not create a commit"
    );
    assert_eq!(run(&base, &["check"]).0, 0);
}

#[test]
fn commit_message_auto_workflow_summarizes_every_step_in_one_commit() {
    let base = root("commit-message-workflow-auto");
    write(
        &base,
        "flow.toml",
        "version = 1\n[[step]]\nkind = \"tasks\"\nverb = \"new\"\nas = \"task\"\nvalues = { title = \"Created\" }\n[[step]]\nkind = \"tasks\"\nverb = \"start\"\nkeys = [\"@task\"]\n[[step]]\nkind = \"focus\"\nverb = \"add\"\nkeys = [\"@task\"]\n",
    );
    commit_policy(&base, "auto_commit = true");
    let (code, stdout, stderr) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let message = git(&base, &["log", "-1", "--format=%B"]);
    assert!(
        message.starts_with("chore(clerk): add 1 and update 1 managed object(s)"),
        "{message}"
    );
    for detail in ["Added tasks (1): APP-001.", "Updated focus (1): APP-001."] {
        assert!(message.contains(detail), "{message}");
    }
    assert_eq!(message.matches("Clerk-Operation:").count(), 1);
    assert_eq!(git(&base, &["rev-list", "--count", "HEAD"]).trim(), "2");
}

#[test]
fn compact_operation_ids_preserve_legacy_receipts_and_latest_order() {
    let base = root("commit-message-operation-legacy");
    commit_policy(&base, "auto_commit = true");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Legacy"]).0, 0);
    let directory = base.join(".git/clerk/operations");
    let original = std::fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let body = std::fs::read_to_string(&original).unwrap();
    let mut receipt: toml::Table = body.parse().unwrap();
    let old_id = "00000000000000000000000000000001-123";
    receipt.insert("id".into(), toml::Value::String(old_id.into()));
    receipt.remove("message");
    let legacy_path = directory.join(format!("{old_id}.toml"));
    std::fs::write(&legacy_path, receipt.to_string()).unwrap();
    std::fs::remove_file(original).unwrap();
    let (code, stdout, stderr) = run(&base, &["operation", "status", old_id, "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(
        stdout.contains(old_id) && stdout.contains("\"message\":null"),
        "{stdout}"
    );
    assert_eq!(run(&base, &["tasks", "start", "APP-001"]).0, 0);
    let (code, stdout, stderr) = run(&base, &["operation", "status", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(
        stdout.contains("\"id\":\"op-") && !stdout.contains(old_id),
        "{stdout}"
    );
    // A receipt that predates message persistence cannot invent a new message on retry.
    receipt.insert("commit".into(), toml::Value::String("failed".into()));
    std::fs::write(&legacy_path, receipt.to_string()).unwrap();
    let (code, _, stderr) = run(&base, &["operation", "retry", old_id]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("no recorded commit message"), "{stderr}");
    assert_eq!(run(&base, &["operation", "status", "../outside"]).0, 2);
}

#[test]
fn commit_message_required_rejects_before_business_writes() {
    let base = root("commit-message-required");
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    let previous = git(&base, &["rev-parse", "HEAD"]);
    let original = read(&base, ".clerk/data/tasks.csv");
    for args in [
        vec!["tasks", "new", "--title", "Blocked"],
        vec!["tasks", "new", "--title", "Blocked", "--dry-run"],
        vec!["focus", "set"],
        vec!["render"],
        vec!["seal", "--note", "No write"],
        vec!["request", "apply"],
        vec![
            "people",
            "add",
            "--person",
            "alice",
            "--email",
            "alice@example.com",
        ],
    ] {
        let (code, _, stderr) = run(&base, &args);
        assert_eq!(code, 2, "{args:?}: {stderr}");
        assert!(stderr.contains("supply --message"), "{stderr}");
        assert_eq!(read(&base, ".clerk/data/tasks.csv"), original);
        assert_eq!(git(&base, &["rev-parse", "HEAD"]), previous);
        assert!(git(&base, &["status", "--porcelain"]).is_empty());
    }
    let message = "feat(tasks): register the first task\n\nDescribe the accepted scope.\n\nVerified the task input.";
    let (code, stdout, stderr) = run(
        &base,
        &["tasks", "new", "--title", "Accepted", "--message", message],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("APP-001"), "{stdout}");
    let saved = git(&base, &["log", "-1", "--format=%B"]);
    assert!(saved.starts_with(message), "{saved}");
    assert_eq!(saved.matches("Clerk-Operation:").count(), 1);
}

#[test]
fn commit_message_auto_summarizes_objects_without_field_values() {
    let base = root("commit-message-auto");
    commit_policy(
        &base,
        "auto_commit = true\ncommit_mode = \"auto\"\ncommit_message = \"legacy subject\"",
    );
    assert_eq!(run(&base, &["tasks", "new", "--title", "First"]).0, 0);
    let new = git(&base, &["log", "-1", "--format=%B"]);
    assert!(
        new.starts_with("chore(clerk): add 1 managed object(s)"),
        "{new}"
    );
    assert!(
        !new.contains("First") && new.contains(".clerk/data/tasks.csv"),
        "{new}"
    );
    assert_eq!(
        run(&base, &["tasks", "set", "APP-001", "--title", "Renamed"]).0,
        0
    );
    let edited = git(&base, &["log", "-1", "--format=%B"]);
    assert!(
        edited.contains("Updated tasks (1): APP-001.") && !edited.contains("Renamed"),
        "{edited}"
    );
    assert_eq!(run(&base, &["tasks", "start", "APP-001"]).0, 0);
    let started = git(&base, &["log", "-1", "--format=%B"]);
    assert!(started.contains("Updated tasks (1): APP-001."), "{started}");
    assert!(started.lines().next().unwrap().len() <= 72);
}

#[test]
fn commit_message_fixed_mode_and_legacy_configuration_remain_compatible() {
    for (name, mode) in [
        ("commit-message-default", "commit_mode = \"default\"\n"),
        ("commit-message-legacy", ""),
    ] {
        let base = root(name);
        commit_policy(
            &base,
            &format!(
                "auto_commit = true\n{mode}commit_message = \"chore(records): fixed subject\""
            ),
        );
        assert_eq!(run(&base, &["tasks", "new", "--title", "Fixed"]).0, 0);
        let fixed = git(&base, &["log", "-1", "--format=%B"]);
        assert!(
            fixed.starts_with("chore(records): fixed subject\n"),
            "{fixed}"
        );
        assert!(fixed.contains("Added tasks (1): APP-001."), "{fixed}");
        assert_eq!(
            run(
                &base,
                &[
                    "tasks",
                    "start",
                    "APP-001",
                    "--message",
                    "chore(tasks): explicit subject"
                ]
            )
            .0,
            0
        );
        assert!(
            git(&base, &["log", "-1", "--format=%B"])
                .starts_with("chore(tasks): explicit subject\n")
        );
    }
}

#[test]
fn commit_message_file_is_utf8_and_preserves_multiline_content() {
    let base = root("commit-message-file");
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    write(
        &base,
        ".git/message.txt",
        "\u{feff}feat(tasks): preserve body\r\n\r\n# A literal heading\r\n\r\nVerified UTF-8 text: 中文。\r\n",
    );
    let path = base.join(".git/message.txt");
    let (code, stdout, stderr) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "File",
            "--message-file",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let saved = git(&base, &["log", "-1", "--format=%B"]);
    assert!(
        saved.contains("# A literal heading") && saved.contains("Verified UTF-8 text: 中文。"),
        "{saved}"
    );
    let (_, receipt, _) = run(&base, &["operation", "status", "--json"]);
    assert!(
        receipt.contains("preserve body") && receipt.contains("Clerk-Operation:"),
        "{receipt}"
    );
}

#[test]
fn commit_message_invalid_sources_and_stdin_conflicts_do_not_mutate() {
    let base = root("commit-message-invalid");
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    std::fs::write(base.join(".git/non-utf8"), [0xff, 0xfe]).unwrap();
    let bad_file = base.join(".git/non-utf8");
    for args in [
        vec!["tasks", "new", "--title", "No", "--message", "   "],
        vec!["tasks", "new", "--title", "No", "--message", "\nNo subject"],
        vec![
            "tasks",
            "new",
            "--title",
            "No",
            "--message",
            "Bad\u{1b}message",
        ],
        vec![
            "tasks",
            "new",
            "--title",
            "No",
            "--message",
            "Subject\n\nClerk-Operation: forged",
        ],
        vec![
            "tasks",
            "new",
            "--title",
            "No",
            "--message",
            "A",
            "--message-file",
            "missing",
        ],
        vec![
            "tasks",
            "new",
            "--title",
            "No",
            "--message-file",
            bad_file.to_str().unwrap(),
        ],
        vec!["contracts", "new", "--title", "No", "--message-file", "-"],
        vec!["tasks", "new", "--from", "-", "--message-file", "-"],
        vec!["batch", "--file", "-", "--message-file", "-"],
        vec![
            "tasks",
            "brief",
            "APP-001",
            "--file",
            "-",
            "--message-file",
            "-",
        ],
    ] {
        let (code, _, stderr) = run(&base, &args);
        assert_ne!(code, 0, "{args:?}: {stderr}");
        assert!(
            git(&base, &["status", "--porcelain"]).is_empty(),
            "{args:?}: {stderr}"
        );
    }
}

#[test]
fn commit_message_stdin_supports_an_explicit_separate_body() {
    use std::io::Write;
    use std::process::Stdio;
    let base = root("commit-message-stdin");
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    let mut child = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .args([
            "--root",
            base.to_str().unwrap(),
            "contracts",
            "new",
            "--title",
            "Piped",
            "--body",
            "Contract body",
            "--message-file",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"feat(contracts): accept a piped message\n\nContract scope.\n\nVerified separate input.\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        git(&base, &["log", "-1", "--format=%B"])
            .starts_with("feat(contracts): accept a piped message")
    );
}

#[test]
fn commit_message_workflow_freezes_one_message_and_retry_does_not_replay() {
    let base = root("commit-message-retry");
    write(
        &base,
        "flow.toml",
        "version = 1\n[[step]]\nkind = \"tasks\"\nverb = \"new\"\nas = \"task\"\nvalues = { title = \"Only once\" }\n[[step]]\nkind = \"tasks\"\nverb = \"start\"\nkeys = [\"@task\"]\n",
    );
    commit_policy(&base, "auto_commit = true\ncommit_mode = \"required\"");
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            base.join(".git/hooks/pre-commit"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let message = "feat(tasks): freeze the original workflow intent\n\nCreate and start exactly one task.\n\nVerified by workflow validation.";
    let (code, _, stderr) = run(
        &base,
        &[
            "batch",
            "--file",
            base.join("flow.toml").to_str().unwrap(),
            "--message",
            message,
        ],
    );
    assert_ne!(code, 0, "{stderr}");
    assert!(stderr.contains("local data was saved"), "{stderr}");
    let records = read(&base, ".clerk/data/tasks.csv");
    assert!(records.contains("APP-001,Only once,active"));
    let (code, status, stderr) = run(&base, &["operation", "status", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    let id_start = status.find("\"id\":\"").unwrap() + 6;
    let id = status[id_start..].split('"').next().unwrap();
    assert!(id.starts_with("op-") && id.len() < 26, "{id}");
    assert_eq!(
        run(
            &base,
            &["operation", "retry", id, "--message", "Changed intent"]
        )
        .0,
        2
    );
    // Change local policy without changing HEAD or any business file digest.
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 0\n");
    git(
        &base,
        &[
            "update-index",
            "--assume-unchanged",
            ".clerk/workspace.toml",
        ],
    );
    write(
        &base,
        ".clerk/workspace.toml",
        &format!(
            "{WORKSPACE}\n[policy]\nauto_commit = true\ncommit_mode = \"default\"\ncommit_message = \"chore(clerk): changed default\"\n"
        ),
    );
    let (code, stdout, stderr) = run(&base, &["operation", "retry", id]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), records);
    assert!(git(&base, &["log", "-1", "--format=%B"]).starts_with(message));
    assert_eq!(git(&base, &["rev-list", "--count", "HEAD"]).trim(), "2");
}

#[test]
fn commit_message_modes_validate_at_load_and_no_commit_commands_reject_messages() {
    let base = root("commit-message-policy-validation");
    for policy in [
        "commit_mode = \"typo\"",
        "commit_mode = 1",
        "commit_message = false",
        "commit_message = \"\"",
    ] {
        write(
            &base,
            ".clerk/workspace.toml",
            &format!("{WORKSPACE}\n[policy]\n{policy}\n"),
        );
        assert_eq!(run(&base, &["check"]).0, 2, "{policy}");
    }
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[policy]\ncommit_mode = \"required\"\n"),
    );
    assert_eq!(
        run(&base, &["tasks", "new", "--title", "No auto commit"]).0,
        0
    );
    for args in [
        vec!["tasks", "list", "--message", "Ignored"],
        vec!["version", "--message", "Ignored"],
        vec!["session", "list", "--message", "Ignored"],
        vec!["workflow", "list", "--message", "Ignored"],
        vec![
            "tasks",
            "new",
            "--title",
            "Proposal",
            "--request",
            "proposal",
            "--message",
            "Ignored",
        ],
    ] {
        let (code, _, stderr) = run(&base, &args);
        assert_eq!(code, 2, "{args:?}: {stderr}");
    }
    let (code, schema, stderr) = run(&base, &["explain", "tasks", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        schema.contains("\"commit_policy\"") && schema.contains("\"mode\":\"required\""),
        "{schema}"
    );
}

const WORKSPACE: &str = r#"version = 1
name = "fixture"

[time]
offset_hours = 8

[ids.tasks]
pattern = "{prefix}-{serial:3}"
counter = "serial"
start = 1

[ids.tasks.tokens]
prefix = "prefixes"

[ids.contracts]
pattern = "CON-{serial:2}"
counter = "serial"

[ids.assets]
pattern = "A-{serial:2}"
counter = "serial"

[[path]]
path = ".clerk/data"
level = "sealed"

[[path]]
path = "contracts"
level = "sealed"

[[path]]
path = "reviews"
level = "validated"

[[path]]
path = ".clerk/assets.csv"
level = "sealed"

[[path]]
path = "log"
level = "validated"
"#;

const CONTRACT_KIND: &str = r#"title = "契约"
storage = "document"
dir = "contracts"
filename = "{id}.md"
key = ["id"]
numbering = "contracts"

[state]
field = "state"
registry = "states"
default = "todo"

[[field]]
name = "id"
type = "text"
role = "status"

[[field]]
name = "title"
type = "text"
required = true

[[field]]
name = "state"
type = "enum"
registry = "states"
role = "status"

[[field]]
name = "date"
type = "date"
role = "status"
auto = "today"

[[transition]]
name = "start"
to = "active"
from = ["todo"]

[[transition]]
name = "finish"
to = "done"
from = ["active"]
archive = true

[[view]]
name = "list"
columns = ["id", "title", "state"]
"#;

const ASSET_KIND: &str = r#"title = "资源"
table = ".clerk/assets.csv"
key = ["id"]
numbering = "assets"

[[field]]
name = "id"
type = "text"
role = "status"

[[field]]
name = "path"
type = "path"
required = true

[[field]]
name = "sha256"
type = "text"
role = "status"

[hash]
file = "path"
digest = "sha256"

[[view]]
name = "list"
columns = ["id", "path", "sha256"]
"#;
const LOGS_KIND: &str = r###"title = "日志"
storage = "document"
dir = "log"
archive = "log/archive"
filename = "{maintainer}/{date}.md"
key = ["maintainer", "date"]

[[field]]
name = "maintainer"
type = "text"
role = "status"

[[field]]
name = "date"
type = "date"
role = "status"

[[action]]
name = "add"
handler = "append"
heading = "## [T{time}] {title}"

[[action]]
name = "prune"
handler = "prune"

[[view]]
name = "list"
columns = ["maintainer", "date"]
"###;
const REVIEW_KIND: &str = r#"title = "复核"
storage = "document"
dir = "reviews"
filename = "{date}-{time}-{slug}.md"
meta = "bullets"
key = ["date", "time"]

[[field]]
name = "date"
type = "date"
display = "日期"

[[field]]
name = "time"
type = "text"
display = "时间"

[[field]]
name = "slug"
type = "text"

[[field]]
name = "title"
type = "text"

[[field]]
name = "scope"
type = "text"
display = "范围"

[[view]]
name = "list"
columns = ["date", "time", "slug", "title"]
"#;
const TASK_KIND: &str = r#"title = "任务"
table = ".clerk/data/tasks.csv"
key = ["id"]
numbering = "tasks"
relations_table = ".clerk/data/relations.csv"

[state]
field = "state"
registry = "states"
default = "todo"

[[field]]
name = "id"
type = "text"
role = "status"

[[field]]
name = "title"
type = "text"
required = true

[[field]]
name = "state"
type = "enum"
registry = "states"
role = "status"

[[field]]
name = "evidence"
type = "path"

[[field]]
name = "commit"
type = "commit"

[[field]]
name = "updated"
type = "stamp"
role = "status"
auto = "now"

[[field]]
name = "brief"
type = "path"
role = "status"

[[field]]
name = "completion"
type = "path"
role = "status"

[[field]]
name = "claimant"
type = "text"
role = "status"

[[field]]
name = "claimant_email"
type = "email"
role = "status"

[[field]]
name = "claimed_at"
type = "stamp"
role = "status"

[[transition]]
name = "start"
to = "active"
from = ["todo"]

[[transition]]
name = "complete"
to = "done"
from = ["active"]
requires = ["evidence", "commit"]
archive = true

[[action]]
name = "claim"
handler = "identity"
fields = ["claimant", "claimant_email", "claimed_at"]
require_empty = ["claimant"]

[[action]]
name = "release"
handler = "clear"
fields = ["claimant", "claimant_email", "claimed_at"]

[[action]]
name = "brief"
handler = "attach"
field = "brief"
filename = "tasks/{id}.md"

[[action]]
name = "finish"
handler = "attach"
field = "completion"
filename = "tasks/{id}-done.md"

[[relation]]
name = "blocked"
target = "tasks"

[[view]]
name = "list"
columns = ["id", "title", "state", "updated"]
"#;

fn root(name: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if base.exists() {
        std::fs::remove_dir_all(&base).expect("cleaned");
    }
    std::fs::create_dir_all(base.join(".clerk/config/registry")).expect("registry dir");
    std::fs::create_dir_all(base.join(".clerk/config/kinds")).expect("kinds dir");
    std::fs::create_dir_all(base.join(".clerk/data")).expect("data dir");

    write(&base, ".clerk/workspace.toml", WORKSPACE);
    write(
        &base,
        ".clerk/config/registry/states.csv",
        "key,display\ntodo,待开始\nactive,进行中\ndone,完成\n",
    );
    write(
        &base,
        ".clerk/config/registry/prefixes.csv",
        "key,display\nAPP,预约应用\n",
    );
    write(
        &base,
        ".clerk/config/registry/type.csv",
        "key,display\nability,能力\ndefect,缺陷\n",
    );
    write(&base, ".clerk/config/kinds/tasks.toml", TASK_KIND);
    write(&base, ".clerk/config/kinds/contracts.toml", CONTRACT_KIND);
    write(&base, ".clerk/config/kinds/assets.toml", ASSET_KIND);
    write(&base, ".clerk/config/kinds/reviews.toml", REVIEW_KIND);
    write(&base, ".clerk/config/kinds/logs.toml", LOGS_KIND);
    write(
        &base,
        ".clerk/data/tasks.csv",
        "id,title,state,evidence,commit,updated\n",
    );

    let status = Command::new("git")
        .arg("-C")
        .arg(&base)
        .args(["init", "-q"])
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed");
    for (key, value) in [
        ("user.name", "tester"),
        ("user.email", "tester@example.com"),
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&base)
            .args(["config", key, value])
            .status()
            .expect("git config");
        assert!(status.success(), "git config failed");
    }
    git(&base, &["checkout", "-q", "-b", "main"]);
    base
}

fn write(base: &Path, relative: &str, body: &str) {
    let path = base.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(&path, body).expect("written");
}

fn run(base: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .arg("--root")
        .arg(base)
        .args(args)
        .output()
        .expect("ran clerk");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn read(base: &Path, relative: &str) -> String {
    std::fs::read_to_string(base.join(relative)).expect("read")
}

fn git(base: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(base)
        .args(args)
        .output()
        .expect("ran git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Turn on automatic commits and start from a clean tree.
fn enable_auto_commit(base: &Path) {
    let path = base.join(".clerk/workspace.toml");
    let mut body = std::fs::read_to_string(&path).expect("read declaration");
    body.push_str("\n[policy]\nauto_commit = true\n");
    std::fs::write(&path, body).expect("written");
    initial_commit(base);
}

fn initial_commit(base: &Path) {
    git(base, &["add", "-A"]);
    git(base, &["commit", "-q", "-m", "initial state"]);
}

#[test]
fn allocates_numbers_in_order() {
    let base = root("numbers");
    let (code, stdout, _) = run(
        &base,
        &["tasks", "new", "--title", "第一项", "--prefix", "APP"],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-001"), "stdout: {stdout}");

    let (code, stdout, _) = run(
        &base,
        &["tasks", "new", "--title", "第二项", "--prefix", "APP"],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-002"), "stdout: {stdout}");

    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(table.starts_with("id,title,state,"), "table: {table}");
    assert!(table.contains("APP-001,第一项,todo,"), "table: {table}");
}

#[test]
fn list_filters_by_state() {
    let base = root("filters-ops");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "new", "--title", "乙"]);
    run(&base, &["tasks", "start", "APP-002"]);

    let (code, stdout, _) = run(&base, &["tasks", "list", "--state", "active"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-002"), "stdout: {stdout}");
    assert!(!stdout.contains("APP-001"), "stdout: {stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--state", "active", "--display"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-002"), "{stdout}");
    assert!(!stdout.contains("APP-001"), "{stdout}");
    // With --display the state cell shows the Chinese name.
    assert!(stdout.contains("进行中"), "{stdout}");
}

#[test]
fn transition_requires_its_fields() {
    let base = root("transition");
    run(&base, &["tasks", "new", "--title", "任务"]);
    run(&base, &["tasks", "start", "APP-001"]);

    let (code, _, stderr) = run(&base, &["tasks", "complete", "APP-001"]);
    assert_eq!(code, 3, "missing fields must fail validation: {stderr}");
    assert!(stderr.contains("evidence"), "stderr: {stderr}");

    let (code, _, stderr) = run(
        &base,
        &[
            "tasks",
            "complete",
            "APP-001",
            "--evidence",
            "tasks/APP-001-done.md",
            "--commit",
            "abcdef1",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains(",done,"));
}

#[test]
fn transitions_respect_the_source_state() {
    let base = root("source-state");
    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, _, stderr) = run(
        &base,
        &[
            "tasks",
            "complete",
            "APP-001",
            "--evidence",
            "x.md",
            "--commit",
            "abcdef1",
        ],
    );
    assert_eq!(code, 4, "wrong source state must conflict: {stderr}");
    assert!(stderr.contains("todo"), "stderr: {stderr}");
}

#[test]
fn set_refuses_the_state_field() {
    let base = root("set-state");
    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, _, stderr) = run(&base, &["tasks", "set", "APP-001", "--state", "done"]);
    assert_eq!(code, 2, "stderr: {stderr}");
    assert!(stderr.contains("transition"), "stderr: {stderr}");
}

#[test]
fn set_updates_a_field() {
    let base = root("set-field");
    run(&base, &["tasks", "new", "--title", "旧标题"]);
    let (code, _, stderr) = run(&base, &["tasks", "set", "APP-001", "--title", "新标题"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(table.contains("新标题"), "table: {table}");
}

#[test]
fn rejects_unknown_enum_values() {
    let base = root("enum");
    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, _, stderr) = run(
        &base,
        &["tasks", "set", "APP-001", "--title", "任务", "--title", "x"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let (code, _, stderr) = run(&base, &["tasks", "list", "--state", "nope"]);
    assert_eq!(code, 3, "stderr: {stderr}");
    assert!(stderr.contains("nope"), "stderr: {stderr}");
}

#[test]
fn creates_the_table_and_headers_on_first_write() {
    let base = root("first-write");
    std::fs::remove_file(base.join(".clerk/data/tasks.csv")).expect("removed");
    let (code, _, stderr) = run(&base, &["tasks", "new", "--title", "任务"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(
        table.starts_with("id,title,state,evidence,commit,updated"),
        "{table}"
    );
    assert!(table.contains("APP-001"), "{table}");
}

#[test]
fn check_reports_bad_data() {
    let base = root("check");
    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");

    write(
        &base,
        ".clerk/data/tasks.csv",
        "id,title,state,evidence,commit,updated\nAPP-001,任务,unknown,,,20261031-101010\n",
    );
    let (code, stdout, stderr) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("unknown"), "stdout: {stdout}");
}

#[test]
fn json_output_is_one_envelope() {
    let base = root("json");
    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, stdout, _) = run(&base, &["tasks", "list", "--json"]);
    assert_eq!(code, 0);
    let text = stdout.trim();
    assert!(text.starts_with("{\"kind\":\"tasks\""), "{text}");
    assert!(text.contains("\"count\":1"), "{text}");
    assert!(text.contains("\"data\":["), "{text}");
    assert!(text.contains("\"APP-001\""), "{text}");
}

#[test]
fn doctor_passes_on_a_complete_workspace() {
    let base = root("doctor");
    let (code, stdout, stderr) = run(&base, &["doctor"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("ok"), "stdout: {stdout}");
}

#[test]
fn kinds_and_explain_describe_the_declaration() {
    let base = root("explain");
    let (code, stdout, _) = run(&base, &["kinds"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("tasks"), "{stdout}");

    let (code, stdout, _) = run(&base, &["explain", "tasks"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("complete"), "{stdout}");
    assert!(stdout.contains("evidence"), "{stdout}");
}

#[test]
fn dry_run_reports_without_writing() {
    let base = root("dry-run");
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, stdout, _) = run(&base, &["tasks", "new", "--title", "任务", "--dry-run"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-001"), "{stdout}");
    let after = read(&base, ".clerk/data/tasks.csv");
    assert_eq!(before, after, "dry run must not write");
}

#[test]
fn unknown_kind_names_the_known_ones() {
    let base = root("unknown-kind");
    let (code, _, stderr) = run(&base, &["widgets", "list"]);
    assert_eq!(code, 2, "stderr: {stderr}");
    assert!(stderr.contains("tasks"), "stderr: {stderr}");
}

#[test]
fn a_write_records_a_seal_and_check_verifies_it() {
    let base = root("seal");
    let (code, _, stderr) = run(&base, &["tasks", "new", "--title", "任务"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let seal = read(&base, ".clerk/seal/seal.csv");
    assert!(
        seal.starts_with("key,sha256,sealed_at,by,by_email"),
        "{seal}"
    );
    assert!(seal.contains(".clerk/data/tasks.csv"), "{seal}");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");

    // A manual edit without a reseal must be reported.
    let table = read(&base, ".clerk/data/tasks.csv").replace("任务", "改过的");
    write(&base, ".clerk/data/tasks.csv", &table);
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("seal"), "stdout: {stdout}");
}

#[test]
fn dry_run_writes_no_seal() {
    let base = root("dry-run-seal");
    let (code, _, _) = run(&base, &["tasks", "new", "--title", "任务", "--dry-run"]);
    assert_eq!(code, 0);
    assert!(!base.join(".clerk/seal/seal.csv").exists());
}

#[test]
fn auto_commit_commits_the_data_and_the_seal() {
    let base = root("auto-commit");
    enable_auto_commit(&base);
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "任务"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let log = git(&base, &["log", "--format=%s"]);
    assert_eq!(log.lines().count(), 2, "log: {log}");
    assert!(
        log.contains("chore(clerk): add 1 managed object(s)"),
        "log: {log}"
    );

    let status = git(&base, &["status", "--porcelain"]);
    assert!(status.trim().is_empty(), "tree must be clean: {status}");

    let changed = git(&base, &["show", "--stat", "--format=", "HEAD"]);
    assert!(changed.contains("tasks.csv"), "{changed}");
    assert!(changed.contains("seal.csv"), "{changed}");
}

const STATUS_VIEW: &str = r#"

[[view]]
name = "status"
renderer = "table"
target = "status.md"
region = "tasks"
columns = ["id", "title", "state"]
"#;

fn add_status_view(base: &Path) {
    let path = base.join(".clerk/config/kinds/tasks.toml");
    let mut body = std::fs::read_to_string(&path).expect("read kind");
    body.push_str(STATUS_VIEW);
    std::fs::write(&path, body).expect("written");
}

#[test]
fn render_fills_the_declared_region_and_keeps_the_prose() {
    let base = root("render");
    add_status_view(&base);
    write(
        &base,
        "status.md",
        "# 现状\n\n前言。\n\n<!-- clerk:begin tasks -->\n旧内容\n<!-- clerk:end tasks -->\n\n后记。\n",
    );
    run(&base, &["tasks", "new", "--title", "任务"]);

    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let page = read(&base, "status.md");
    assert!(
        flat(&page).contains("| APP-001 | 任务 | 待开始 |"),
        "{page}"
    );
    assert!(page.contains("前言。"), "prose must survive: {page}");
    assert!(page.contains("后记。"), "prose must survive: {page}");
    assert!(!page.contains("旧内容"), "{page}");
}

#[test]
fn check_reports_a_missing_region() {
    let base = root("missing-region");
    add_status_view(&base);
    write(&base, "status.md", "# 现状\n\n没有区块标记。\n");
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("region"), "stdout: {stdout}");
}

#[test]
fn scope_lists_the_managed_paths() {
    let base = root("scope");
    let (code, stdout, _) = run(&base, &["scope"]);
    assert_eq!(code, 0);
    assert!(stdout.contains(".clerk/data"), "{stdout}");
    assert!(stdout.contains("sealed"), "{stdout}");
}

#[test]
fn document_create_list_and_transition() {
    let base = root("document");
    let (code, stdout, stderr) = run(&base, &["contracts", "new", "--title", "契约甲"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("CON-01"), "stdout: {stdout}");

    let page = read(&base, "contracts/CON-01.md");
    assert!(page.starts_with("# 契约甲"), "{page}");
    assert!(page.contains("<!-- clerk:meta"), "{page}");
    assert!(page.contains("id = CON-01"), "{page}");
    assert!(page.contains("state = todo"), "{page}");

    let (code, stdout, _) = run(&base, &["contracts", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("CON-01"), "{stdout}");
    assert!(stdout.contains("契约甲"), "{stdout}");

    let (code, _, stderr) = run(&base, &["contracts", "start", "CON-01"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(read(&base, "contracts/CON-01.md").contains("state = active"));

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");
}

#[test]
fn document_set_keeps_the_body() {
    let base = root("document-body");
    run(
        &base,
        &[
            "contracts",
            "new",
            "--title",
            "契约乙",
            "--body",
            "正文一段。",
        ],
    );
    let (code, _, stderr) = run(&base, &["contracts", "set", "CON-01", "--title", "契约丙"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let page = read(&base, "contracts/CON-01.md");
    assert!(page.contains("# 契约丙"), "{page}");
    assert!(page.contains("正文一段。"), "body must survive: {page}");
    assert!(!page.contains("契约乙"), "{page}");
}

#[test]
fn document_transition_refuses_the_wrong_source_state() {
    let base = root("document-state");
    run(&base, &["contracts", "new", "--title", "契约丁"]);
    run(&base, &["contracts", "start", "CON-01"]);
    let (code, _, stderr) = run(&base, &["contracts", "start", "CON-01"]);
    assert_eq!(code, 4, "stderr: {stderr}");
    assert!(stderr.contains("active"), "stderr: {stderr}");
}

fn finished_task(base: &Path) {
    run(base, &["tasks", "new", "--title", "任务"]);
    run(base, &["tasks", "start", "APP-001"]);
    run(
        base,
        &[
            "tasks",
            "complete",
            "APP-001",
            "--evidence",
            "tasks/APP-001-done.md",
            "--commit",
            "abcdef1",
        ],
    );
}

#[test]
fn archive_moves_the_row_and_restore_brings_it_back() {
    let base = root("archive");
    finished_task(&base);

    let (code, stdout, stderr) = run(&base, &["tasks", "archive", "APP-001"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let live = read(&base, ".clerk/data/tasks.csv");
    assert!(!live.contains("APP-001"), "live: {live}");
    let archived = read(&base, ".clerk/archive/tasks.csv");
    assert!(archived.contains("APP-001"), "archive: {archived}");

    let (_, stdout, _) = run(&base, &["tasks", "list"]);
    assert!(!stdout.contains("APP-001"), "stdout: {stdout}");
    let (_, stdout, _) = run(&base, &["tasks", "list", "--data-scope", "archived"]);
    assert!(stdout.contains("APP-001"), "stdout: {stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "restore", "APP-001"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("APP-001"));
}

#[test]
fn archive_refuses_an_unfinished_instance() {
    let base = root("archive-refused");
    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, _, stderr) = run(&base, &["tasks", "archive", "APP-001"]);
    assert_eq!(code, 4, "stderr: {stderr}");
    assert!(stderr.contains("todo"), "stderr: {stderr}");
}

#[test]
fn document_archive_moves_the_file() {
    let base = root("document-archive");
    run(&base, &["contracts", "new", "--title", "契约戊"]);
    run(&base, &["contracts", "start", "CON-01"]);
    run(&base, &["contracts", "finish", "CON-01"]);

    let (code, stdout, stderr) = run(&base, &["contracts", "archive", "CON-01"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(!base.join("contracts/CON-01.md").exists());
    assert!(base.join(".clerk/archive/contracts/CON-01.md").exists());

    let (code, _, stderr) = run(&base, &["contracts", "restore", "CON-01"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(base.join("contracts/CON-01.md").exists());
    assert!(!base.join(".clerk/archive/contracts/CON-01.md").exists());
}

#[test]
fn request_capture_writes_a_manifest_and_keeps_the_data() {
    let base = root("request-capture");
    initial_commit(&base);

    let (code, stdout, stderr) = run(
        &base,
        &["tasks", "new", "--title", "任务", "--request", "add-task"],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("captured"), "stdout: {stdout}");

    let manifest = read(&base, ".clerk/requests/pending/tasks-add-task.toml");
    assert!(manifest.contains("kind = \"tasks\""), "{manifest}");
    assert!(manifest.contains("verb = \"new\""), "{manifest}");
    assert!(manifest.contains("title = \"任务\""), "{manifest}");
    assert!(!manifest.contains("baseline = \"\""), "{manifest}");

    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(!table.contains("APP-001"), "table: {table}");

    let (code, stdout, _) = run(&base, &["request", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("add-task"), "{stdout}");
}

#[test]
fn request_apply_runs_the_ordinary_path() {
    let base = root("request-apply");
    initial_commit(&base);
    run(
        &base,
        &["tasks", "new", "--title", "任务", "--request", "add-task"],
    );

    let (code, stdout, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("APP-001"));
    assert!(
        !base
            .join(".clerk/requests/pending/tasks-add-task.toml")
            .exists()
    );
    let log = read(&base, ".clerk/requests/applied.log");
    assert!(log.contains("add-task"), "{log}");
}

#[test]
fn request_reject_drops_the_file_and_records_the_reason() {
    let base = root("request-reject");
    initial_commit(&base);
    run(
        &base,
        &["tasks", "new", "--title", "任务", "--request", "add-task"],
    );

    let (code, _, stderr) = run(
        &base,
        &["request", "reject", "add-task", "--reason", "先不加"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !base
            .join(".clerk/requests/pending/tasks-add-task.toml")
            .exists()
    );
    let log = read(&base, ".clerk/requests/rejected.log");
    assert!(log.contains("先不加"), "{log}");
}

#[test]
fn request_apply_refuses_an_unknown_baseline() {
    let base = root("request-baseline");
    initial_commit(&base);
    run(
        &base,
        &["tasks", "new", "--title", "任务", "--request", "add-task"],
    );
    let path = base.join(".clerk/requests/pending/tasks-add-task.toml");
    let body = std::fs::read_to_string(&path).expect("read").replace(
        &format!(
            "baseline = \"{}\"",
            git(&base, &["rev-parse", "--short", "HEAD"]).trim()
        ),
        "baseline = \"0000000\"",
    );
    std::fs::write(&path, body).expect("written");

    let (code, _, _) = run(&base, &["request", "apply"]);
    assert_eq!(code, 5, "stale baseline must stop the apply");
}

#[test]
fn sync_gate_refuses_a_write_when_the_remote_is_ahead() {
    let base = root("sync-gate");
    initial_commit(&base);
    git(&base, &["checkout", "-q", "-b", "side"]);
    write(&base, ".clerk/side.txt", "side\n");
    git(&base, &["add", "-A"]);
    git(&base, &["commit", "-q", "-m", "side work"]);
    let side = git(&base, &["rev-parse", "HEAD"]).trim().to_string();
    git(&base, &["checkout", "-q", "main"]);
    git(&base, &["update-ref", "refs/remotes/origin/main", &side]);

    let (code, stdout, stderr) = run(&base, &["sync", "check"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("behind 1"), "stdout: {stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "new", "--title", "任务"]);
    assert_eq!(code, 5, "stderr: {stderr}");
    assert!(
        stderr.contains("not in the local branch"),
        "stderr: {stderr}"
    );
}

#[test]
fn asset_creation_records_the_digest() {
    let base = root("asset-digest");
    write(&base, "assets/图.png", "binary-ish content\n");
    let (code, stdout, stderr) = run(&base, &["assets", "new", "--path", "assets/图.png"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("A-01"), "stdout: {stdout}");

    let rows = clerk::foundation::csv::parse(&read(&base, ".clerk/assets.csv")).expect("table");
    let headers = &rows[0];
    let digest_at = headers
        .iter()
        .position(|header| header == "sha256")
        .expect("sha256 column");
    let digest = rows[1][digest_at].clone();
    assert_eq!(digest.len(), 64, "digest: {digest}");
    assert!(
        digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );

    let (code, stdout, _) = run(&base, &["assets", "verify"]);
    assert_eq!(code, 0, "stdout: {stdout}");
}

#[test]
fn verify_reports_a_changed_file() {
    let base = root("asset-verify");
    write(&base, "assets/图.png", "first\n");
    run(&base, &["assets", "new", "--path", "assets/图.png"]);
    write(&base, "assets/图.png", "second\n");

    let (code, stdout, _) = run(&base, &["assets", "verify"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("does not match"), "stdout: {stdout}");
}

#[test]
fn verify_reports_a_missing_file() {
    let base = root("asset-missing");
    write(&base, "assets/图.png", "content\n");
    run(&base, &["assets", "new", "--path", "assets/图.png"]);
    std::fs::remove_file(base.join("assets/图.png")).expect("removed");

    let (code, stdout, _) = run(&base, &["assets", "verify"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("missing"), "stdout: {stdout}");
}

#[test]
fn registering_a_missing_file_is_refused() {
    let base = root("asset-absent");
    let (code, _, stderr) = run(&base, &["assets", "new", "--path", "assets/none.png"]);
    assert!(code != 0, "stderr: {stderr}");
    assert!(stderr.contains("missing"), "stderr: {stderr}");
}

const GENERATED_VIEW: &str = r#"

[[view]]
name = "index"
renderer = "table"
target = "index.md"
columns = ["id", "title", "state"]
"#;

#[test]
fn render_overwrites_a_generated_file_and_check_detects_drift() {
    let base = root("generated");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read");
    body.push_str(GENERATED_VIEW);
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    let mut workspace = std::fs::read_to_string(base.join(".clerk/workspace.toml")).expect("read");
    workspace.push_str("\n[[path]]\npath = \"index.md\"\nlevel = \"generated\"\n");
    std::fs::write(base.join(".clerk/workspace.toml"), workspace).expect("written");

    run(&base, &["tasks", "new", "--title", "任务"]);
    std::fs::write(base.join("index.md"), "手写内容\n").expect("written");
    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let page = read(&base, "index.md");
    assert!(page.contains("APP-001"), "page: {page}");
    assert!(!page.contains("手写内容"), "page: {page}");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");

    std::fs::write(base.join("index.md"), "改坏了\n").expect("written");
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("generated"), "stdout: {stdout}");
}

#[test]
fn two_views_can_share_one_region() {
    let base = root("composite");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read");
    body.push_str(
        r#"

[[view]]
name = "status-a"
renderer = "table"
target = "status.md"
region = "tasks"
columns = ["id", "title", "state"]

[[view]]
name = "status-b"
renderer = "table"
target = "status.md"
region = "tasks"
columns = ["id", "state"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    std::fs::write(
        base.join("status.md"),
        "# 现状\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    )
    .expect("written");

    run(&base, &["tasks", "new", "--title", "任务"]);
    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let page = read(&base, "status.md");
    assert_eq!(flat(&page).matches("| id |").count(), 2, "page: {page}");
    assert!(page.contains("APP-001"), "page: {page}");
}

#[test]
fn registry_identity_is_the_first_column() {
    let registry =
        clerk::config::registry::Registry::parse("code,name\nAPP,reservation\n", "prefixes")
            .expect("loaded");
    assert_eq!(registry.keys(), vec!["APP"]);
    assert_eq!(registry.display_of("APP"), "APP");
    assert!(
        !registry.is_domain(),
        "no display column means a plain list"
    );
}

#[test]
fn link_writes_the_three_column_relation_table() {
    let base = root("link");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "new", "--title", "乙"]);
    let (code, stdout, stderr) = run(
        &base,
        &["tasks", "link", "APP-001", "blocked", "tasks", "APP-002"],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let table = read(&base, ".clerk/data/relations.csv");
    assert!(table.starts_with("from,kind,to"), "table: {table}");
    assert!(table.contains("APP-001,blocked,APP-002"), "table: {table}");

    let (code, _, stderr) = run(
        &base,
        &["tasks", "link", "APP-001", "blocked", "tasks", "APP-002"],
    );
    assert_eq!(code, 4, "duplicate link must conflict: {stderr}");

    let (code, _, stderr) = run(
        &base,
        &["tasks", "unlink", "APP-001", "blocked", "tasks", "APP-002"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!read(&base, ".clerk/data/relations.csv").contains("APP-001,blocked"));
}

#[test]
fn show_prints_one_field_per_line() {
    let base = root("show-detail");
    run(&base, &["tasks", "new", "--title", "详情"]);
    let (code, stdout, _) = run(&base, &["tasks", "show", "APP-001"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("title"), "{stdout}");
    assert!(stdout.contains("详情"), "{stdout}");
    assert!(
        !stdout.contains("---"),
        "detail view must not draw a table: {stdout}"
    );
}

#[test]
fn evidence_may_reference_another_repository() {
    let base = root("cross-repo");
    let (code, _, stderr) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "跨仓",
            "--evidence",
            "../library-app/benchmarks/x.md",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let (code, _, stderr) = run(
        &base,
        &["tasks", "new", "--title", "绝对", "--evidence", "/etc/x.md"],
    );
    assert_eq!(code, 3, "stderr: {stderr}");
}

#[test]
fn bullets_metadata_round_trips() {
    let base = root("bullets");
    let (code, stdout, stderr) = run(
        &base,
        &[
            "reviews",
            "new",
            "--date",
            "20261031",
            "--time",
            "101112",
            "--slug",
            "reservation",
            "--title",
            "预约应用复核",
        ],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let page = read(&base, "reviews/20261031-101112-reservation.md");
    assert!(page.starts_with("# 预约应用复核"), "{page}");
    assert!(page.contains("- 日期: 20261031"), "{page}");
    assert!(!page.contains("clerk:meta"), "{page}");

    let (code, stdout, _) = run(&base, &["reviews", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("reservation"), "{stdout}");

    let (code, stdout, _) = run(&base, &["reviews", "show", "20261031", "101112"]);
    assert_eq!(code, 0, "composite key must address the page: {stdout}");
    assert!(stdout.contains("预约应用复核"), "{stdout}");

    let (code, _, stderr) = run(
        &base,
        &[
            "reviews",
            "set",
            "20261031",
            "101112",
            "--scope",
            "预约应用",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(read(&base, "reviews/20261031-101112-reservation.md").contains("- 范围: 预约应用"));
}

#[test]
fn files_outside_the_name_pattern_are_ignored() {
    let base = root("name-pattern");
    run(
        &base,
        &[
            "reviews",
            "new",
            "--date",
            "20261031",
            "--time",
            "101112",
            "--slug",
            "reservation",
            "--title",
            "预约应用复核",
        ],
    );
    write(&base, "reviews/README.md", "# 索引\n\n说明。\n");

    let (code, stdout, _) = run(&base, &["reviews", "list"]);
    assert_eq!(code, 0, "stdout: {stdout}");
    assert!(
        !stdout.contains("README"),
        "index page must not be an instance: {stdout}"
    );
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");
}

#[test]
fn creating_a_document_appends_its_index_row() {
    let base = root("index-append");
    let mut body = std::fs::read_to_string(base.join(".clerk/config/kinds/contracts.toml"))
        .expect("read kind");
    body.push_str(
        r#"

[index]
file = "contracts/README.md"
columns = ["file", "title"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/contracts.toml"), body).expect("written");
    write(
        &base,
        "contracts/README.md",
        "# 契约\n\n| 文件 | 内容 |\n|---|---|\n",
    );

    let (code, _, stderr) = run(&base, &["contracts", "new", "--title", "第一份"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let page = read(&base, "contracts/README.md");
    assert!(
        page.contains("| [CON-01.md](CON-01.md) | 第一份 |"),
        "index: {page}"
    );

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");

    let (code, _, stderr) = run(&base, &["contracts", "new", "--title", "第二份"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let page = read(&base, "contracts/README.md");
    assert!(page.contains("CON-01.md"), "index: {page}");
    assert!(page.contains("CON-02.md"), "index: {page}");
}

#[test]
fn check_reports_index_mismatches() {
    let base = root("index-check");
    let mut body = std::fs::read_to_string(base.join(".clerk/config/kinds/contracts.toml"))
        .expect("read kind");
    body.push_str(
        r#"

[index]
file = "contracts/README.md"
columns = ["file", "title"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/contracts.toml"), body).expect("written");
    write(
        &base,
        "contracts/README.md",
        "# Contracts\n\n| File | Title |\n|---|---|\n",
    );
    let (code, stdout, stderr) = run(&base, &["contracts", "new", "--title", "第一份"]);
    assert_eq!(code, 0, "{stdout} {stderr}");

    // A row without a file, and a file without a row.
    write(
        &base,
        "contracts/README.md",
        "# 契约\n\n| 文件 | 内容 |\n|---|---|\n| [CON-09.md](CON-09.md) | 缺失 |\n",
    );
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("not a document"), "stdout: {stdout}");
    assert!(stdout.contains("no index row"), "stdout: {stdout}");
}

#[test]
fn archiving_a_document_drops_its_index_row() {
    let base = root("index-archive");
    let mut body = std::fs::read_to_string(base.join(".clerk/config/kinds/contracts.toml"))
        .expect("read kind");
    body.push_str(
        r#"

[index]
file = "contracts/README.md"
columns = ["file", "title"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/contracts.toml"), body).expect("written");
    write(
        &base,
        "contracts/README.md",
        "# 契约\n\n| 文件 | 内容 |\n|---|---|\n",
    );
    run(&base, &["contracts", "new", "--title", "第一份"]);
    run(&base, &["contracts", "start", "CON-01"]);
    run(&base, &["contracts", "finish", "CON-01"]);

    let (code, _, stderr) = run(&base, &["contracts", "archive", "CON-01"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let page = read(&base, "contracts/README.md");
    assert!(!page.contains("CON-01.md"), "index: {page}");
}

#[test]
fn lines_renderer_expands_a_template() {
    let base = root("lines");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body.push_str(
        r#"

[[view]]
name = "lines"
renderer = "lines"
target = "status.md"
region = "tasks"
template = "- {id} {state} {title}"
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    write(
        &base,
        "status.md",
        "# 现状\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );
    run(&base, &["tasks", "new", "--title", "任务"]);

    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let page = read(&base, "status.md");
    assert!(page.contains("- APP-001 todo 任务"), "page: {page}");
}

#[test]
fn table_renderer_can_keep_raw_keys() {
    let base = root("raw-keys");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body.push_str(
        r#"

[[view]]
name = "raw"
renderer = "table"
target = "status.md"
region = "tasks"
display = false
columns = ["id", "state"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    write(
        &base,
        "status.md",
        "# 现状\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );
    run(&base, &["tasks", "new", "--title", "任务"]);

    let (code, _, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let page = read(&base, "status.md");
    assert!(flat(&page).contains("| todo |"), "page: {page}");
    assert!(!page.contains("待开始"), "page: {page}");
}

#[test]
fn composite_view_writes_every_fragment() {
    let base = root("composite-fragments");
    write(&base, ".clerk/config/registry/focus.csv", "key\nAPP-001\n");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "new", "--title", "乙"]);
    run(
        &base,
        &["tasks", "link", "APP-001", "blocked", "tasks", "APP-002"],
    );
    run(
        &base,
        &[
            "tasks",
            "set",
            "APP-001",
            "--evidence",
            "tasks/APP-001-done.md",
        ],
    );

    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body.push_str(
        r#"

[[view]]
name = "frag-registry"
renderer = "registry"
title = "状态取值"
source = "states"
 columns = ["key", "display"]
 headers = ["键", "显示名"]

[[view]]
name = "frag-batch"
renderer = "focus"
title = "当前批次"
source = ".clerk/config/registry/focus.csv"
columns = ["id", "title"]
headers = ["编号", "一句话"]

[[view]]
name = "frag-list"
renderer = "table"
title = "清单"
display = false
columns = ["id", "title", "state"]
headers = ["编号", "标题", "状态"]

[[view]]
name = "frag-details"
renderer = "details"
title = "详情"
columns = ["evidence"]

[[view]]
name = "frag-relations"
renderer = "filetable"
title = "关联"
source = ".clerk/data/relations.csv"
columns = ["from", "kind", "to"]
headers = ["从", "关系", "到"]

[[view]]
name = "tasks"
renderer = "composite"
target = "status.md"
region = "tasks"
fragments = ["frag-registry", "frag-batch", "frag-list", "frag-details", "frag-relations"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    write(
        &base,
        "status.md",
        "# 现状\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );

    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let page = read(&base, "status.md");

    assert!(page.contains("### 状态取值"), "page: {page}");
    assert!(page.contains("| todo | 待开始 |"), "page: {page}");
    assert!(page.contains("### 当前批次"), "page: {page}");
    assert!(flat(&page).contains("| APP-001 | 甲 |"), "page: {page}");
    assert!(page.contains("### 清单"), "page: {page}");
    assert!(
        flat(&page).contains("| APP-002 | 乙 | todo |"),
        "page: {page}"
    );
    assert!(page.contains("### 详情"), "page: {page}");
    assert!(
        page.contains("evidence: [APP-001-done.md](tasks/APP-001-done.md)"),
        "page: {page}"
    );
    assert!(page.contains("### 关联"), "page: {page}");
    assert!(
        page.contains("| APP-001 | blocked | APP-002 |"),
        "page: {page}"
    );
}

fn review_with_template(base: &Path) {
    let body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/reviews.toml")).expect("read kind");
    // Root keys must come before the first array of tables.
    let insert = "template = \"reviews/skeleton.md\"\n\n[sections]\nrequired = [\"结论\", \"依据\"]\n\n[metadata]\nrequired = [\"scope\"]\nstrict = true\n\n";
    let at = ["[[field]]", "[[view]]", "[[transition]]"]
        .iter()
        .filter_map(|token| body.find(token))
        .min();
    let body = match at {
        Some(at) => format!("{}{}{}", &body[..at], insert, &body[at..]),
        None => format!("{body}\n{insert}"),
    };
    std::fs::write(base.join(".clerk/config/kinds/reviews.toml"), body).expect("written");
    write(
        base,
        "reviews/skeleton.md",
        "## 一、结论\n\n待填。\n\n## 二、依据\n\n待填。\n\n范围：{scope}\n",
    );
}

#[test]
fn documents_are_created_from_a_template() {
    let base = root("template");
    review_with_template(&base);
    let (code, stdout, stderr) = run(
        &base,
        &[
            "reviews",
            "new",
            "--date",
            "20261031",
            "--time",
            "101112",
            "--slug",
            "reservation",
            "--title",
            "预约应用复核",
            "--scope",
            "预约应用",
        ],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");

    let page = read(&base, "reviews/20261031-101112-reservation.md");
    assert!(page.contains("## 一、结论"), "page: {page}");
    assert!(page.contains("## 二、依据"), "page: {page}");
    assert!(
        page.contains("范围：预约应用"),
        "placeholder must expand: {page}"
    );

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");
}

#[test]
fn check_reports_a_missing_section() {
    let base = root("section-missing");
    review_with_template(&base);
    run(
        &base,
        &[
            "reviews",
            "new",
            "--date",
            "20261031",
            "--time",
            "101112",
            "--slug",
            "reservation",
            "--title",
            "预约应用复核",
            "--scope",
            "预约应用",
        ],
    );
    let path = base.join("reviews/20261031-101112-reservation.md");
    let body = std::fs::read_to_string(&path)
        .expect("read")
        .replace("## 二、依据", "## 二、说明");
    std::fs::write(&path, body).expect("written");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("依据"), "stdout: {stdout}");
}

#[test]
fn check_reports_a_broken_link_and_missing_metadata() {
    let base = root("doc-links");
    review_with_template(&base);
    run(
        &base,
        &[
            "reviews",
            "new",
            "--date",
            "20261031",
            "--time",
            "101112",
            "--slug",
            "reservation",
            "--title",
            "预约应用复核",
            "--scope",
            "预约应用",
        ],
    );
    let path = base.join("reviews/20261031-101112-reservation.md");
    let body = std::fs::read_to_string(&path)
        .expect("read")
        .replace("待填。", "见 [证据](missing.md)。");
    std::fs::write(&path, body).expect("written");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("does not resolve"), "stdout: {stdout}");

    // Clearing a required metadata value is reported too.
    run(
        &base,
        &["reviews", "set", "20261031", "101112", "--scope", ""],
    );
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("metadata `scope`"), "stdout: {stdout}");
}

#[test]
fn identity_action_fills_and_clears_fields() {
    let base = root("action");
    run(&base, &["tasks", "new", "--title", "认领"]);

    let (code, stdout, stderr) = run(&base, &["tasks", "claim", "APP-001"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(
        table.contains("tester"),
        "claimant from git identity: {table}"
    );
    assert!(table.contains("tester@example.com"), "{table}");

    let (code, _, stderr) = run(&base, &["tasks", "claim", "APP-001"]);
    assert_eq!(code, 4, "a second claim must conflict: {stderr}");
    assert!(stderr.contains("claimant"), "stderr: {stderr}");

    let (code, _, stderr) = run(&base, &["tasks", "release", "APP-001"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let (_, stdout, _) = run(&base, &["tasks", "show", "APP-001"]);
    assert!(stdout.contains("claimant"), "{stdout}");
    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(!table.contains("tester@example.com"), "cleared: {table}");
}

#[test]
fn action_respects_the_declared_states() {
    let base = root("action-states");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body = body.replace(
        "name = \"claim\"\nhandler = \"identity\"",
        "name = \"claim\"\nhandler = \"identity\"\nstates = [\"active\"]",
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");

    run(&base, &["tasks", "new", "--title", "认领"]);
    let (code, _, stderr) = run(&base, &["tasks", "claim", "APP-001"]);
    assert_eq!(code, 4, "state guard must refuse: {stderr}");
    assert!(stderr.contains("todo"), "stderr: {stderr}");

    run(&base, &["tasks", "start", "APP-001"]);
    let (code, _, stderr) = run(&base, &["tasks", "claim", "APP-001"]);
    assert_eq!(code, 0, "stderr: {stderr}");
}

#[test]
fn log_appends_entries_to_one_day_file() {
    let base = root("log-add");
    initial_commit(&base);
    let (code, stdout, stderr) = run(
        &base,
        &[
            "logs",
            "add",
            "--side",
            "tester",
            "--title",
            "第一条",
            "--body",
            "正文一",
        ],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let (code, _, stderr) = run(
        &base,
        &[
            "logs",
            "add",
            "--side",
            "tester",
            "--title",
            "第二条",
            "--body",
            "正文二",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let files: Vec<_> = std::fs::read_dir(base.join("log/tester"))
        .expect("dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(files.len(), 1, "one file per day: {files:?}");

    let page = read(&base, &format!("log/tester/{}", files[0]));
    assert!(page.contains("tester log"), "{page}");
    assert!(page.contains("maintainer: tester"), "{page}");
    assert!(page.contains("## [T"), "{page}");
    assert!(page.contains("第一条"), "{page}");
    assert!(page.contains("第二条"), "{page}");
    assert!(page.contains("正文二"), "{page}");

    let (code, stdout, _) = run(&base, &["logs", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("tester"), "{stdout}");
    assert!(stdout.contains("2026"), "{stdout}");
}

#[test]
fn log_archive_moves_old_files() {
    let base = root("log-archive");
    initial_commit(&base);
    run(
        &base,
        &[
            "logs", "add", "--side", "tester", "--title", "今天", "--body", "x",
        ],
    );
    // A file dated long ago is moved by the archive command.
    write(
        &base,
        "log/tester/20200101.md",
        "# [20200101] tester 工作记录\n",
    );
    let (code, _, stderr) = run(&base, &["logs", "prune", "--before", "20210101"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(base.join("log/archive/tester/20200101.md").exists());
    assert!(!base.join("log/tester/20200101.md").exists());
}

#[test]
fn attach_action_writes_the_file_and_records_the_path() {
    let base = root("attach");
    run(&base, &["tasks", "new", "--title", "附件"]);

    let (code, stdout, stderr) = run(
        &base,
        &["tasks", "brief", "APP-001", "--body", "任务说明正文"],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let page = read(&base, "tasks/APP-001.md");
    assert_eq!(page, "任务说明正文");

    let (code, _, stderr) = run(&base, &["tasks", "finish", "APP-001", "--body", "完成说明"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(read(&base, "tasks/APP-001-done.md"), "完成说明");

    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(table.contains("tasks/APP-001.md"), "{table}");
    assert!(table.contains("tasks/APP-001-done.md"), "{table}");

    let (code, stdout, _) = run(&base, &["tasks", "show", "APP-001"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("tasks/APP-001.md"), "{stdout}");
}

#[test]
fn attach_action_reads_a_file() {
    let base = root("attach-file");
    run(&base, &["tasks", "new", "--title", "附件"]);
    let source = base.join("draft.md");
    std::fs::write(&source, "来自文件\n").expect("written");

    let (code, _, stderr) = run(
        &base,
        &[
            "tasks",
            "brief",
            "APP-001",
            "--file",
            source.to_str().expect("path"),
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(read(&base, "tasks/APP-001.md"), "来自文件\n");
}

#[test]
fn seal_rewrites_the_baseline_and_notes_the_reason() {
    let base = root("seal-command");
    run(&base, &["tasks", "new", "--title", "任务"]);
    // A manual edit breaks the seal until it is re-sealed.
    let table = read(&base, ".clerk/data/tasks.csv").replace("任务", "手改");
    write(&base, ".clerk/data/tasks.csv", &table);
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");

    let (code, stdout, stderr) = run(&base, &["seal", "--note", "手工修正"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");

    let log = read(&base, "log/seal-log.md");
    assert!(log.contains("手工修正"), "{log}");
}

#[test]
fn seal_requires_a_note() {
    let base = root("seal-note");
    let (code, _, stderr) = run(&base, &["seal"]);
    assert_eq!(code, 2, "stderr: {stderr}");
    assert!(stderr.contains("--note"), "stderr: {stderr}");
}

#[test]
fn delete_removes_a_row_and_a_page() {
    let base = root("delete");
    run(&base, &["tasks", "new", "--title", "删除我"]);
    let (code, _, stderr) = run(&base, &["tasks", "delete", "APP-001"]);
    assert_eq!(code, 2, "without --force it must refuse: {stderr}");
    assert!(stderr.contains("--force"), "stderr: {stderr}");

    let (code, _, stderr) = run(&base, &["tasks", "delete", "APP-001", "--force"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!read(&base, ".clerk/data/tasks.csv").contains("APP-001"));

    run(&base, &["contracts", "new", "--title", "删掉的契约"]);
    let (code, _, stderr) = run(&base, &["contracts", "delete", "CON-01", "--force"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!base.join("contracts/CON-01.md").exists());

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0, "stdout: {stdout}");
}

#[test]
fn vocabularies_renderer_merges_sources() {
    let base = root("vocabularies");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body.push_str(
        r#"

[[view]]
name = "frag-axes"
renderer = "vocabularies"
title = "分类取值"
headers = ["分类", "可取值", "多填"]

[[view.entries]]
display = "状态"
registry = "states"
multi = true

[[view.entries]]
display = "任务"
kind = "tasks"

[[view]]
name = "frag-details"
renderer = "details"
title = "详情"
logs = true
columns = ["evidence"]

[[view]]
name = "tasks"
renderer = "composite"
target = "status.md"
region = "tasks"
fragments = ["frag-axes", "frag-details"]
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    write(
        &base,
        "status.md",
        "# 现状\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );
    run(&base, &["tasks", "new", "--title", "甲"]);
    write(
        &base,
        "log/tester/20261031.md",
        "# [20261031] tester 工作记录\n\n处理了 APP-001。\n",
    );

    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let page = read(&base, "status.md");
    assert!(page.contains("### 分类取值"), "page: {page}");
    assert!(
        page.contains("| 状态 | todo, active, done | yes |"),
        "page: {page}"
    );
    assert!(page.contains("| 任务 | APP-001 | no |"), "page: {page}");
    assert!(
        page.contains("logs: log/tester/20261031.md"),
        "page: {page}"
    );
}

#[test]
fn init_creates_the_workspace_skeleton() {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("init");
    if base.exists() {
        std::fs::remove_dir_all(&base).expect("cleaned");
    }
    std::fs::create_dir_all(base.join(".clerk")).expect("created");
    let output = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .arg("--root")
        .arg(&base)
        .args(["init"])
        .output()
        .expect("ran");
    // Without a marker the workspace cannot be opened, so init reports it.
    assert!(!output.status.success());

    std::fs::write(base.join(".clerk/workspace.toml"), "version = 1\n").expect("marker");
    let output = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .arg("--root")
        .arg(&base)
        .args(["init"])
        .output()
        .expect("ran");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(base.join(".clerk/config/kinds").is_dir());
    assert!(base.join(".clerk/data").is_dir());
    assert!(base.join("status.md").exists());
}

#[test]
fn place_and_help_describe_the_workspace() {
    let base = root("place");
    write(
        &base,
        ".clerk/config/registry/category.csv",
        "key,display,path\narchitecture,架构,architecture\n",
    );
    let (code, stdout, _) = run(&base, &["place", "architecture"]);
    assert_eq!(code, 0);
    assert!(stdout.trim() == "architecture", "{stdout}");

    let (code, _, stderr) = run(&base, &["place", "missing"]);
    assert_eq!(code, 2, "stderr: {stderr}");

    let (code, stdout, _) = run(&base, &["help", "tasks"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("complete"), "{stdout}");
    assert!(stdout.contains("claim"), "{stdout}");

    let (code, stdout, _) = run(&base, &["completions", "bash"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("clerk"), "{stdout}");
}

#[test]
fn people_and_batch_round_trip() {
    let base = root("people");
    let (code, _, stderr) = run(
        &base,
        &[
            "people",
            "add",
            "--person",
            "alice",
            "--email",
            "alice@example.com",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let (code, stdout, _) = run(&base, &["people", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("alice@example.com"), "{stdout}");

    let (code, _, stderr) = run(
        &base,
        &[
            "people",
            "add",
            "--person",
            "alice",
            "--email",
            "alice@example.com",
        ],
    );
    assert_eq!(code, 4, "duplicate must conflict: {stderr}");

    let (code, _, stderr) = run(&base, &["people", "remove", "--email", "alice@example.com"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!read(&base, ".clerk/config/registry/person.csv").contains("alice"));

    assert_eq!(run(&base, &["tasks", "new", "--title", "first"]).0, 0);
    assert_eq!(run(&base, &["tasks", "new", "--title", "second"]).0, 0);
    let (code, _, stderr) = run(&base, &["focus", "set", "APP-001", "APP-002"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let (code, stdout, _) = run(&base, &["focus", "show"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-001"), "{stdout}");
    assert!(stdout.contains("APP-002"), "{stdout}");
}

#[test]
fn brief_counts_states() {
    let base = root("brief");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "new", "--title", "乙"]);
    run(&base, &["tasks", "start", "APP-001"]);

    let (code, stdout, _) = run(&base, &["brief"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("tasks"), "{stdout}");
    assert!(stdout.contains("待开始"), "{stdout}");
    assert!(stdout.contains("进行中"), "{stdout}");
}

#[test]
fn links_check_reports_unresolved_targets() {
    let base = root("links-check");
    write(
        &base,
        "guide/01-intro.md",
        "# 导览\n\n见 [缺失](02-missing.md)。\n",
    );
    write(
        &base,
        "guide/02-more.md",
        "# 更多\n\n见 [导览](01-intro.md)。\n",
    );

    let (code, stdout, _) = run(&base, &["links", "check", "guide"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.contains("02-missing.md"), "stdout: {stdout}");
    assert!(
        !stdout.contains("01-intro.md`"),
        "resolvable link must pass: {stdout}"
    );
}

#[test]
fn check_reports_a_broken_table_row() {
    let base = root("table-shape");
    run(&base, &["contracts", "new", "--title", "表格"]);
    let path = base.join("contracts/CON-01.md");
    let body = std::fs::read_to_string(&path).expect("read");
    std::fs::write(
        &path,
        format!("{body}\n| 列一 | 列二 |\n|---|---|\n| 只有一列 |\n"),
    )
    .expect("written");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(
        stdout.contains("cells where the header has"),
        "stdout: {stdout}"
    );
}

#[test]
fn display_is_opt_in_on_the_command_line() {
    let base = root("display-opt");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "start", "APP-001"]);

    let (code, stdout, _) = run(&base, &["tasks", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("active"), "keys by default: {stdout}");
    assert!(!stdout.contains("进行中"), "keys by default: {stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--display"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("进行中"),
        "--display shows display text: {stdout}"
    );

    let (code, stdout, _) = run(&base, &["tasks", "show", "APP-001", "--display"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("进行中"), "{stdout}");
}

#[test]
fn domain_file_carries_the_display_column() {
    let base = root("axis-display");
    write(
        &base,
        ".clerk/config/registry/axes.csv",
        "axis,values,multi\ntype,ability|defect,true\n",
    );
    write(
        &base,
        ".clerk/config/registry/axes_display.csv",
        "axis,key,display\ntype,ability,能力\ntype,defect,缺陷\nrepo,code,预约应用\n",
    );
    write(
        &base,
        ".clerk/config/kinds/tickets.toml",
        r#"title = "工单"
table = ".clerk/data/tickets.csv"
key = ["id"]

[[field]]
name = "id"
type = "text"

[[field]]
name = "title"
type = "text"

[[field]]
name = "kind"
type = "enum"
registry = "type"
display = "类型"

[[view]]
name = "list"
columns = ["id", "kind"]
"#,
    );
    write(
        &base,
        ".clerk/data/tickets.csv",
        "id,title,kind\nT-1,甲,ability\n",
    );

    let (code, stdout, _) = run(&base, &["tickets", "list"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("ability"), "keys by default: {stdout}");

    let (code, stdout, _) = run(&base, &["tickets", "list", "--display"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("能力"),
        "companion display text applies: {stdout}"
    );
    assert!(stdout.contains("类型"), "{stdout}");
}

#[test]
fn body_is_read_from_a_pipe_when_no_flag_is_given() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let base = root("stdin-body");
    run(&base, &["tasks", "new", "--title", "管道"]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .arg("--root")
        .arg(&base)
        .args(["tasks", "brief", "APP-001"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawned");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all("来自管道的说明\n".as_bytes())
        .expect("written");
    let output = child.wait_with_output().expect("ran");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(read(&base, "tasks/APP-001.md"), "来自管道的说明\n");
}

#[test]
fn log_takes_its_body_from_a_pipe() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let base = root("stdin-log");
    initial_commit(&base);
    let mut child = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .arg("--root")
        .arg(&base)
        .args(["logs", "add", "--side", "tester", "--title", "记录"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawned");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all("管道正文\n".as_bytes())
        .expect("written");
    let output = child.wait_with_output().expect("ran");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let files: Vec<String> = std::fs::read_dir(base.join("log/tester"))
        .expect("dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    let page = read(&base, &format!("log/tester/{}", files[0]));
    assert!(page.contains("管道正文"), "{page}");
}

#[test]
fn list_sorts_and_prints_long() {
    let base = root("sort-long");
    run(&base, &["tasks", "new", "--title", "乙"]);
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "start", "APP-002"]);

    let (code, stdout, _) = run(&base, &["tasks", "list", "--sort", "title"]);
    assert_eq!(code, 0, "{stdout}");
    let first = stdout.lines().nth(2).unwrap_or_default();
    assert!(first.contains("APP-001"), "sorted by title: {stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--sort", "title", "--desc"]);
    assert_eq!(code, 0, "{stdout}");
    let first = stdout.lines().nth(2).unwrap_or_default();
    assert!(first.contains("APP-002"), "reversed: {stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "--sort", "missing"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("no column"), "{stderr}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--long"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("title"), "{stdout}");
    assert!(stdout.contains("甲"), "{stdout}");
}

#[test]
fn list_filters_by_claim() {
    let base = root("mine-claimed");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "new", "--title", "乙"]);
    run(&base, &["tasks", "claim", "APP-001"]);

    let (code, stdout, _) = run(&base, &["tasks", "list", "--claimed"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-001"), "{stdout}");
    assert!(!stdout.contains("APP-002"), "{stdout}");

    run(&base, &["tasks", "claim", "APP-002"]);
    let (code2, stdout, _) = run(&base, &["tasks", "list", "--mine"]);
    assert_eq!(code2, 0);
    assert!(stdout.contains("APP-001"), "mine sees own claims: {stdout}");
}

#[test]
fn commands_and_verbs_take_a_unique_prefix() {
    let base = root("prefix");
    run(&base, &["tasks", "new", "--title", "甲"]);

    let (code, stdout, _) = run(&base, &["tas", "lis"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("APP-001"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "sta", "APP-001"]);
    assert_eq!(code, 0, "{stdout}");

    let (code, _, stderr) = run(&base, &["s", "list"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("ambiguous"), "{stderr}");
    assert!(stderr.contains("scope"), "{stderr}");

    let (code, _, stderr) = run(&base, &["nosuch"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown command"), "{stderr}");
}

#[test]
fn built_in_subcommands_take_a_prefix() {
    let base = root("prefix-sub");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(
        &base,
        &[
            "logs", "add", "--side", "tester", "--title", "记录", "--body", "正文",
        ],
    );

    let (code, stdout, _) = run(&base, &["logs", "lis"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("tester"), "{stdout}");
}

#[test]
fn help_pages_come_from_the_declaration() {
    let base = root("help");
    let (code, stdout, _) = run(&base, &["help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Workflows:"), "{stdout}");
    assert!(stdout.contains("tasks"), "{stdout}");

    let (code, stdout, _) = run(&base, &["help", "tasks"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Verbs:"), "{stdout}");
    assert!(stdout.contains("start"), "{stdout}");
    assert!(stdout.contains("Fields:"), "{stdout}");
    assert!(stdout.contains("title"), "{stdout}");

    let (code, stdout, _) = run(&base, &["help", "logs"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Actions:"), "{stdout}");
    assert!(stdout.contains("append"), "{stdout}");

    let (code, _, stderr) = run(&base, &["help", "nosuch"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("no page"), "{stderr}");
}

#[test]
fn view_options_are_checked_against_their_renderer() {
    let base = root("view-options");
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body.push_str(
        r#"

[[view]]
name = "bad"
renderer = "table"
logs = true
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");

    let (code, _, stderr) = run(&base, &["kinds"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("logs"), "{stderr}");
    assert!(stderr.contains("allowed"), "{stderr}");

    let mut unknown =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    unknown = unknown.replace("renderer = \"table\"", "renderer = \"nonsense\"");
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), unknown).expect("written");
    let (code, _, stderr) = run(&base, &["kinds"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown renderer"), "{stderr}");
}

#[test]
fn built_in_json_uses_the_shared_envelope() {
    let base = root("json-shape");
    run(&base, &["tasks", "new", "--title", "甲"]);

    let (code, stdout, _) = run(&base, &["kinds", "--json"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"columns\""), "{stdout}");
    assert!(stdout.contains("\"data\""), "{stdout}");
    assert!(stdout.contains("tasks"), "{stdout}");

    let (code, stdout, _) = run(&base, &["whoami", "--json"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("\"columns\":[\"field\",\"value\"]"),
        "{stdout}"
    );
    assert!(stdout.contains("\"count\":5"), "{stdout}");

    let (code, stdout, _) = run(&base, &["people", "list", "--json"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"columns\""), "{stdout}");
}

#[test]
fn document_kind_reads_subdirectories() {
    let base = root("nested-docs");
    write(
        &base,
        ".clerk/config/kinds/parts.toml",
        r#"title = "部件"
storage = "document"
dir = "parts"
filename = "{group}/{id}.md"
key = ["group", "id"]

[[field]]
name = "group"
type = "text"
write = "create"

[[field]]
name = "id"
type = "text"
write = "create"

[[field]]
name = "title"
type = "text"
write = "create"

[[view]]
name = "list"
columns = ["group", "id", "title"]
"#,
    );
    write(&base, "parts/core/engine.md", "# 引擎\n\n正文。\n");
    write(&base, "parts/ui/hud.md", "# 面板\n\n正文。\n");

    let (code, stdout, _) = run(&base, &["parts", "list"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("engine"), "{stdout}");
    assert!(stdout.contains("hud"), "{stdout}");
    assert!(stdout.contains("core"), "{stdout}");
}

#[test]
fn lines_renderer_can_add_a_detail_line() {
    let base = root("detail-line");
    run(&base, &["tasks", "new", "--title", "甲"]);
    let mut body =
        std::fs::read_to_string(base.join(".clerk/config/kinds/tasks.toml")).expect("read kind");
    body.push_str(
        r#"

[[field]]
name = "note"
type = "text"
display = "详情"

[[view]]
name = "lines"
renderer = "lines"
target = "status.md"
region = "tasks"
template = "- {id} [{state}] {title}"
detail_field = "note"
"#,
    );
    std::fs::write(base.join(".clerk/config/kinds/tasks.toml"), body).expect("written");
    write(
        &base,
        "status.md",
        "# 现状\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );
    run(&base, &["tasks", "set", "APP-001", "--note", "补充说明"]);

    let (code, stdout, stderr) = run(&base, &["render"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let page = read(&base, "status.md");
    assert!(page.contains("- APP-001 [todo] 甲"), "{page}");
    assert!(page.contains("  详情: 补充说明"), "{page}");
}

#[test]
fn a_view_orders_by_type_then_identifier_parts() {
    let base = root("view-sort");
    write(
        &base,
        ".clerk/config/registry/prefixes.csv",
        "key,display\nAPP,预约应用\nAPI,预约接口\n",
    );
    write(
        &base,
        ".clerk/config/registry/type.csv",
        "key,display,order\nability,能力,2\ndefect,缺陷,1\n",
    );
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        r#"title = "任务"
table = ".clerk/data/tasks.csv"
key = ["id"]
numbering = "tasks"

[[field]]
name = "id"
type = "text"
role = "status"

[[field]]
name = "title"
type = "text"

[[field]]
name = "types"
type = "enum"
registry = "type"
multi = true

[[field]]
name = "state"
type = "text"
role = "status"

[[view]]
name = "list"
columns = ["id", "title", "types", "state"]
sort = ["types", "id:prefix", "id:serial"]
"#,
    );
    write(
        &base,
        ".clerk/data/tasks.csv",
        "id,title,types,state\nAPI-002,乙,defect,done\nAPP-002,丁,ability,done\nAPI-001,丙,defect,done\nAPP-001,甲,ability,done\n",
    );

    let (code, stdout, stderr) = run(&base, &["tasks", "list"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    // defect is order 1 and ability is order 2, so the defect rows come first
    // even though their keys sort later.
    let order: Vec<usize> = ["API-001", "API-002", "APP-001", "APP-002"]
        .iter()
        .map(|id| {
            stdout
                .find(id)
                .unwrap_or_else(|| panic!("missing {id}: {stdout}"))
        })
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{stdout}");
}

/// Collapse runs of spaces so an assertion does not depend on cell padding.
fn flat(text: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for character in text.chars() {
        if character == ' ' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(character);
            space = false;
        }
    }
    out
}

#[test]
fn the_sort_flag_takes_several_keys() {
    let base = root("sort-flag");
    write(
        &base,
        ".clerk/config/registry/prefixes.csv",
        "key,display\nAPP,预约应用\nAPI,预约接口\n",
    );
    write(
        &base,
        ".clerk/data/tasks.csv",
        "id,title,state\nAPP-002,乙,todo\nAPI-001,丙,done\nAPP-001,甲,todo\nAPI-002,丁,done\n",
    );

    let (code, stdout, stderr) = run(&base, &["tasks", "list", "--sort", "id:prefix,id:serial"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let order: Vec<usize> = ["API-001", "API-002", "APP-001", "APP-002"]
        .iter()
        .map(|id| {
            stdout
                .find(id)
                .unwrap_or_else(|| panic!("missing {id}: {stdout}"))
        })
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "--sort", "nosuch"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("no column"), "{stderr}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "--sort", "id:nosuch"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown part"), "{stderr}");
}

#[test]
fn check_reports_duplicate_keys_once_per_storage() {
    let base = root("duplicate-keys");
    write(
        &base,
        ".clerk/data/tasks.csv",
        "id,title,state\nAPP-001,甲,todo\nAPP-001,乙,todo\n",
    );

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(
        stdout.contains("key `APP-001` is also used at .clerk/data/tasks.csv:2"),
        "{stdout}"
    );
}

#[test]
fn help_reaches_a_kind_page_from_every_spelling() {
    let base = root("help-routing");

    for argv in [
        vec!["help", "tasks"],
        vec!["tasks", "--help"],
        vec!["tasks", "-h"],
        vec!["tas", "--help"],
    ] {
        let (code, stdout, stderr) = run(&base, &argv);
        assert_eq!(code, 0, "{argv:?} stderr: {stderr}");
        assert!(stdout.starts_with("clerk tasks - "), "{argv:?}: {stdout}");
        assert!(stdout.contains("Views:"), "{argv:?}: {stdout}");
        assert!(stdout.contains("Options:"), "{argv:?}: {stdout}");
    }

    for argv in [vec!["tas", "lis", "--help"], vec!["help", "tas", "lis"]] {
        let (code, out, err) = run(&base, &argv);
        assert_eq!(code, 0, "{err}");
        assert!(
            out.starts_with("clerk tasks list")
                && out.contains("Data scope:")
                && out.contains("Reading:"),
            "{out}"
        );
        assert!(!out.contains("--message"));
    }
    let (code, _, stderr) = run(&base, &["nosuch", "--help"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("no page for `nosuch`"), "{stderr}");
}

#[test]
fn an_unknown_option_is_rejected() {
    let base = root("unknown-option");
    let (code, _, stderr) = run(&base, &["tasks", "list", "--nosuch"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown option `--nosuch`"), "{stderr}");
}

#[test]
fn bulk_new_markdown_output_and_quiet_mode() {
    let base = root("bulk-and-formats");
    write(&base, "focus.csv", "title,state\n甲,todo\n乙,todo\n");
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--from", "focus.csv"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    assert!(stdout.contains("APP-001"), "{stdout}");
    assert!(stdout.contains("APP-002"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--md"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("| id |"), "{stdout}");
    assert!(stdout.contains("| APP-001 |"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--quiet"]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "{stdout}");
}

#[test]
fn sorting_by_a_column_keeps_identifier_order_inside_groups() {
    let base = root("sort-by-column");
    write(
        &base,
        ".clerk/config/registry/repos.csv",
        "key,display\ncode,代码\ndesign,设计\n",
    );
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        r#"title = "任务"
description = "测试用任务"
table = ".clerk/data/tasks.csv"
key = ["id"]
numbering = "tasks"

[[field]]
name = "id"
type = "text"
role = "status"

[[field]]
name = "title"
type = "text"

[[field]]
name = "repos"
type = "enum"
registry = "repos"
multi = true

[[view]]
name = "list"
columns = ["id", "title", "repos"]
"#,
    );
    write(
        &base,
        ".clerk/data/tasks.csv",
        "id,title,repos\nAPP-002,乙,design\nAPP-001,甲,code\nAPP-003,丙,code\n",
    );

    let (code, stdout, stderr) = run(&base, &["tasks", "list", "--sort", "repos"]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let order: Vec<usize> = ["APP-001", "APP-003", "APP-002"]
        .iter()
        .map(|id| {
            stdout
                .find(id)
                .unwrap_or_else(|| panic!("missing {id}: {stdout}"))
        })
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{stdout}");
}

#[test]
fn list_filter_expressions_combine_conditions() {
    let base = root("filter-forms");
    run(&base, &["tasks", "new", "--title", "预约采样"]);
    run(&base, &["tasks", "new", "--title", "网格调度"]);
    run(&base, &["tasks", "new", "--title", "库存合成"]);
    run(&base, &["tasks", "start", "APP-002"]);

    let (code, stdout, stderr) = run(
        &base,
        &["tasks", "list", "--filter", "state in (todo, active)"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("APP-001"), "{stdout}");
    assert!(stdout.contains("APP-003"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--filter", "state != todo"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-002"), "{stdout}");
    assert!(!stdout.contains("APP-001"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--filter", "title ~ 调度"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-002"), "{stdout}");
    assert!(!stdout.contains("APP-001"), "{stdout}");

    let (code, stdout, _) = run(
        &base,
        &[
            "tasks",
            "list",
            "--filter",
            "(state = todo or state = active) and title ~ 调度",
        ],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-002"), "{stdout}");
    assert!(!stdout.contains("APP-003"), "{stdout}");

    let (code, stdout, _) = run(
        &base,
        &[
            "tasks",
            "list",
            "--filter",
            "updated >= 20000101 and updated <= 29991231",
        ],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("APP-001"), "{stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "--filter", "nofield = 1"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("has no field"), "{stderr}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "--filter", "state ="]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("expected a value"), "{stderr}");
}

#[test]
fn short_flags_match_their_long_names() {
    let base = root("short-flags");
    run(&base, &["tasks", "new", "--title", "甲"]);
    run(&base, &["tasks", "new", "--title", "乙"]);

    let (code, long, _) = run(&base, &["tasks", "list", "--json"]);
    assert_eq!(code, 0);
    let (code, short, stderr) = run(&base, &["tasks", "list", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(long, short);
    let removed = run(&base, &["tasks", "list", "-j"]);
    assert_eq!(removed.0, 2);
    assert!(removed.2.contains("unknown option `-j`"));

    let (code, stdout, _) = run(&base, &["tasks", "list", "-rq"]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "clustered quiet still printed: {stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "-s", "title", "-r"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("乙"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "-f", "title ~ 甲"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("甲"), "{stdout}");
    assert!(!stdout.contains("乙"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "-f=title ~ 甲"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("甲"), "{stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "-rs", "title"]);
    assert_eq!(code, 0, "{stderr}");

    let (code, _, stderr) = run(&base, &["tasks", "list", "-z"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown option `-z`"), "{stderr}");
}

#[test]
fn whoami_maps_the_git_email_to_a_person() {
    let base = root("whoami-person");
    write(
        &base,
        ".clerk/config/registry/person.csv",
        "person,email,name\ntester-person,tester@example.com,tester\n",
    );

    let (code, stdout, stderr) = run(&base, &["whoami", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"person\""), "{stdout}");
    assert!(stdout.contains("tester-person"), "{stdout}");
}

#[test]
fn check_reads_markdown_under_a_validated_path() {
    let base = root("validated-path");
    write(&base, "notes/broken.md", "# 笔记\n\n[缺失](missing.md)\n");
    let mut workspace =
        std::fs::read_to_string(base.join(".clerk/workspace.toml")).expect("read workspace");
    workspace.push_str("\n[[path]]\npath = \"notes\"\nlevel = \"validated\"\n");
    std::fs::write(base.join(".clerk/workspace.toml"), workspace).expect("written");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.contains("does not resolve"), "{stdout}");
}

#[test]
fn command_defaults_fill_what_the_command_line_leaves_out() {
    let base = root("command-defaults");
    let mut workspace =
        std::fs::read_to_string(base.join(".clerk/workspace.toml")).expect("read workspace");
    workspace.push_str(
        "\n[commands.\"tasks.list\"]\nstate = \"active\"\n[commands.\"tasks.new\"]\ntitle = \"默认标题\"\n[commands.\"check\"]\nquiet = true\n",
    );
    std::fs::write(base.join(".clerk/workspace.toml"), workspace).expect("written");

    run(&base, &["tasks", "new"]);
    run(&base, &["tasks", "new", "--title", "显式标题"]);
    run(&base, &["tasks", "start", "APP-001"]);

    let (code, stdout, stderr) = run(&base, &["tasks", "list"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("默认标题"), "{stdout}");
    assert!(!stdout.contains("显式标题"), "{stdout}");

    let (code, stdout, _) = run(&base, &["tasks", "list", "--state", "todo"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("显式标题"), "{stdout}");

    let (code, stdout, _) = run(&base, &["help", "tasks"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Workspace defaults:"), "{stdout}");
    assert!(stdout.contains("state=active"), "{stdout}");

    let (code, stdout, _) = run(&base, &["check", "--json"]);
    assert_eq!(code, 0);
    assert!(!stdout.is_empty(), "json still prints");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0);
    assert!(
        stdout.is_empty(),
        "quiet default hides the summary: {stdout}"
    );
}

#[test]
fn command_defaults_are_validated_at_load() {
    let base = root("command-defaults-bad");
    let mut workspace =
        std::fs::read_to_string(base.join(".clerk/workspace.toml")).expect("read workspace");
    workspace.push_str("\n[commands.\"tasks.list\"]\nbogus = 1\n");
    std::fs::write(base.join(".clerk/workspace.toml"), workspace).expect("written");

    let (code, _, stderr) = run(&base, &["check"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("unknown option or field `bogus`"),
        "{stderr}"
    );

    let base = root("command-defaults-forbidden");
    let mut workspace =
        std::fs::read_to_string(base.join(".clerk/workspace.toml")).expect("read workspace");
    workspace.push_str("\n[commands.\"tasks.delete\"]\nforce = true\n");
    std::fs::write(base.join(".clerk/workspace.toml"), workspace).expect("written");

    let (code, _, stderr) = run(&base, &["check"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("may not be a default"), "{stderr}");
}

#[test]
fn a_new_log_page_uses_the_kind_header() {
    let base = root("kind-header");
    let path = base.join(".clerk/config/kinds/logs.toml");
    write(
        &base,
        ".clerk/templates/kind-header.md",
        "# [{date}] log\nby {maintainer}",
    );
    let mut kind = std::fs::read_to_string(&path).expect("read logs kind");
    kind = kind.replacen(
        "[[field]]",
        "header = \".clerk/templates/kind-header.md\"\n\n[[field]]",
        1,
    );
    std::fs::write(&path, kind).expect("written");

    let (code, _, stderr) = run(&base, &["logs", "add", "--title", "甲"]);
    assert_eq!(code, 0, "{stderr}");
    let page = std::fs::read_dir(base.join("log"))
        .expect("log dir")
        .flatten()
        .find(|entry| entry.path().is_dir())
        .expect("maintainer dir");
    let file = std::fs::read_dir(page.path())
        .expect("maintainer dir")
        .flatten()
        .next()
        .expect("page");
    let body = std::fs::read_to_string(file.path()).expect("read page");
    assert!(body.starts_with("# "), "{body}");
    assert!(body.contains("log\nby tester"), "{body}");
}

#[test]
fn a_broken_declaration_is_reported_instead_of_an_unknown_command() {
    let base = root("broken-declaration");
    std::fs::write(
        base.join(".clerk/config/kinds/tasks.toml"),
        "title = \"任务\"\nnosuchkey = 1\n",
    )
    .expect("written");

    let (code, _, stderr) = run(&base, &["tasks", "list"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("workspace did not load"), "{stderr}");
    assert!(stderr.contains("nosuchkey"), "{stderr}");
    assert!(!stderr.contains("unknown command"), "{stderr}");
}

#[test]
fn headers_can_be_hidden_and_a_default_switch_can_be_turned_off() {
    let base = root("headers-off");
    let mut workspace =
        std::fs::read_to_string(base.join(".clerk/workspace.toml")).expect("read workspace");
    workspace.push_str(
        "\n[commands.\"whoami\"]\nheaders = \"hide\"\n[commands.\"check\"]\nquiet = true\n",
    );
    std::fs::write(base.join(".clerk/workspace.toml"), workspace).expect("written");

    let (code, stdout, _) = run(&base, &["whoami"]);
    assert_eq!(code, 0);
    assert!(!stdout.contains("field"), "{stdout}");
    assert!(stdout.contains("git name"), "{stdout}");

    let (code, stdout, _) = run(&base, &["whoami", "--headers", "show"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("field"), "{stdout}");

    let (code, stdout, _) = run(&base, &["check"]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "quiet default still printed: {stdout}");

    let (code, stdout, _) = run(&base, &["check", "--no-quiet"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("check passed"), "{stdout}");

    let (code, _, stderr) = run(&base, &["whoami", "--no-nope"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown option `--no-nope`"), "{stderr}");
}

#[test]
fn a_bare_kind_prints_a_short_guide() {
    let base = root("bare-kind");
    let (code, stdout, stderr) = run(&base, &["tasks"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Usage:") && stdout.contains("clerk tasks <verb>"),
        "{stdout}"
    );
    assert!(stdout.contains("Transitions"), "{stdout}");
    assert!(stdout.contains("Examples:"), "{stdout}");
    assert!(!stdout.contains("clerk: usage"), "{stdout}");

    let (code, _, stderr) = run(&base, &["tasks", "nosuch"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("has no verb"), "{stderr}");
}

#[test]
fn an_action_can_set_fields_and_require_others() {
    let base = root("action-set");
    let path = base.join(".clerk/config/kinds/tasks.toml");
    let mut kind = std::fs::read_to_string(&path).expect("read tasks kind");
    kind.push_str(
        "\n[[action]]\nname = \"sign\"\nhandler = \"set\"\nrequires = [\"claimant\"]\nvalues = { evidence = \"{identity}\", commit = \"0123456\" }\n",
    );
    std::fs::write(&path, kind).expect("written");

    run(&base, &["tasks", "new", "--title", "甲"]);
    let (code, _, stderr) = run(&base, &["tasks", "sign", "APP-001"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(
        stderr.contains("needs `claimant` to be filled first"),
        "{stderr}"
    );

    let (code, _, stderr) = run(&base, &["tasks", "claim", "APP-001"]);
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(&base, &["tasks", "sign", "APP-001"]);
    assert_eq!(code, 0, "{stderr}");
    let (code, stdout, _) = run(&base, &["tasks", "show", "APP-001", "--json"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"tester\""), "{stdout}");
    assert!(stdout.contains("\"0123456\""), "{stdout}");
}

#[test]
fn a_log_heading_can_come_from_a_template_file() {
    let base = root("template-heading");
    write(
        &base,
        ".clerk/templates/entry.md",
        "## [{time}] {title} (templated)\n",
    );
    let path = base.join(".clerk/config/kinds/logs.toml");
    let kind = std::fs::read_to_string(&path)
        .expect("read logs kind")
        .replace(
            "heading = \"## [T{time}] {title}\"",
            "heading = \".clerk/templates/entry.md\"",
        );
    std::fs::write(&path, kind).expect("written");

    let (code, _, stderr) = run(&base, &["logs", "add", "--title", "甲"]);
    assert_eq!(code, 0, "{stderr}");
    let page = std::fs::read_dir(base.join("log"))
        .expect("log dir")
        .flatten()
        .find(|entry| entry.path().is_dir())
        .expect("maintainer dir");
    let file = std::fs::read_dir(page.path())
        .expect("maintainer dir")
        .flatten()
        .next()
        .expect("page");
    let body = std::fs::read_to_string(file.path()).expect("read page");
    assert!(
        body.lines().any(|line| {
            line.strip_prefix("## [")
                .and_then(|rest| rest.split_once("] "))
                .is_some_and(|(time, _)| {
                    time.len() == 6 && time.bytes().all(|byte| byte.is_ascii_digit())
                })
        }),
        "{body}"
    );
    assert!(body.contains("甲 (templated)"), "{body}");
}

#[test]
fn review_archived_identifier_is_never_reused() {
    let base = root("review-id-lifetime");
    finished_task(&base);
    assert_eq!(run(&base, &["tasks", "archive", "APP-001"]).0, 0);
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "Later task"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("APP-002"), "reused archived ID: {stdout}");
}

#[test]
fn review_bulk_failure_leaves_no_partial_data() {
    let base = root("review-bulk-atomic");
    let before = read(&base, ".clerk/data/tasks.csv");
    write(&base, "input.csv", "title\nValid task\n\"\"\n");
    let (code, _, stderr) = run(&base, &["tasks", "new", "--from", "input.csv"]);
    assert_ne!(code, 0, "bad second row must fail: {stderr}");
    assert_eq!(
        read(&base, ".clerk/data/tasks.csv"),
        before,
        "failed batch changed data"
    );
}

#[test]
fn review_request_rejects_changed_target_since_baseline() {
    let base = root("review-stale-request");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Original"]).0, 0);
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "set",
                "APP-001",
                "--title",
                "Requested",
                "--request",
                "edit-title"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &["tasks", "set", "APP-001", "--title", "Concurrent edit"]
        )
        .0,
        0
    );
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "Concurrent change"]);
    let (code, stdout, stderr) = run(&base, &["request", "apply"]);
    assert_ne!(
        code, 0,
        "stale request silently overwrote current data: {stdout} {stderr}"
    );
}

#[test]
fn review_request_preserves_document_body() {
    let base = root("review-request-body");
    initial_commit(&base);
    let (code, _, stderr) = run(
        &base,
        &[
            "contracts",
            "new",
            "--title",
            "Review",
            "--body",
            "Required body text",
            "--request",
            "new-contract",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        read(&base, "contracts/CON-01.md").contains("Required body text"),
        "request discarded body"
    );
}

#[test]
fn review_request_receipt_is_committed_with_mutation() {
    let base = root("review-request-receipt");
    let config = format!(
        "{}\n[policy]\nauto_commit = true\nauto_push = false\n",
        WORKSPACE
    );
    write(&base, ".clerk/workspace.toml", &config);
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "Requested",
                "--request",
                "new-task"
            ]
        )
        .0,
        0
    );
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "Submit request"]);
    let (code, stdout, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    let status = git(&base, &["status", "--porcelain"]);
    assert!(
        status.trim().is_empty(),
        "request cleanup and receipt missed commit: {status}"
    );
}

#[test]
fn review_write_refuses_unexpected_seal_drift() {
    let base = root("review-seal-drift");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Original"]).0, 0);
    let body = read(&base, ".clerk/data/tasks.csv").replace("Original", "Manual edit");
    write(&base, ".clerk/data/tasks.csv", &body);
    assert_ne!(run(&base, &["check"]).0, 0, "drift must be detectable");
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "Another"]);
    assert_ne!(
        code, 0,
        "ordinary write accepted and resealed prior manual change: {stdout} {stderr}"
    );
}

#[test]
fn writer_lock_refuses_a_second_process_until_released() {
    let base = root("writer-lock");
    let context = clerk::execution::context::Context::open(base.to_str()).expect("context");
    let guard =
        clerk::execution::transaction::Transaction::begin(&context.workspace).expect("lock");
    let (code, _, stderr) = run(&base, &["tasks", "new", "--title", "Blocked"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("writer lock"), "{stderr}");
    drop(guard);
    assert_eq!(run(&base, &["tasks", "new", "--title", "Allowed"]).0, 0);
}

#[test]
fn interrupted_writer_child() {
    let Some(base) = std::env::var_os("CLERK_TEST_CRASH_ROOT") else {
        return;
    };
    let base = PathBuf::from(base);
    let mut context = clerk::execution::context::Context::open(base.to_str()).expect("context");
    let _guard = clerk::execution::pipeline::begin(&mut context).expect("transaction");
    let mut values = clerk::execution::engine::Values::new();
    values.insert("title".into(), "Unaccepted".into());
    clerk::execution::engine::create(&mut context, "tasks", &values, &Default::default())
        .expect("write");
    clerk::execution::transaction::write(base.join("interrupted.md"), "unfinished")
        .expect("new file");
    clerk::execution::transaction::rename(base.join("original.md"), base.join("moved.md"))
        .expect("rename");
    std::fs::write(base.join(".git/ready"), "ready").expect("ready");
    loop {
        std::thread::park();
    }
}

#[test]
fn next_writer_recovers_a_killed_process_and_releases_its_lock() {
    let base = root("process-recovery");
    write(&base, "original.md", "original body");
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "interrupted_writer_child", "--nocapture"])
        .env("CLERK_TEST_CRASH_ROOT", &base)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("child");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !base.join(".git/ready").exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("poll") {
            panic!("child exited: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = base.join(".git/ready").exists();
    child.kill().expect("kill");
    child.wait().expect("reap");
    assert!(ready, "child did not reach the crash point");
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "Accepted"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(
        stdout.contains("APP-001"),
        "unaccepted ID was not rolled back: {stdout}"
    );
    assert!(!read(&base, ".clerk/data/tasks.csv").contains("Unaccepted"));
    assert_eq!(read(&base, "original.md"), "original body");
    assert!(!base.join("moved.md").exists());
    assert!(!base.join("interrupted.md").exists());
    assert!(!base.join(".git/clerk/undo").exists());
    assert_eq!(run(&base, &["check"]).0, 0);
}

#[test]
fn failed_document_index_update_rolls_back_the_document_and_identifier() {
    let base = root("document-rollback");
    let declaration = read(&base, ".clerk/config/kinds/contracts.toml")
        + "\n[index]\nfile = \"contracts/README.md\"\ncolumns = [\"file\", \"title\"]\n";
    write(&base, ".clerk/config/kinds/contracts.toml", &declaration);
    let (code, _, _) = run(&base, &["contracts", "new", "--title", "Rejected"]);
    assert_ne!(code, 0);
    assert!(!base.join("contracts/CON-01.md").exists());
    write(
        &base,
        "contracts/README.md",
        "# Contracts\n\n| File | Title |\n|---|---|\n",
    );
    let (code, stdout, stderr) = run(&base, &["contracts", "new", "--title", "Accepted"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("CON-01"), "{stdout}");
}

#[test]
fn deleted_identifiers_remain_reserved_for_tables_and_documents() {
    let base = root("deleted-ids");
    assert_eq!(run(&base, &["tasks", "new", "--title", "First"]).0, 0);
    assert_eq!(run(&base, &["tasks", "delete", "APP-001", "--force"]).0, 0);
    assert!(
        run(&base, &["tasks", "new", "--title", "Next"])
            .1
            .contains("APP-002")
    );
    assert_eq!(run(&base, &["contracts", "new", "--title", "First"]).0, 0);
    assert_eq!(
        run(&base, &["contracts", "delete", "CON-01", "--force"]).0,
        0
    );
    assert!(
        run(&base, &["contracts", "new", "--title", "Next"])
            .1
            .contains("CON-02")
    );
}

#[test]
fn request_replay_of_the_same_content_is_refused() {
    let base = root("request-idempotence");
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "Requested",
                "--request",
                "new-task"
            ]
        )
        .0,
        0
    );
    let manifest = read(&base, ".clerk/requests/pending/tasks-new-task.toml");
    assert_eq!(run(&base, &["request", "apply"]).0, 0);
    write(
        &base,
        ".clerk/requests/pending/tasks-new-task.toml",
        &manifest,
    );
    let (code, _, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(!read(&base, ".clerk/data/tasks.csv").contains("APP-002"));
}

#[test]
fn a_failed_request_batch_rolls_back_all_prior_requests() {
    let base = root("request-batch-rollback");
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &["tasks", "new", "--title", "Valid", "--request", "a-valid"]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &["tasks", "new", "--title", "", "--request", "z-invalid"]
        )
        .0,
        0
    );
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, _, _) = run(&base, &["request", "apply"]);
    assert_ne!(code, 0);
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    assert!(
        base.join(".clerk/requests/pending/tasks-a-valid.toml")
            .exists()
    );
    assert!(
        base.join(".clerk/requests/pending/tasks-z-invalid.toml")
            .exists()
    );
    assert!(!base.join(".clerk/requests/applied.log").exists());
}

#[test]
fn request_dry_run_validates_batch_without_retaining_changes() {
    let base = root("request-batch-dry");
    initial_commit(&base);
    for name in ["a", "b"] {
        assert_eq!(
            run(&base, &["tasks", "new", "--title", name, "--request", name]).0,
            0
        );
    }
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, stdout, stderr) = run(&base, &["request", "apply", "--dry-run"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("APP-001") && stdout.contains("APP-002"),
        "{stdout}"
    );
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    assert!(!base.join(".clerk/requests/applied.log").exists());
    assert_eq!(
        std::fs::read_dir(base.join(".clerk/requests/pending"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn git_push_failure_preserves_data_and_commit_but_returns_failure() {
    let base = root("push-failure");
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[policy]\nauto_commit = true\nauto_push = true\n"),
    );
    initial_commit(&base);
    let previous = git(&base, &["rev-parse", "HEAD"]);
    let (code, _, stderr) = run(&base, &["tasks", "new", "--title", "Keep local"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("local data was saved"), "{stderr}");
    assert_ne!(git(&base, &["rev-parse", "HEAD"]), previous);
    assert!(read(&base, ".clerk/data/tasks.csv").contains("Keep local"));
    assert_eq!(run(&base, &["check"]).0, 0);
    assert!(git(&base, &["status", "--porcelain"]).trim().is_empty());
}

#[test]
fn transaction_refuses_a_write_outside_the_workspace() {
    let base = root("write-boundary");
    let context = clerk::execution::context::Context::open(base.to_str()).unwrap();
    let _guard = clerk::execution::transaction::Transaction::begin(&context.workspace).unwrap();
    let error =
        clerk::execution::transaction::write(base.join("../outside.md"), "bad").unwrap_err();
    assert!(error.message().contains("write paths"), "{error}");
}

#[test]
fn dry_run_relations_leave_data_and_seals_unchanged() {
    let base = root("dry-run-link");
    for title in ["First", "Second"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    let seal = read(&base, ".clerk/seal/seal.csv");
    let (code, _, stderr) = run(
        &base,
        &[
            "tasks",
            "link",
            "APP-001",
            "blocked",
            "tasks",
            "APP-002",
            "--dry-run",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(!base.join(".clerk/data/relations.csv").exists());
    assert_eq!(read(&base, ".clerk/seal/seal.csv"), seal);
}

#[test]
fn failed_git_commit_keeps_accepted_data_and_returns_failure() {
    let base = root("commit-failure");
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[policy]\nauto_commit = true\n"),
    );
    initial_commit(&base);
    let previous = git(&base, &["rev-parse", "HEAD"]);
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            base.join(".git/hooks/pre-commit"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let (code, _, stderr) = run(&base, &["tasks", "new", "--title", "Kept"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("local data was saved"), "{stderr}");
    assert_eq!(git(&base, &["rev-parse", "HEAD"]), previous);
    assert!(read(&base, ".clerk/data/tasks.csv").contains("Kept"));
    assert_eq!(run(&base, &["check"]).0, 0);
}

#[test]
fn request_set_allows_an_unrelated_record_to_change() {
    let base = root("request-unrelated");
    assert_eq!(run(&base, &["tasks", "new", "--title", "First"]).0, 0);
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "set",
                "APP-001",
                "--title",
                "Requested",
                "--request",
                "edit"
            ]
        )
        .0,
        0
    );
    assert_eq!(run(&base, &["tasks", "new", "--title", "Unrelated"]).0, 0);
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "Unrelated record"]);
    let (code, stdout, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    let table = read(&base, ".clerk/data/tasks.csv");
    assert!(table.contains("Requested") && table.contains("Unrelated"));
}

#[test]
fn legacy_request_manifests_remain_readable() {
    let base = root("request-legacy");
    initial_commit(&base);
    let baseline = git(&base, &["rev-parse", "HEAD"]);
    let body = format!(
        "version = 1\nkind = \"tasks\"\nverb = \"new\"\nkeys = []\nbaseline = \"{}\"\n[values]\ntitle = \"Legacy\"\n",
        baseline.trim()
    );
    write(&base, ".clerk/requests/pending/tasks-legacy.toml", &body);
    let (code, stdout, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("Legacy"));
}

#[test]
fn numbering_recovers_legacy_archived_history_without_a_ledger() {
    let base = root("legacy-numbering");
    finished_task(&base);
    assert_eq!(run(&base, &["tasks", "archive", "APP-001"]).0, 0);
    std::fs::remove_file(base.join(".clerk/seal/issued.csv")).unwrap();
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "Next"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("APP-002"), "{stdout}");
}

#[test]
fn invalid_output_options_roll_back_a_write() {
    let base = root("output-rollback");
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, _, _) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Rejected",
            "--headers",
            "invalid",
        ],
    );
    assert_ne!(code, 0);
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    assert!(!base.join(".clerk/seal/issued.csv").exists());
}

#[test]
fn render_dry_run_keeps_the_page_unchanged() {
    let base = root("render-dry");
    let declaration = read(&base, ".clerk/config/kinds/tasks.toml")
        + "\n[[view]]\nname = \"page\"\nrenderer = \"lines\"\ntarget = \"status.md\"\nregion = \"tasks\"\ntemplate = \"- {id} {title}\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    write(
        &base,
        "status.md",
        "# Status\n\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );
    assert_eq!(run(&base, &["tasks", "new", "--title", "First"]).0, 0);
    let before = read(&base, "status.md");
    let (code, stdout, stderr) = run(&base, &["render", "--dry-run"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(read(&base, "status.md"), before);
}

#[test]
fn unknown_valued_options_are_rejected_before_any_write() {
    let base = root("unknown-value");
    let before = read(&base, ".clerk/data/tasks.csv");
    for arguments in [
        vec!["tasks", "new", "--title", "Ignored", "--prefx", "APP"],
        vec!["tasks", "list", "--tytle", "Ignored"],
        vec!["whoami", "--nonesuch", "Ignored"],
        vec![
            "tasks",
            "new",
            "--title",
            "Ignored",
            "--bogus",
            "value",
            "--request",
            "bad",
        ],
    ] {
        let (code, _, stderr) = run(&base, &arguments);
        assert_eq!(code, 2, "{arguments:?}: {stderr}");
        assert!(stderr.contains("unknown option"), "{stderr}");
    }
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    assert!(!base.join(".clerk/requests/pending/tasks-bad.toml").exists());
}

#[test]
fn switch_values_and_invalid_numeric_options_are_rejected() {
    let base = root("option-values");
    for option in [
        "--quiet=false",
        "--force=true",
        "--max-width=-1",
        "--max-width=wide",
        "--headers=none",
    ] {
        let (code, _, stderr) = run(&base, &["tasks", "list", option]);
        assert_eq!(code, 2, "{option}: {stderr}");
    }
}

#[test]
fn command_default_types_are_checked_while_loading_the_workspace() {
    let cases = [
        ("quiet-string", "tasks.list", "quiet = \"true\""),
        ("quiet-integer", "tasks.list", "quiet = 1"),
        ("width-negative", "tasks.list", "max-width = -1"),
        ("headers-value", "tasks.list", "headers = \"none\""),
        ("sort-table", "tasks.list", "sort = { field = \"title\" }"),
        ("columns-mixed", "tasks.list", "columns = [\"title\", true]"),
        ("unknown-prefix", "tasks.new", "prefix = \"UNKNOWN\""),
        ("unknown-state", "tasks.list", "state = \"unknown\""),
        ("unknown-subcommand", "request.nope", "quiet = true"),
    ];
    for (name, command, value) in cases {
        let base = root(name);
        let config = format!("{WORKSPACE}\n[commands.\"{command}\"]\n{value}\n");
        write(&base, ".clerk/workspace.toml", &config);
        let (code, _, stderr) = run(&base, &["check"]);
        assert_eq!(code, 2, "{name}: {stderr}");
        assert!(
            stderr.contains(".clerk/workspace.toml") && stderr.contains("commands."),
            "{name}: {stderr}"
        );
    }
}

#[test]
fn boolean_field_defaults_are_field_values_instead_of_switches() {
    let base = root("boolean-default");
    let declaration = read(&base, ".clerk/config/kinds/tasks.toml")
        + "\n[[field]]\nname = \"enabled\"\ntype = \"bool\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    for flag in [false, true] {
        write(
            &base,
            ".clerk/workspace.toml",
            &format!("{WORKSPACE}\n[commands.\"tasks.new\"]\nenabled = {flag}\n"),
        );
        let (code, stdout, stderr) = run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "Flag",
                "--columns",
                "id,enabled",
                "--json",
            ],
        );
        assert_eq!(code, 0, "{stderr}");
        assert!(
            stdout.contains(&format!("\"enabled\":\"{flag}\"")),
            "{stdout}"
        );
    }
}

#[test]
fn an_empty_text_default_overrides_the_field_default() {
    let base = root("empty-default");
    let declaration = read(&base, ".clerk/config/kinds/tasks.toml")
        + "\n[[field]]\nname = \"memo\"\ntype = \"text\"\ndefault = \"fallback\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[commands.\"tasks.new\"]\nmemo = \"\"\n"),
    );
    let (code, stdout, stderr) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Empty",
            "--columns",
            "memo",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"memo\":\"\""), "{stdout}");
    assert!(!stdout.contains("fallback"), "{stdout}");
}

#[test]
fn numbered_and_document_inputs_are_listed_in_the_generated_help() {
    let base = root("help-options");
    let (code, stdout, stderr) = run(&base, &["contracts", "--help"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("--body") && stdout.contains("--file"),
        "{stdout}"
    );
    let (_, stdout, _) = run(&base, &["tasks", "--help"]);
    assert!(stdout.contains("--prefix <value>"), "{stdout}");
    let (_, stdout, _) = run(&base, &["logs", "--help"]);
    assert!(
        stdout.contains("--before") && stdout.contains("--side"),
        "{stdout}"
    );
}

#[test]
fn completion_uses_the_same_command_catalog_as_help() {
    let base = root("completion-catalog");
    for shell in ["bash", "zsh", "powershell"] {
        let (code, stdout, stderr) = run(&base, &["completions", shell]);
        assert_eq!(code, 0, "{stderr}");
        assert!(stdout.contains("links"), "{stdout}");
        assert!(
            !stdout.split_whitespace().any(|word| word == "log"),
            "undeclared stale command: {stdout}"
        );
    }
}

#[test]
fn version_flag_works_without_a_command() {
    let base = root("version-flag");
    let (code, stdout, stderr) = run(&base, &["--version"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("clerk 0.1.0 ("));
}

#[test]
fn version_json_is_build_identity_even_without_a_workspace() {
    let output = Command::new(env!("CARGO_BIN_EXE_clerk"))
        .args([
            "--root",
            "this-workspace-does-not-exist",
            "version",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.starts_with("{\"version\":\"0.1.0\",\"commit\":"),
        "{text}"
    );
    assert!(text.contains("\"dirty\":"));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    for format in ["workspace", "workflow", "context", "batch_draft"] {
        assert_eq!(value["formats"][format], 1);
    }
    assert_ne!(run(&root("version-extra"), &["version", "extra"]).0, 0);
}

#[test]
fn terminal_metadata_detects_a_held_dead_end_and_resume_recovers() {
    let base = root("held-recovery");
    write(
        &base,
        ".clerk/config/registry/states.csv",
        "key,display,terminal\ntodo,Todo,false\nactive,Active,false\nheld,Held,false\ndone,Done,true\n",
    );
    let declaration = format!(
        "{TASK_KIND}\n[[transition]]\nname = \"hold\"\nto = \"held\"\nfrom = [\"active\"]\n"
    );
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    let (code, out, err) = run(&base, &["check"]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        out.contains("state `held` cannot reach a terminal state"),
        "{out}{err}"
    );
    let declaration = format!(
        "{declaration}\n[[transition]]\nname = \"resume\"\nto = \"active\"\nfrom = [\"held\"]\n"
    );
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    for args in [
        vec!["tasks", "new", "--title", "recover"],
        vec!["tasks", "start", "APP-001"],
        vec!["tasks", "hold", "APP-001"],
        vec!["tasks", "resume", "APP-001"],
        vec!["check"],
    ] {
        let (code, out, err) = run(&base, &args);
        assert_eq!(code, 0, "{args:?}: {out}{err}");
    }
    assert!(read(&base, ".clerk/data/tasks.csv").contains(",active,"));
}

#[test]
fn log_entry_references_are_validated_and_collected_without_body_mentions() {
    let base = root("explicit-log-refs");
    write(
        &base,
        ".clerk/config/kinds/logs.toml",
        &LOGS_KIND.replace(
            "handler = \"append\"",
            "handler = \"append\"\nentry_target = \"tasks\"",
        ),
    );
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &format!("{TASK_KIND}\n[context]\nlogs = \"logs\"\n"),
    );
    for title in ["first", "second"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    let (code, out, err) = run(
        &base,
        &[
            "logs",
            "add",
            "--title",
            "Stage",
            "--tasks",
            "APP-001,APP-002",
            "--body",
            "Reservation implementation",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
    let pages = clerk::storage::records::markdown_files(&base.join("log")).unwrap();
    assert_eq!(pages.len(), 1);
    let body = std::fs::read_to_string(&pages[0]).unwrap();
    let entries = clerk::collaboration::log_entry::parse(&body).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].id.is_some());
    assert_eq!(entries[0].keys, ["APP-001", "APP-002"]);
    let (code, out, err) = run(&base, &["context", "tasks", "APP-002", "--json"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("log explicit reference"), "{out}");
    assert!(out.contains("Reservation implementation"));
    assert!(!out.contains("<!-- clerk:entry"));
    let (code, out, err) = run(
        &base,
        &[
            "logs",
            "add",
            "--title",
            "Bad",
            "--tasks",
            "APP-999",
            "--body",
            "must not write",
        ],
    );
    assert_ne!(code, 0, "{out}{err}");
    assert_eq!(std::fs::read_to_string(&pages[0]).unwrap(), body);
    assert_eq!(
        run(
            &base,
            &[
                "logs",
                "add",
                "--title",
                "Legacy mention",
                "--body",
                "APP-001 progressed"
            ]
        )
        .0,
        0
    );
    let (code, out, err) = run(&base, &["context", "tasks", "APP-001", "--json"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("log text mention"));
    assert!(out.contains("log explicit reference"));
    assert_eq!(run(&base, &["check"]).0, 0);
}

#[test]
fn local_sessions_warn_block_expire_and_share_scope_between_worktrees() {
    let base = root("local-session-leases");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "fixture"]);
    let path = base.join(".git/clerk/sessions/sessions.toml");
    assert_eq!(run(&base, &["session", "list", "--json"]).0, 0);
    assert!(!path.exists());
    assert_eq!(run(&base, &["tasks", "new", "--title", "scope"]).0, 0);
    let args = [
        "session",
        "open",
        "--chat",
        "chat-one",
        "--tasks",
        "APP-001",
        "--paths",
        "src/reservation",
        "--json",
    ];
    let (code, out, err) = run(&base, &args);
    assert_eq!(code, 0, "{out}{err}");
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let id = table["session"][0]["id"].as_str().unwrap().to_string();
    let before = std::fs::read_to_string(&path).unwrap();
    let (code, out, err) = run(
        &base,
        &[
            "session", "open", "--chat", "chat-two", "--paths", "src", "--strict", "--json",
        ],
    );
    assert_eq!(code, 4, "{out}{err}");
    assert!(err.contains("no session data was changed"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    let (code, out, err) = run(
        &base,
        &[
            "session", "open", "--chat", "chat-two", "--paths", "src", "--json",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains(&format!("\"overlaps\":[\"{id}\"]")));
    // Retire the warning-only session and expire the first without sleeping.
    let mut table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let rows = table["session"].as_array_mut().unwrap();
    rows[0]["expires"] = toml::Value::Integer(0);
    rows[1]["closed"] = toml::Value::Boolean(true);
    std::fs::write(&path, toml::to_string(&table).unwrap()).unwrap();
    let (code, out, err) = run(
        &base,
        &[
            "session",
            "open",
            "--chat",
            "chat-three",
            "--paths",
            "src",
            "--strict",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(run(&base, &["session", "touch", &id, "--strict"]).0, 4);
    // A related worktree sees the same active ranges despite its different branch.
    git(&base, &["add", ".clerk"]);
    git(&base, &["commit", "-qm", "task"]);
    let checkout = base.join("other-worktree");
    git(
        &base,
        &[
            "worktree",
            "add",
            "-qb",
            "other",
            checkout.to_str().unwrap(),
        ],
    );
    let (code, out, err) = run(
        &checkout,
        &[
            "session",
            "open",
            "--chat",
            "other",
            "--paths",
            "src/reservation/new.rs",
            "--strict",
        ],
    );
    assert_eq!(code, 4, "{out}{err}");
    assert_eq!(
        run(
            &base,
            &["session", "open", "--chat", "bad", "--paths", "../outside"]
        )
        .0,
        2
    );
    assert_eq!(
        run(
            &base,
            &[
                "session", "open", "--chat", "bad", "--paths", "src", "--tasks", "APP-999"
            ]
        )
        .0,
        3
    );
    assert_eq!(
        run(
            &base,
            &[
                "session", "open", "--chat", "bad", "--paths", "src", "--ttl", "0"
            ]
        )
        .0,
        2
    );
}

#[test]
fn reference_repair_follows_target_and_document_moves_and_preserves_suffixes() {
    let base = root("reference-history");
    write(
        &base,
        "docs/readme.md",
        "[old](../old.md#part)\n[ref]: ../old.md:165\n`old.md`\n```md\n[example](../old.md)\n```\n",
    );
    write(&base, "old.md", "# Target\n");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "original"]);
    std::fs::create_dir_all(base.join("docs/deeper")).unwrap();
    git(&base, &["mv", "old.md", "new.md"]);
    git(&base, &["mv", "docs/readme.md", "docs/deeper/readme.md"]);
    git(&base, &["commit", "-qm", "rename"]);
    let before = read(&base, "docs/deeper/readme.md");
    let (code, out, err) = run(&base, &["links", "repair", "docs", "--linkify", "--json"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("../../new.md#part"), "{out}");
    assert_eq!(read(&base, "docs/deeper/readme.md"), before);
    let (code, out, err) = run(
        &base,
        &["links", "repair", "docs", "--linkify", "--apply", "--json"],
    );
    assert_eq!(code, 0, "{out}{err}");
    let after = read(&base, "docs/deeper/readme.md");
    assert!(after.contains("[old](../../new.md#part)"), "{after}");
    assert!(after.contains("[ref]: ../../new.md:165"));
    assert!(after.contains("```md\n[example](../old.md)\n```"));
    assert_eq!(run(&base, &["links", "check", "docs"]).0, 0);
    assert!(
        run(&base, &["links", "repair", "docs", "--linkify", "--json"])
            .1
            .contains("\"files_changed\":0")
    );
}

#[test]
fn reference_history_never_chooses_between_reused_names() {
    let base = root("reference-ambiguity");
    write(&base, "old.md", "# Target\n");
    write(&base, "index.md", "[old](old.md)\n");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "original"]);
    git(&base, &["mv", "old.md", "one.md"]);
    git(&base, &["commit", "-qm", "first"]);
    write(&base, "old.md", "# Another target\n");
    git(&base, &["add", "old.md"]);
    git(&base, &["commit", "-qm", "reuse"]);
    git(&base, &["mv", "old.md", "two.md"]);
    git(&base, &["commit", "-qm", "second"]);
    let (code, out, err) = run(&base, &["links", "repair", "index.md", "--apply", "--json"]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(out.contains("\"status\":\"ambiguous\""), "{out}");
    assert_eq!(read(&base, "index.md"), "[old](old.md)\n");
}

#[test]
fn log_workflow_references_use_aliases_and_failed_validation_rolls_back() {
    let base = root("log-workflow-refs");
    write(
        &base,
        ".clerk/config/kinds/logs.toml",
        &LOGS_KIND.replace(
            "handler = \"append\"",
            "handler = \"append\"\nentry_target = \"tasks\"",
        ),
    );
    let workflow = r#"version = 1
[[step]]
kind = "tasks"
verb = "new"
as = "fresh"
values = { title = "transaction" }
[[step]]
kind = "logs"
verb = "add"
tokens = { title = "Stage", tasks = "${fresh.id}" }
body = "Reservation results\n\n## Detail\nMore evidence without the task id"
"#;
    write(
        &base,
        "flow.toml",
        &workflow.replace("${fresh.id}", "APP-999"),
    );
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "fixture"]);
    let before = read(&base, ".clerk/data/tasks.csv");
    assert_ne!(
        run(
            &base,
            &["batch", "--file", base.join("flow.toml").to_str().unwrap()]
        )
        .0,
        0
    );
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    assert!(
        clerk::storage::records::markdown_files(&base.join("log"))
            .unwrap()
            .is_empty()
    );
    write(&base, "flow.toml", workflow);
    git(&base, &["add", "flow.toml"]);
    git(&base, &["commit", "-qm", "valid workflow"]);
    let (code, out, err) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(code, 0, "{out}{err}");
    let page = clerk::storage::records::markdown_files(&base.join("log"))
        .unwrap()
        .remove(0);
    let entries =
        clerk::collaboration::log_entry::parse(&std::fs::read_to_string(page).unwrap()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].keys, ["APP-001"]);
    assert!(entries[0].text.contains("## Detail\nMore evidence"));
}

#[test]
fn reference_linkification_preserves_generated_regions_and_normalizes_root_aliases() {
    let base = root("reference-generated");
    write(&base, "docs/source.rs", "// implementation\n");
    write(&base, "root-only.md", "# Repository README\n");
    write(
        &base,
        "docs/readme.md",
        "`source.rs`\n`root-only.md`\n\n<!-- clerk:begin example -->\n`source.rs`\n[missing](old.rs)\n<!-- clerk:end example -->\n\n```text\n`source.rs`\n```\n",
    );
    // An equivalent root containing .. must not break write-boundary checks.
    let alias = base.join("docs/..");
    let (code, out, err) = run(
        &alias,
        &["links", "repair", "docs", "--linkify", "--apply", "--json"],
    );
    assert_eq!(code, 1, "{out}{err}");
    let body = read(&base, "docs/readme.md");
    assert!(body.starts_with("[source.rs](<source.rs>)"), "{body}");
    assert!(body.contains("`root-only.md`"));
    assert!(body.contains("<!-- clerk:begin example -->\n`source.rs`"));
    assert!(body.contains("```text\n`source.rs`\n```"));
    let (code, out, err) = run(&base, &["links", "scan", "docs", "--linkify", "--json"]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(out.contains("\"files_changed\":0"));
}

#[test]
fn rendered_path_fields_link_from_the_document_without_changing_stored_values() {
    let base = root("rendered-path-links");
    write(&base, "docs/proof.md", "# Evidence\n");
    write(
        &base,
        "docs/status.md",
        "# Tasks\n<!-- clerk:begin tasks -->\n<!-- clerk:end tasks -->\n",
    );
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &format!(
            r#"{TASK_KIND}
[[view]]
name = "status"
renderer = "table"
target = "docs/status.md"
region = "tasks"
columns = ["id", "evidence"]
"#
        ),
    );
    let (code, out, err) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "linked evidence",
            "--evidence",
            "docs/proof.md",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, out, err) = run(&base, &["render"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(read(&base, "docs/status.md").contains("[docs/proof.md](<proof.md>)"));
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    let (code, out, err) = run(&base, &["tasks", "show", "APP-001", "--json"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("\"evidence\":\"docs/proof.md\""));
    assert!(!out.contains("[docs/proof.md]"));
}

#[test]
fn unique_command_and_verb_prefixes_use_the_full_command_defaults() {
    let base = root("prefix-defaults");
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[commands.\"tasks.new\"]\ntitle = \"From default\"\n"),
    );
    let (code, stdout, stderr) = run(&base, &["tas", "ne"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("From default"), "{stdout}");
    let (code, stdout, stderr) = run(&base, &["tas", "ne", "--title", "Explicit"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Explicit") && !stdout.contains("From default"),
        "{stdout}"
    );
}

#[test]
fn columns_and_sort_keep_fields_outside_the_default_list_view() {
    let base = root("full-record-output");
    let declaration = read(&base, ".clerk/config/kinds/tasks.toml")
        + "\n[[field]]\nname = \"priority\"\ntype = \"int\"\n";
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    assert_eq!(
        run(
            &base,
            &["tasks", "new", "--title", "Alpha", "--priority", "9"]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &["tasks", "new", "--title", "Beta", "--priority", "1"]
        )
        .0,
        0
    );
    let (code, stdout, stderr) = run(
        &base,
        &[
            "tasks",
            "list",
            "--sort",
            "priority",
            "--columns",
            "title,priority",
            "--tsv",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "title\tpriority\nBeta\t1\nAlpha\t9\n");
}

#[test]
fn fallback_help_uses_the_shared_command_and_option_descriptions() {
    let base = root("fallback-help");
    std::fs::remove_file(base.join(".clerk/workspace.toml")).unwrap();
    let (code, stdout, stderr) = run(&base, &["help"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("links") && stdout.contains("--root") && !stdout.contains("--headers"),
        "{stdout}"
    );
    let (code, _, stderr) = run(&base, &["--unknown", "value"]);
    assert_eq!(code, 2, "{stderr}");
}

#[test]
fn manually_written_requests_cannot_bypass_token_validation() {
    let base = root("request-unknown-token");
    initial_commit(&base);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "Requested",
                "--request",
                "bad-token"
            ]
        )
        .0,
        0
    );
    let request = read(&base, ".clerk/requests/pending/tasks-bad-token.toml")
        + "\n[tokens]\nprefx = \"APP\"\n";
    write(
        &base,
        ".clerk/requests/pending/tasks-bad-token.toml",
        &request,
    );
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, _, stderr) = run(&base, &["request", "apply"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("prefx"), "{stderr}");
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
}

#[test]
fn requests_reject_read_verbs_and_undeclared_legacy_aliases() {
    for verb in ["list", "create", "nosuch"] {
        let base = root(&format!("request-invalid-verb-{verb}"));
        initial_commit(&base);
        assert_eq!(
            run(
                &base,
                &[
                    "tasks",
                    "new",
                    "--title",
                    "Requested",
                    "--request",
                    "invalid-verb"
                ]
            )
            .0,
            0
        );
        let manifest = read(&base, ".clerk/requests/pending/tasks-invalid-verb.toml")
            .replace("verb = \"new\"", &format!("verb = \"{verb}\""));
        write(
            &base,
            ".clerk/requests/pending/tasks-invalid-verb.toml",
            &manifest,
        );
        let (code, _, stderr) = run(&base, &["request", "apply"]);
        assert_eq!(code, 2, "{stderr}");
        assert!(stderr.contains("declared write verb"), "{stderr}");
        assert!(!base.join(".clerk/requests/applied.log").exists());
    }
}

fn reference_fields(base: &Path) {
    write(
        base,
        ".clerk/config/registry/person.csv",
        "person,email,name\nalice,a@example.com,Alice\nalice,b@example.com,Other\n",
    );
    write(
        base,
        ".clerk/config/kinds/tasks.toml",
        &format!(
            "{TASK_KIND}\n[[field]]\nname = \"owners\"\ntype = \"ref\"\nregistry = \"person\"\nmulti = true\n\n[[field]]\nname = \"related\"\ntype = \"ref\"\ntarget = \"contracts\"\nmulti = true\n"
        ),
    );
}

#[test]
fn references_validate_each_key_on_writes_capture_defaults_and_check() {
    let base = root("reference-values");
    reference_fields(&base);
    for args in [
        vec![
            "tasks",
            "new",
            "--title",
            "Bad",
            "--owners",
            "alice|unknown",
        ],
        vec![
            "tasks",
            "new",
            "--title",
            "Bad",
            "--owners",
            "Alice",
            "--request",
            "bad",
        ],
        vec!["tasks", "new", "--title", "Bad", "--related", "CON-99"],
        vec!["tasks", "new", "--title", "Bad", "--owners", "|"],
    ] {
        let (code, _, stderr) = run(&base, &args);
        assert_eq!(code, 3, "{stderr}");
        assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    }
    let (code, _, stderr) = run(
        &base,
        &["tasks", "new", "--title", "Valid", "--owners", "alice"],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(&base, &["tasks", "set", "APP-001", "--owners", "unknown"]);
    assert_eq!(code, 3, "{stderr}");
    let (code, _, stderr) = run(&base, &["check"]);
    assert_eq!(code, 0, "{stderr}");
    let content = read(&base, ".clerk/data/tasks.csv").replace("alice", "unknown");
    write(&base, ".clerk/data/tasks.csv", &content);
    let (code, stdout, stderr) = run(&base, &["check"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stdout.contains("unknown"), "{stdout}");
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[commands.\"tasks.new\"]\nowners = \"unknown\"\n"),
    );
    let (code, _, stderr) = run(&base, &["kinds"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("owners"), "{stderr}");
}

#[test]
fn references_include_archived_table_and_document_targets() {
    let base = root("reference-archives");
    reference_fields(&base);
    write(
        &base,
        ".clerk/archive/contracts/CON-09.md",
        "# Archived\n\n<!-- clerk:meta\nid = CON-09\nstate = done\n-->\n",
    );
    let (code, _, stderr) = run(
        &base,
        &["tasks", "new", "--title", "Valid", "--related", "CON-09"],
    );
    assert_eq!(code, 0, "{stderr}");
    let kind = read(&base, ".clerk/config/kinds/tasks.toml")
        .replace("target = \"contracts\"", "target = \"tasks\"");
    write(&base, ".clerk/config/kinds/tasks.toml", &kind);
    write(
        &base,
        ".clerk/archive/tasks.csv",
        "id,title,state\nAPP-099,Archived,done\n",
    );
    let (code, _, stderr) = run(&base, &["tasks", "set", "APP-001", "--related", "APP-099"]);
    assert_eq!(code, 0, "{stderr}");
}

#[test]
fn reference_declarations_require_one_supported_source() {
    for (name, field) in [
        ("missing", "type = \"ref\""),
        (
            "both",
            "type = \"ref\"\nregistry = \"states\"\ntarget = \"tasks\"",
        ),
        ("ignored", "type = \"text\"\nregistry = \"states\""),
        ("unknown", "type = \"ref\"\ntarget = \"missing\""),
        ("composite", "type = \"ref\"\ntarget = \"reviews\""),
    ] {
        let base = root(&format!("reference-source-{name}"));
        write(
            &base,
            ".clerk/config/kinds/tasks.toml",
            &format!("{TASK_KIND}\n[[field]]\nname = \"related\"\n{field}\n"),
        );
        let (code, _, stderr) = run(&base, &["kinds"]);
        assert_eq!(code, 2, "{name}: {stderr}");
    }
}

#[test]
fn replay_bulk_and_documents_use_reference_checks() {
    let base = root("reference-entry-points");
    reference_fields(&base);
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "baseline"]);
    let (code, _, stderr) = run(
        &base,
        &[
            "tasks",
            "new",
            "--title",
            "Captured",
            "--owners",
            "alice",
            "--request",
            "valid",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let path = ".clerk/requests/pending/tasks-valid.toml";
    write(&base, path, &read(&base, path).replace("alice", "unknown"));
    let (code, _, stderr) = run(&base, &["request", "apply", "valid"]);
    assert_eq!(code, 3, "{stderr}");
    assert!(base.join(path).exists());
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    write(
        &base,
        "input.csv",
        "title,owners\nValid,alice\nInvalid,unknown\n",
    );
    let (code, _, stderr) = run(&base, &["tasks", "new", "--from", "input.csv"]);
    assert_eq!(code, 3, "{stderr}");
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
    write(
        &base,
        ".clerk/config/kinds/contracts.toml",
        &format!(
            "{CONTRACT_KIND}\n[[field]]\nname = \"owner\"\ntype = \"ref\"\nregistry = \"person\"\n"
        ),
    );
    let (code, _, stderr) = run(
        &base,
        &[
            "contracts",
            "new",
            "--title",
            "Bad",
            "--owner",
            "unknown",
            "--body",
            "Body",
        ],
    );
    assert_eq!(code, 3, "{stderr}");
    assert!(!base.join("contracts/CON-01.md").exists());
}

#[test]
fn real_document_declarations_accept_creation_inputs_and_keep_identity_fixed() {
    let base = root("document-input-access");
    write(
        &base,
        ".clerk/config/kinds/contracts.toml",
        include_str!("../examples/library/kinds/contracts.toml"),
    );
    write(
        &base,
        ".clerk/config/kinds/reviews.toml",
        include_str!("../examples/library/kinds/reviews.toml"),
    );
    write(
        &base,
        "contracts/README.md",
        "# Contracts\n\n| File | Title |\n|---|---|\n",
    );
    let (code, _, stderr) = run(
        &base,
        &[
            "contracts",
            "new",
            "--slug",
            "reservation-record",
            "--title",
            "Reservation",
            "--body",
            "Original body",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(read(&base, "contracts/CON-01-reservation-record.md").contains("Original body"));
    let (code, _, stderr) = run(&base, &["contracts", "set", "CON-01", "--title", "Updated"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(read(&base, "contracts/README.md").contains("Updated"));
    assert!(!read(&base, "contracts/README.md").contains("Reservation"));
    assert!(read(&base, "contracts/CON-01-reservation-record.md").contains("Original body"));
    let before = read(&base, "contracts/CON-01-reservation-record.md");
    for args in [
        vec!["contracts", "set", "CON-01", "--slug", "renamed"],
        vec![
            "contracts",
            "new",
            "--slug",
            "../escape",
            "--title",
            "Bad",
            "--body",
            "Body",
        ],
    ] {
        let (code, _, _) = run(&base, &args);
        assert_ne!(code, 0);
    }
    assert_eq!(
        before,
        read(&base, "contracts/CON-01-reservation-record.md")
    );
    let (code, _, stderr) = run(
        &base,
        &[
            "reviews",
            "new",
            "--slug",
            "input-access",
            "--title",
            "Review",
            "--body",
            "## 结论\n## 依据与方法\n## 发现\n## 建议与验收\n## 未验证边界\n",
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, stdout, stderr) = run(&base, &["reviews", "list", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("input-access"), "{stdout}");
    let (code, stdout, stderr) = run(&base, &["help", "contracts"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("create only") && stdout.contains("tool maintained"),
        "{stdout}"
    );
}

const WORKFLOW: &str = r#"version = 1
[[step]]
kind = "tasks"
verb = "new"
as = "task"
values = { title = "Workflow task" }
[[step]]
kind = "tasks"
verb = "start"
keys = ["@task"]
[[step]]
kind = "tasks"
verb = "set"
keys = ["@task"]
values = { evidence = "tasks/${task.id}.md" }
[[step]]
kind = "tasks"
verb = "brief"
keys = ["@task"]
body = "Created ${task.id}"
[[step]]
verb = "render"
"#;

#[test]
fn workflow_commits_once_and_dry_run_restores_everything() {
    let base = root("workflow-once");
    write(&base, "flow.toml", WORKFLOW);
    enable_auto_commit(&base);
    let before = read(&base, ".clerk/data/tasks.csv");
    let head = git(&base, &["rev-parse", "HEAD"]);
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "--file",
            base.join("flow.toml").to_str().unwrap(),
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "--file",
            base.join("flow.toml").to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("succeeded"), "{stdout}");
    assert_eq!(
        git(
            &base,
            &["rev-list", "--count", &format!("{}..HEAD", head.trim())]
        )
        .trim(),
        "1"
    );
    assert!(read(&base, "tasks/APP-001.md").contains("Created APP-001"));
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    let paths = git(&base, &["show", "--format=", "--name-only", "HEAD"]);
    assert!(!paths.contains(".clerk/operations/"), "{paths}");
    let (code, stdout, stderr) = run(&base, &["operation", "status", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"data\":\"saved\""), "{stdout}");
}

#[test]
fn workflow_rejects_bad_steps_and_dirty_trees_without_partial_writes() {
    let base = root("workflow-rollback");
    let flow = WORKFLOW.replace("verb = \"start\"", "verb = \"complete\"");
    write(&base, "flow.toml", &flow);
    enable_auto_commit(&base);
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, _, stderr) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_ne!(code, 0, "{stderr}");
    assert!(stderr.contains("step 2"), "{stderr}");
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    write(&base, "other.txt", "other user work");
    let (code, _, stderr) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(code, 4, "{stderr}");
    assert_eq!(read(&base, "other.txt"), "other user work");
}

#[test]
fn git_retry_commits_saved_data_without_replaying_the_verb() {
    let base = root("git-retry");
    enable_auto_commit(&base);
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "Once", "--json"]);
    assert_ne!(code, 0, "{stdout} {stderr}");
    assert!(
        stdout.contains("\"data\":\"saved\"") && stdout.contains("\"commit\":\"failed\""),
        "{stdout}"
    );
    let before = read(&base, ".clerk/data/tasks.csv");
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 0\n");
    let (code, stdout, stderr) = run(&base, &["operation", "retry", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
    assert!(!before.contains("APP-002"));
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    let head = git(&base, &["rev-parse", "HEAD"]);
    assert_eq!(run(&base, &["operation", "retry"]).0, 0);
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
}

#[test]
fn safe_sync_only_fast_forwards_clean_configured_branches() {
    let base = root("sync-update");
    let remote = root("sync-update-remote");
    initial_commit(&base);
    git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]);
    git(
        &base,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&base, &["push", "-q", "origin", "main"]);
    let baseline = git(&base, &["rev-parse", "HEAD"]);
    write(&base, "remote.txt", "remote update");
    git(&base, &["add", "remote.txt"]);
    git(&base, &["commit", "-qm", "remote change"]);
    git(&base, &["push", "-q", "origin", "main"]);
    git(&base, &["reset", "--hard", baseline.trim()]);
    let (code, _, stderr) = run(&base, &["sync", "update"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(read(&base, "remote.txt"), "remote update");
    write(&base, "local.txt", "untracked work");
    let (code, _, stderr) = run(&base, &["sync", "update"]);
    assert_eq!(code, 4, "{stderr}");
    assert_eq!(read(&base, "local.txt"), "untracked work");
    git(&base, &["add", "local.txt"]);
    git(&base, &["commit", "-qm", "local change"]);
    git(&base, &["checkout", "--detach"]);
    assert_eq!(run(&base, &["sync", "update"]).0, 4);
}

#[test]
fn workflow_handles_full_task_lifecycle_and_records_each_step() {
    let base = root("workflow-lifecycle");
    write(
        &base,
        ".clerk/config/kinds/tasks.toml",
        &TASK_KIND.replace(
            "require_empty = [\"claimant\"]",
            "require_empty = [\"claimant\"]\nrequire_clean = true",
        ),
    );
    let flow = r#"version = 1
[[step]]
kind = "tasks"
verb = "new"
as = "task"
values = { title = "Complete task" }
[[step]]
kind = "tasks"
verb = "claim"
keys = ["@task"]
[[step]]
kind = "tasks"
verb = "start"
keys = ["@task"]
[[step]]
kind = "tasks"
verb = "brief"
keys = ["@task"]
body = "Scope and acceptance"
[[step]]
kind = "tasks"
verb = "finish"
keys = ["@task"]
body = "Verified result"
[[step]]
kind = "tasks"
verb = "complete"
keys = ["@task"]
values = { evidence = "tasks/${task.id}-done.md", commit = "abcdef123456" }
[[step]]
kind = "logs"
verb = "add"
tokens = { title = "Completed ${task.id}", side = "tester" }
body = "Work verified"
[[step]]
verb = "render"
"#;
    write(&base, "flow.toml", flow);
    enable_auto_commit(&base);
    let (code, stdout, stderr) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("done"));
    assert!(read(&base, "tasks/APP-001-done.md").contains("Verified result"));
    let receipts: Vec<_> = std::fs::read_dir(base.join(".git/clerk/operations"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let receipt = std::fs::read_to_string(&receipts[0]).unwrap();
    let parsed = clerk::config::decl::parse(&receipt, "receipt").unwrap();
    assert_eq!(
        clerk::config::decl::array_of_tables(parsed["batch"].as_table().unwrap(), "step").len(),
        8
    );
    assert_eq!(run(&base, &["check"]).0, 0);
}

#[test]
fn push_retry_preserves_the_commit_and_refuses_changed_files() {
    let base = root("push-retry-state");
    let remote = root("push-retry-remote");
    git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]);
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[policy]\nauto_commit = true\nauto_push = true\n"),
    );
    initial_commit(&base);
    git(
        &base,
        &[
            "remote",
            "add",
            "origin",
            base.join("missing.git").to_str().unwrap(),
        ],
    );
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--title", "Once", "--json"]);
    assert_ne!(code, 0, "{stdout} {stderr}");
    assert!(
        stdout.contains("\"commit\":\"succeeded\"") && stdout.contains("\"push\":\"unknown\""),
        "{stdout}"
    );
    let head = git(&base, &["rev-parse", "HEAD"]);
    git(
        &base,
        &["remote", "set-url", "origin", remote.to_str().unwrap()],
    );
    let before = read(&base, ".clerk/data/tasks.csv");
    write(&base, ".clerk/data/tasks.csv", &(before.clone() + "\n"));
    assert_eq!(run(&base, &["operation", "retry"]).0, 4);
    write(&base, ".clerk/data/tasks.csv", &before);
    let (code, stdout, stderr) = run(&base, &["operation", "retry", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("\"push\":\"succeeded\""), "{stdout}");
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
    assert_eq!(head.trim(), git(&remote, &["rev-parse", "main"]).trim());
    assert_eq!(run(&base, &["operation", "retry"]).0, 0);
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
}

#[test]
fn write_sync_reloads_defaults_and_refuses_diverged_history() {
    let base = root("sync-reload");
    let remote = root("sync-reload-remote");
    initial_commit(&base);
    git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]);
    git(
        &base,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&base, &["push", "-q", "origin", "main"]);
    let original = git(&base, &["rev-parse", "HEAD"]);
    write(
        &base,
        ".clerk/workspace.toml",
        &format!("{WORKSPACE}\n[commands.\"tasks.new\"]\ntitle = \"Remote default\"\n"),
    );
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "remote default"]);
    git(&base, &["push", "-q", "origin", "main"]);
    git(&base, &["reset", "--hard", original.trim()]);
    let (code, stdout, stderr) = run(&base, &["tasks", "new", "--sync", "ff-only"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("Remote default"));
    git(&base, &["reset", "--hard", original.trim()]);
    write(&base, "local.txt", "local");
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "local divergence"]);
    let head = git(&base, &["rev-parse", "HEAD"]);
    let (code, _, stderr) = run(&base, &["sync", "update"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("diverged"), "{stderr}");
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
}

#[test]
fn workflow_final_check_rolls_back_documents_and_indexes() {
    let base = root("workflow-final-check");
    write(
        &base,
        ".clerk/config/kinds/contracts.toml",
        &(CONTRACT_KIND.to_string()
            + "\n[sections]\nrequired = [\"Required section\"]\n\n[index]\nfile = \"contracts/README.md\"\ncolumns = [\"file\", \"title\"]\n"),
    );
    let index = "# Contracts\n\n| File | Title |\n|---|---|\n";
    write(&base, "contracts/README.md", index);
    write(
        &base,
        "flow.toml",
        "version = 1\n[[step]]\nkind = \"contracts\"\nverb = \"new\"\nvalues = { title = \"Incomplete\" }\nbody = \"Missing required section\"\n",
    );
    enable_auto_commit(&base);
    let (code, _, stderr) = run(
        &base,
        &["batch", "--file", base.join("flow.toml").to_str().unwrap()],
    );
    assert_eq!(code, 3, "{stderr}");
    assert!(!base.join("contracts/CON-01.md").exists());
    assert_eq!(read(&base, "contracts/README.md"), index);
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn git_retry_preserves_unrelated_staging_and_newer_heads() {
    let base = root("retry-preserves-work");
    enable_auto_commit(&base);
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
    assert_ne!(run(&base, &["tasks", "new", "--title", "Once"]).0, 0);
    write(&base, "unrelated.txt", "Other user work");
    git(&base, &["add", "unrelated.txt"]);
    let status = git(&base, &["status", "--porcelain"]);
    assert_eq!(run(&base, &["operation", "retry"]).0, 4);
    assert_eq!(status, git(&base, &["status", "--porcelain"]));
    write(&base, ".git/hooks/pre-commit", "#!/bin/sh\nexit 0\n");
    git(&base, &["commit", "-qm", "manual resolution"]);
    let head = git(&base, &["rev-parse", "HEAD"]);
    assert_eq!(run(&base, &["operation", "retry"]).0, 4);
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
}

#[test]
fn configured_write_sync_updates_before_loading_command_defaults() {
    let base = root("sync-policy");
    let remote = root("sync-policy-remote");
    let config = format!("{WORKSPACE}\n[sync]\nwrite_update = \"ff-only\"\n");
    write(&base, ".clerk/workspace.toml", &config);
    initial_commit(&base);
    git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]);
    git(
        &base,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&base, &["push", "-q", "origin", "main"]);
    let original = git(&base, &["rev-parse", "HEAD"]);
    write(
        &base,
        ".clerk/workspace.toml",
        &(config + "\n[commands.\"tasks.new\"]\ntitle = \"Policy default\"\n"),
    );
    git(&base, &["add", "."]);
    git(&base, &["commit", "-qm", "remote defaults"]);
    git(&base, &["push", "-q", "origin", "main"]);
    git(&base, &["reset", "--hard", original.trim()]);
    let (code, stdout, stderr) = run(&base, &["tasks", "new"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(read(&base, ".clerk/data/tasks.csv").contains("Policy default"));
    let before = read(&base, ".clerk/data/tasks.csv");
    assert_eq!(run(&base, &["tasks", "new", "--title", "Blocked"]).0, 4);
    assert_eq!(before, read(&base, ".clerk/data/tasks.csv"));
}

#[test]
fn batch_changes_preserve_members_validate_inputs_and_avoid_empty_commits() {
    let base = root("batch-changes");
    for title in ["one", "two", "three"] {
        assert_eq!(run(&base, &["tasks", "new", "--title", title]).0, 0);
    }
    assert_eq!(run(&base, &["focus", "set", "APP-001"]).0, 0);
    enable_auto_commit(&base);
    let head = git(&base, &["rev-parse", "HEAD"]);
    let (code, stdout, stderr) = run(&base, &["focus", "add", "APP-001", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("\"added\":[]"), "{stdout}");
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
    let before = read(&base, ".clerk/config/registry/focus.csv");
    assert_eq!(run(&base, &["focus", "add", "APP-002", "missing"]).0, 3);
    assert_eq!(before, read(&base, ".clerk/config/registry/focus.csv"));
    assert_eq!(run(&base, &["focus", "add", "APP-002", "--dry-run"]).0, 0);
    assert_eq!(before, read(&base, ".clerk/config/registry/focus.csv"));
    assert_eq!(
        run(&base, &["focus", "add", "APP-002", "APP-002", "APP-003"]).0,
        0
    );
    assert_eq!(
        read(&base, ".clerk/config/registry/focus.csv"),
        "key\nAPP-001\nAPP-002\nAPP-003\n"
    );
    assert_eq!(run(&base, &["focus", "remove", "missing", "APP-002"]).0, 0);
    assert_eq!(
        read(&base, ".clerk/config/registry/focus.csv"),
        "key\nAPP-001\nAPP-003\n"
    );
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

const PARAMETER_FLOW: &str = r#"version = 1
title = "Create and start"
[[param]]
name = "description"
type = "text"
required = true
[[step]]
kind = "tasks"
verb = "new"
as = "created"
values = { title = "${params.description}" }
tokens = { prefix = "APP" }
[[step]]
kind = "focus"
verb = "add"
keys = ["@created"]
[[step]]
kind = "tasks"
verb = "start"
keys = ["@created"]
"#;

#[test]
fn template_preview_is_structural_and_execution_uses_one_transaction() {
    let base = root("template-execution");
    write(&base, ".clerk/config/workflows/start.toml", PARAMETER_FLOW);
    enable_auto_commit(&base);
    let head = git(&base, &["rev-parse", "HEAD"]);
    let literal = "quotes \"\n[[step]]\n${created.id}";
    let (code, stdout, stderr) = run(
        &base,
        &["workflow", "preview", "start", "--description", literal],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let value = stdout.parse::<toml::Value>().expect("executable TOML");
    assert_eq!(value["step"].as_array().unwrap().len(), 3);
    assert_eq!(
        value["step"][0]["values"]["title"].as_str(),
        Some("quotes \"\n[[step]]\n$${created.id}")
    );
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    assert!(!base.join(".clerk/config/registry/focus.csv").exists());
    let (code, stdout, stderr) = run(
        &base,
        &[
            "batch",
            "--template",
            "start",
            "--description",
            literal,
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(head, git(&base, &["rev-parse", "HEAD"]));
    assert!(!base.join(".clerk/config/registry/focus.csv").exists());
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    let (code, stdout, stderr) = run(
        &base,
        &["batch", "--template", "start", "--description", literal],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(
        git(
            &base,
            &["rev-list", "--count", &format!("{}..HEAD", head.trim())]
        )
        .trim(),
        "1"
    );
    let context = clerk::execution::context::Context::open(base.to_str()).unwrap();
    let records =
        clerk::storage::records::all(&context, context.catalog.require("tasks").unwrap()).unwrap();
    assert_eq!(records[0]["title"], literal);
    assert_eq!(records[0]["state"], "active");
    assert_eq!(
        read(&base, ".clerk/config/registry/focus.csv"),
        "key\nAPP-001\n"
    );
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn template_parameters_and_declarations_fail_before_business_writes() {
    let base = root("template-validation");
    write(&base, ".clerk/config/workflows/start.toml", PARAMETER_FLOW);
    initial_commit(&base);
    for args in [
        vec!["batch", "--template", "start"],
        vec!["batch", "--template", "start", "--descriptino", "wrong"],
        vec![
            "batch",
            "--template",
            "start",
            "--file",
            "flow.toml",
            "--description",
            "both",
        ],
    ] {
        let (code, _, stderr) = run(&base, &args);
        assert_eq!(code, 2, "{stderr}");
    }
    let typed = PARAMETER_FLOW.replace("type = \"text\"", "type = \"ref\"\ntarget = \"tasks\"");
    write(&base, ".clerk/config/workflows/start.toml", &typed);
    assert_eq!(
        run(
            &base,
            &["workflow", "preview", "start", "--description", "missing"]
        )
        .0,
        3
    );
    let invalid = PARAMETER_FLOW.replace("${params.description}", "${params.typo}");
    write(&base, ".clerk/config/workflows/start.toml", &invalid);
    let (code, _, stderr) = run(&base, &["kinds"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("unknown template parameter"), "{stderr}");
    let collision = PARAMETER_FLOW.replace("description", "json");
    write(&base, ".clerk/config/workflows/start.toml", &collision);
    assert_eq!(run(&base, &["kinds"]).0, 2);
    assert_eq!(read(&base, ".clerk/data/tasks.csv").lines().count(), 1);
}

#[test]
fn failed_template_rolls_back_batch_and_allocated_records() {
    let base = root("template-rollback");
    let flow = format!(
        "{PARAMETER_FLOW}\n[[step]]\nkind = \"tasks\"\nverb = \"start\"\nkeys = [\"@created\"]\n"
    );
    write(&base, ".clerk/config/workflows/start.toml", &flow);
    enable_auto_commit(&base);
    let before = read(&base, ".clerk/data/tasks.csv");
    let (code, _, stderr) = run(
        &base,
        &["batch", "--template", "start", "--description", "rollback"],
    );
    assert_eq!(code, 4, "{stderr}");
    assert_eq!(read(&base, ".clerk/data/tasks.csv"), before);
    assert!(!base.join(".clerk/config/registry/focus.csv").exists());
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
}

#[test]
fn handoff_uses_declared_fields_full_ids_and_explicit_document_relations() {
    let base = root("handoff-sources");
    let declaration = format!(
        r#"{TASK_KIND}
[[field]]
name = "specification"
type = "path"
[[relation]]
name = "contract"
target = "contracts"
[context]
attachments = ["specification"]
logs = "logs"
"#
    );
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    let specification = format!(
        "# Original\n\n## brief\n| a | b |\n```text\nSource: original\n```\n{}",
        "验收依据\n".repeat(1000)
    );
    write(&base, "spec.md", &specification);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "handoff",
                "--specification",
                "spec.md"
            ]
        )
        .0,
        0
    );
    assert_eq!(run(&base, &["tasks", "new", "--title", "dependency"]).0, 0);
    assert_eq!(
        run(
            &base,
            &[
                "contracts",
                "new",
                "--title",
                "protocol",
                "--body",
                "contract body"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "link",
                "APP-001",
                "contract",
                "contracts",
                "CON-01"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &base,
            &["tasks", "link", "APP-002", "blocked", "tasks", "APP-001"]
        )
        .0,
        0
    );
    write(
        &base,
        "log/20261101.md",
        "# Logs\n\n## [T090000] older\nAPP-001 older\n\n## [T100000] unrelated\nAPP-0010 prefix trap\n\n## [T110000] current\n修复APP-001完成\n",
    );
    initial_commit(&base);
    let before = git(&base, &["rev-parse", "HEAD"]);
    let (code, stdout, stderr) = run(&base, &["context", "tasks", "APP-001", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    for expected in [
        "\"version\":1",
        "\"role\":\"specification\"",
        "\"truncated\":true",
        "contract body",
        "\"direction\":\"incoming\"",
        "\"sha256\":",
        "修复APP-001完成",
    ] {
        assert!(stdout.contains(expected), "missing {expected}: {stdout}");
    }
    assert!(!stdout.contains("prefix trap"), "{stdout}");
    for mode in [Vec::new(), vec!["--compact"]] {
        let mut args = vec!["context", "tasks", "APP-001"];
        args.extend(mode);
        let (code, human, stderr) = run(&base, &args);
        assert_eq!(code, 0, "{human} {stderr}");
        for expected in [
            "Context: tasks APP-001\n",
            "Excerpts:\n  specification:\n",
            "    Content:\n      | # Original\n      | \n      | ## brief\n",
            "      | | a | b |\n      | ```text\n      | Source: original\n",
            "Relations:\n  direction",
            "Sources:\n  ",
        ] {
            assert!(human.contains(expected), "missing {expected}: {human}");
        }
        assert!(!human.lines().any(|line| line.starts_with('#')), "{human}");
    }
    assert_eq!(read(&base, "spec.md"), specification);
    assert_eq!(before, git(&base, &["rev-parse", "HEAD"]));
    assert!(git(&base, &["status", "--porcelain"]).is_empty());
    assert_eq!(
        run(&base, &["context", "tasks", "APP-001", "--sync", "ff-only"]).0,
        2
    );
}

#[test]
fn handoff_preserves_missing_sources_and_never_recovers_a_journal() {
    let base = root("handoff-readonly");
    let declaration = format!(
        "{TASK_KIND}\n[[field]]\nname = \"specification\"\ntype = \"path\"\n[context]\nattachments = [\"specification\"]\n"
    );
    write(&base, ".clerk/config/kinds/tasks.toml", &declaration);
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "new",
                "--title",
                "missing",
                "--specification",
                "absent.md"
            ]
        )
        .0,
        0
    );
    let (code, stdout, stderr) = run(&base, &["context", "tasks", "APP-001", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("absent.md:"), "{stdout}");
    let context = clerk::execution::context::Context::open(base.to_str()).unwrap();
    let guard = clerk::execution::transaction::Transaction::begin(&context.workspace).unwrap();
    assert_eq!(run(&base, &["context", "tasks", "APP-001"]).0, 4);
    drop(guard);
    let local = clerk::execution::transaction::local_dir(&context.workspace).unwrap();
    std::fs::create_dir(local.join("undo")).unwrap();
    let (code, _, stderr) = run(&base, &["context", "tasks", "APP-001"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("unfinished write journal"), "{stderr}");
    assert!(local.join("undo").exists());
}

#[test]
fn machine_declarations_include_permissions_inputs_and_workflow_structure() {
    let base = root("machine-declarations");
    write(&base, ".clerk/config/workflows/start.toml", PARAMETER_FLOW);
    let (code, stdout, stderr) = run(&base, &["explain", "tasks", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    for expected in [
        "\"write\":\"tool\"",
        "\"transitions\":[",
        "\"actions\":[",
        "\"numbering\":",
        "\"verbs\":[",
    ] {
        assert!(stdout.contains(expected), "{expected}: {stdout}");
    }
    let (code, stdout, stderr) = run(&base, &["workflow", "show", "start", "--json"]);
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert!(stdout.contains("\"params\":["), "{stdout}");
    assert!(stdout.contains("${params.description}"), "{stdout}");
    let (_, stdout, _) = run(&base, &["help", "batch", "--compact"]);
    assert!(stdout.contains("--description <text> required"), "{stdout}");
}

#[test]
fn optional_template_parameters_and_defaults_use_field_constraints() {
    let base = root("template-defaults");
    // Insert the parameter declarations once, before the first step.
    let prefix = PARAMETER_FLOW.split_once("[[step]]").unwrap();
    let flow = format!(
        "{}[[param]]\nname = \"count\"\ntype = \"int\"\n[[param]]\nname = \"category\"\ntype = \"enum\"\nregistry = \"type\"\ndefault = \"ability\"\n[[step]]{}",
        prefix.0, prefix.1
    );
    write(&base, ".clerk/config/workflows/start.toml", &flow);
    let (code, stdout, stderr) = run(
        &base,
        &["workflow", "preview", "start", "--description", "defaults"],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    assert_eq!(
        run(
            &base,
            &[
                "workflow",
                "preview",
                "start",
                "--description",
                "defaults",
                "--count",
                "wrong"
            ]
        )
        .0,
        3
    );
    let invalid = flow.replace("default = \"ability\"", "default = \"missing\"");
    write(&base, ".clerk/config/workflows/start.toml", &invalid);
    assert_eq!(run(&base, &["kinds"]).0, 2);
}

#[test]
fn terminal_layout_is_aligned_by_default_and_compact_is_explicit() {
    let base = root("terminal-layout");
    write(&base, ".clerk/config/workflows/start.toml", PARAMETER_FLOW);
    write(
        &base,
        ".clerk/config/workflows/longer-name.toml",
        PARAMETER_FLOW,
    );
    let (code, stdout, stderr) = run(
        &base,
        &["tasks", "new", "--prefix", "APP", "--title", "Example"],
    );
    assert_eq!(code, 0, "{stdout} {stderr}");
    let aligned = run(&base, &["tasks", "list", "--columns", "id,title,state"]).1;
    assert!(aligned.contains("APP-001 Example todo"), "{aligned}");
    let compact = run(
        &base,
        &["tasks", "list", "--columns", "id,title,state", "--compact"],
    )
    .1;
    assert_eq!(compact, "id title state\nAPP-001 Example todo\n");
    let detail = run(
        &base,
        &[
            "tasks",
            "show",
            "APP-001",
            "--columns",
            "id,title",
            "--compact",
        ],
    )
    .1;
    assert_eq!(detail, "id APP-001\ntitle Example\n");
    let detail = run(
        &base,
        &["tasks", "show", "APP-001", "--columns", "id,title"],
    )
    .1;
    assert_eq!(detail, "id    APP-001\ntitle Example\n");

    let workflows = run(&base, &["workflow", "list"]).1;
    assert!(
        workflows.contains("start       Create and start"),
        "{workflows}"
    );
    assert_eq!(
        run(
            &base,
            &["workflow", "list", "--compact", "--headers", "hide"]
        )
        .1,
        "longer-name Create and start\nstart Create and start\n"
    );
    let show = run(&base, &["workflow", "show", "start"]).1;
    assert!(show.contains("title    Create and start"), "{show}");
    let context = run(&base, &["context", "tasks", "APP-001"]).1;
    assert!(
        context.contains("Source:   .clerk/data/tasks.csv"),
        "{context}"
    );
    let lines = context.lines().collect::<Vec<_>>();
    let workspace = lines
        .iter()
        .position(|line| line.starts_with("Workspace:"))
        .unwrap();
    assert_eq!(lines[workspace], format!("Workspace: {}", base.display()));
    for (offset, label) in [(1, "branch:"), (2, "head:"), (3, "status:")] {
        assert_eq!(lines[workspace + offset].find(label), Some(2), "{context}");
    }
    let context = run(&base, &["context", "tasks", "APP-001", "--compact"]).1;
    assert!(
        context.contains("Source: .clerk/data/tasks.csv"),
        "{context}"
    );
    let help = run(&base, &["help", "tasks", "--compact"]).1;
    assert!(help.contains("--compact"), "{help}");
    assert!(help.contains("key: id"), "{help}");

    for format in ["--json", "--csv", "--tsv", "--md"] {
        assert_eq!(
            run(&base, &["tasks", "list", format]).1,
            run(&base, &["tasks", "list", format, "--compact"]).1,
            "{format}"
        );
    }
    let remote = lines
        .iter()
        .position(|line| *line == "  remote (cached):")
        .unwrap();
    assert!(lines[remote + 1].starts_with("    ref:"), "{context}");
    assert!(lines[remote + 2].starts_with("    head:"), "{context}");
    let mut workspace = read(&base, ".clerk/workspace.toml");
    workspace.push_str("\n[commands.\"tasks.list\"]\ncompact = true\n");
    write(&base, ".clerk/workspace.toml", &workspace);
    assert_eq!(
        run(&base, &["tasks", "list", "--columns", "id,title,state"]).1,
        compact
    );
    assert_eq!(
        run(
            &base,
            &[
                "tasks",
                "list",
                "--columns",
                "id,title,state",
                "--no-compact"
            ]
        )
        .1,
        aligned
    );
}
