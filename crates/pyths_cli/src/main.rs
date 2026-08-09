use clap::{Parser, Subcommand, ValueEnum};

mod commands;

use commands::output::Verbosity;

/// How aggressively to run the `.psc` expander. Mirrors
/// [`pyths_config::ExpandMode`] so clap can derive a ValueEnum.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ExpandModeArg {
    /// Default: expand `.psc` files; pass `.ps` files through untouched.
    Auto,
    /// Expand every input regardless of extension.
    Always,
    /// Never expand, even for `.psc`. Useful for debugging.
    Never,
}

impl From<ExpandModeArg> for pyths_config::ExpandMode {
    fn from(value: ExpandModeArg) -> Self {
        match value {
            ExpandModeArg::Auto => pyths_config::ExpandMode::Auto,
            ExpandModeArg::Always => pyths_config::ExpandMode::Always,
            ExpandModeArg::Never => pyths_config::ExpandMode::Never,
        }
    }
}

#[derive(Parser)]
#[command(name = "pyths")]
#[command(about = "PythScribe — Write Python. Ship to the Web.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Suppress non-error output
    #[arg(long, global = true)]
    quiet: bool,

    /// Show verbose output (timings, debug info)
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a .ps or .psc file to JavaScript
    Compile {
        /// Input file path (.ps or .psc)
        file: String,

        /// Output file path (default: same name with .js extension)
        #[arg(short, long)]
        output: Option<String>,

        /// Print compiled JS to stdout instead of writing to file
        #[arg(long)]
        stdout: bool,

        /// Emit source map
        #[arg(long)]
        sourcemap: bool,

        /// Omit `sourcesContent` from the emitted `.js.map` (A17). The map
        /// still resolves generated positions to `.ps` line:col, but the
        /// original source text (comments, server-side logic, secrets) is
        /// NOT inlined — recommended for production/edge builds where the
        /// `.map` may ship. Only meaningful with `--sourcemap`.
        #[arg(long)]
        no_sources_content: bool,

        /// Print per-phase timing breakdown
        #[arg(long)]
        timings: bool,

        /// Emit TypeScript declaration file (.d.ts)
        #[arg(long)]
        dts: bool,

        /// Compilation target. One of:
        ///   js        — JavaScript only (default)
        ///   worker    — JS with runtime imports from pyths-runtime/core
        ///               (DOM-free, Cloudflare Worker / edge-safe; B-030 follow-up D)
        ///   wasm      — WASM only
        ///   js+wasm   — JS + WASM with browser glue
        ///   wasm-edge — Cloudflare Workers (base64-embedded WASM)
        ///   wasi      — WASI (Node.js node:wasi)
        ///   deno      — Deno (Deno.readFile loader)
        #[arg(long, default_value = "js")]
        target: String,

        /// Watch the source file and recompile on change. Combines well with
        /// the incremental cache: subsequent compiles after no-op edits are
        /// near-instant.
        #[arg(long)]
        watch: bool,

        /// Control `.psc` compressed-source expansion.
        /// `auto` (default): expand `.psc`, pass `.ps` through.
        /// `always`: expand every file regardless of extension.
        /// `never`: never expand, even `.psc` files.
        #[arg(long, value_enum)]
        expand: Option<ExpandModeArg>,

        /// Emit + self-check a subscript-routing certificate (credible
        /// compilation): writes `<output>.cert.json` and validates the
        /// recorded routing decisions against the rules and the emitted
        /// JS. Non-zero exit on any violation. (JS target only.)
        #[arg(long)]
        emit_cert: bool,

        /// Emit React Refresh `$RefreshSig$` / `$RefreshReg$` boilerplate
        /// for `@component` functions. Intended for dev-mode builds run
        /// behind a build plugin (Vite, Next.js) that installs the
        /// matching runtime globals. Off by default.
        #[arg(long)]
        react_refresh: bool,

        /// Overwrite a pre-existing output file even if it does not look
        /// PythScribe-generated (lacks the generated marker). By default the
        /// compiler refuses to clobber a hand-written sibling (e.g. a
        /// hand-written `Counter.js` next to `Counter.ps`). Symlinked outputs
        /// are refused regardless of this flag.
        #[arg(long)]
        force: bool,
    },

    /// Expand a .psc file into canonical .ps without compiling
    Expand {
        /// Input file path (.psc or .ps)
        file: String,

        /// Output file path (default: stdout)
        #[arg(short, long)]
        output: Option<String>,

        /// Verify the round-trip Iron Rule: canonicalize(expand(file)) must equal
        /// canonicalize(the sibling .ps). Exits non-zero on mismatch.
        #[arg(long)]
        verify: bool,
    },

    /// Type-check a .ps file without compiling
    Check {
        /// Input file path (.ps)
        file: String,

        /// Stop after the parser; skip the type checker. Exposes the
        /// authoritative parser's verdict in isolation (used by
        /// scripts/grammar-fuzz.py to compare against grammar/pyths.lark,
        /// which is a purely syntactic acceptor).
        #[arg(long)]
        syntax_only: bool,
    },

    /// Compile and run a .ps file using Node.js
    Run {
        /// Input file path (.ps)
        file: String,

        /// When the program crashes, print a Python-style explanation
        /// paragraph above the trace and map locations back to `.ps`
        /// source via source maps. Adds a small startup cost; useful
        /// for debugging.
        #[arg(long)]
        explain: bool,
    },

    /// Initialize a new PythScribe project
    Init {
        /// Project name
        name: Option<String>,
    },

    /// Run PythScribe test files
    Test {
        /// Path to test file or directory (default: current directory)
        path: Option<String>,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Format PythScribe source files
    Fmt {
        /// Path to file or directory (default: current directory)
        path: Option<String>,

        /// Check formatting without writing changes
        #[arg(long)]
        check: bool,

        /// Indentation size in spaces (default: 4)
        #[arg(long, default_value = "4")]
        indent: usize,
    },

    /// Lint PythScribe files for common issues
    Lint {
        /// Path to file or directory (default: current directory)
        path: Option<String>,
    },

    /// Bundle a PythScribe project into a single JS file
    Bundle {
        /// Entry file path (.ps)
        entry: String,

        /// Output file path (default: <entry>.bundle.js)
        #[arg(short, long)]
        output: Option<String>,

        /// Minify the output
        #[arg(long)]
        minify: bool,
    },

    /// Manage the incremental-compilation cache (Step 9).
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Print cache location and statistics.
    Status,
    /// Delete the entire cache (next compile will rebuild from scratch).
    Clear,
}

fn main() {
    let cli = Cli::parse();

    // Recursive-descent parsing and the codegen tree-walk both nest as deep as
    // the input allows. On the platform-default main-thread stack (~1 MiB on
    // Windows, ~8 MiB on Unix) a pathologically nested program overflows and
    // hard-crashes the process (`STATUS_STACK_OVERFLOW`) *before* the
    // recursion-depth guards (bound `MAX_PARSE_DEPTH` / `MAX_EMIT_DEPTH` = 1000)
    // can fire. Run the whole command on a worker thread with a large stack —
    // the same approach rustc and CPython use — so the guards are always
    // reached and reported as a clean diagnostic instead of a native crash.
    const STACK_SIZE: usize = 512 * 1024 * 1024; // 512 MiB reserved; committed lazily
    let worker = std::thread::Builder::new()
        .name("pyths-compile".into())
        .stack_size(STACK_SIZE)
        .spawn(move || run_cli(cli))
        .expect("failed to spawn compiler worker thread");

    // A panic inside the worker already printed its own message; surface a
    // non-zero exit rather than the default (which would still be non-zero but
    // via a different path). Any Err from run_cli is handled inside it.
    let code = worker.join().unwrap_or(101);
    std::process::exit(code);
}

/// Dispatch a parsed CLI invocation. Returns the process exit code. Runs on the
/// large-stack worker thread spawned by `main`.
fn run_cli(cli: Cli) -> i32 {
    let verbosity = if cli.quiet {
        Verbosity::Quiet
    } else if cli.verbose {
        Verbosity::Verbose
    } else {
        Verbosity::Normal
    };

    let result = match cli.command {
        Commands::Compile {
            file,
            output,
            stdout,
            sourcemap,
            no_sources_content,
            timings,
            dts,
            target,
            watch,
            expand,
            react_refresh,
            emit_cert,
            force,
        } => {
            let expand_mode = expand.map(Into::into);
            if watch {
                if emit_cert {
                    eprintln!("error: --emit-cert is not supported with --watch");
                    std::process::exit(2);
                }
                commands::compile::watch(
                    &file,
                    output.as_deref(),
                    sourcemap,
                    no_sources_content,
                    timings,
                    dts,
                    &target,
                    expand_mode,
                    react_refresh,
                    force,
                    verbosity,
                )
            } else {
                commands::compile::run(
                    &file,
                    output.as_deref(),
                    stdout,
                    sourcemap,
                    no_sources_content,
                    timings,
                    dts,
                    &target,
                    expand_mode,
                    react_refresh,
                    emit_cert,
                    force,
                    verbosity,
                )
            }
        }
        Commands::Expand {
            file,
            output,
            verify,
        } => commands::expand::run(&file, output.as_deref(), verify, verbosity),
        Commands::Check { file, syntax_only } => {
            commands::check::run(&file, verbosity, syntax_only)
        }
        Commands::Run { file, explain } => commands::run::run(&file, explain, verbosity),
        Commands::Init { name } => commands::init::run(name.as_deref(), verbosity),
        Commands::Test { path, verbose } => {
            // --verbose on Test subcommand overrides global
            let v = if verbose {
                Verbosity::Verbose
            } else {
                verbosity
            };
            commands::test::run(path.as_deref(), v)
        }
        Commands::Fmt {
            path,
            check,
            indent,
        } => commands::fmt::run(path.as_deref(), check, indent, verbosity),
        Commands::Lint { path } => commands::lint::run(path.as_deref(), verbosity),
        Commands::Bundle {
            entry,
            output,
            minify,
        } => commands::bundle::run(&entry, output.as_deref(), minify, verbosity),
        Commands::Cache { action } => match action {
            CacheAction::Status => commands::cache::status(verbosity),
            CacheAction::Clear => commands::cache::clear(verbosity),
        },
    };

    if let Err(e) = result {
        commands::output::error_summary(&format!("Error: {}", e));
        return 1;
    }
    0
}
