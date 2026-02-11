/// Project generators for benchmarks and large-scale tests.
use std::fmt::Write;

/// Generate a source file with `count` functions, each `lines_per_fn` lines long.
#[allow(dead_code)]
pub fn generate_functions(lang: &str, count: usize, lines_per_fn: usize) -> String {
    let mut source = String::new();
    let body_lines = lines_per_fn.saturating_sub(2);

    match lang {
        "typescript" | "ts" => {
            for i in 0..count {
                writeln!(source, "function func_{i}(arg: string): string {{").unwrap();
                for j in 0..body_lines {
                    writeln!(source, "    const val_{j} = arg + \"{j}\";").unwrap();
                }
                writeln!(source, "    return arg;\n}}\n").unwrap();
            }
        }
        "python" | "py" => {
            for i in 0..count {
                writeln!(source, "def func_{i}(arg: str) -> str:").unwrap();
                for j in 0..body_lines {
                    writeln!(source, "    val_{j} = arg + \"{j}\"").unwrap();
                }
                writeln!(source, "    return arg\n").unwrap();
            }
        }
        "go" => {
            writeln!(source, "package main\n").unwrap();
            for i in 0..count {
                writeln!(source, "func Func{i}(arg string) string {{").unwrap();
                for j in 0..body_lines {
                    writeln!(source, "\tval{j} := arg + \"{j}\"").unwrap();
                }
                writeln!(source, "\treturn arg\n}}\n").unwrap();
            }
        }
        "rust" | "rs" => {
            for i in 0..count {
                writeln!(source, "pub fn func_{i}(arg: &str) -> String {{").unwrap();
                for j in 0..body_lines {
                    writeln!(source, "    let _val_{j} = format!(\"{{}}_{j}\", arg);").unwrap();
                }
                writeln!(source, "    arg.to_string()\n}}\n").unwrap();
            }
        }
        _ => panic!("Unsupported language: {}", lang),
    }

    source
}

/// Generate a multi-file project structure for benchmark testing.
///
/// Returns a Vec of (relative_path, content) suitable for `create_mapped_project`.
#[allow(dead_code)]
pub fn generate_project(
    files: usize,
    fns_per_file: usize,
    lines_per_fn: usize,
    lang: &str,
) -> Vec<(String, String)> {
    let ext = match lang {
        "typescript" | "ts" => "ts",
        "python" | "py" => "py",
        "go" => "go",
        "rust" | "rs" => "rs",
        _ => panic!("Unsupported language: {}", lang),
    };

    (0..files)
        .map(|i| {
            let path = format!("src/module_{i}.{ext}");
            let content = generate_functions(lang, fns_per_file, lines_per_fn);
            (path, content)
        })
        .collect()
}
