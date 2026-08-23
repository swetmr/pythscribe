// Regression tests for the 2026-08-12 security scan findings #1 / #2 / #6.
//
// Each test is the security scan's reproducer, kept as a permanent guard:
//   #1 CWE-426 — a `.` element in PATH let a cwd-planted `pyths.exe` be
//                selected and EXECUTED by the old bare-name `execFileSync`.
//                (Verified live on Node 22 / win32 before the fix.)
//   #2 CWE-59  — `writeFileSync(declPath, …)` followed a `.d.ps.ts` symlink
//                and a hardlink, clobbering the link target.
//   #6 CWE-73  — the old `finally` block unlinked `<stem>.js` / `.js.map` /
//                `<stem>.d.ts` beside the SOURCE whenever they existed,
//                deleting hand-written project files.
//
// Run with: node --test test-safe-api.mjs
import { test } from "node:test";
import assert from "node:assert";
import { execFileSync } from "node:child_process";
import {
    copyFileSync,
    existsSync,
    linkSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    rmSync,
    symlinkSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
    GENERATED_MARKER,
    looksPythsGenerated,
    makePrivateTempDir,
    removePrivateTempDir,
    resolvePythsCommand,
    searchPath,
    writeGeneratedSibling,
} from "./pyths-safe.js";

const WINDOWS = process.platform === "win32";
const scratch = (tag) => mkdtempSync(join(tmpdir(), `pyths-sec-${tag}-`));
const marked = (body) => `// ${GENERATED_MARKER} — do not edit.\n${body}`;

/** Symlink creation needs privilege on Windows; skip cleanly when denied. */
function trySymlink(target, link) {
    try {
        symlinkSync(target, link, "file");
        return true;
    } catch {
        return false;
    }
}

// ===========================================================================
// #1 — CWE-426 untrusted search path
// ===========================================================================

test("#1 searchPath never returns a cwd-planted executable via a `.` PATH entry", () => {
    const d = scratch("path-dot");
    const exe = join(d, WINDOWS ? "pyths_probe.exe" : "pyths_probe");
    copyFileSync(process.execPath, exe);

    const prevPath = process.env.PATH;
    const prevCwd = process.cwd();
    try {
        process.chdir(d);
        // The exact PATH shape that reproduced the hijack on Node 22 / win32.
        process.env.PATH = "." + (WINDOWS ? ";" : ":") + (prevPath || "");
        assert.equal(
            searchPath("pyths_probe"),
            null,
            "a `.` PATH element must never resolve the current directory",
        );
        // ...and the empty-element form, which some platforms also treat as cwd.
        process.env.PATH = (WINDOWS ? ";" : ":") + (prevPath || "");
        assert.equal(searchPath("pyths_probe"), null, "empty PATH element must be skipped");
    } finally {
        process.chdir(prevCwd);
        process.env.PATH = prevPath;
        rmSync(d, { recursive: true, force: true });
    }
});

test("#1 the pre-fix behaviour is real: bare-name exec DID run the cwd binary", () => {
    // The PoC, retained as evidence. `execFileSync` with a BARE name (what the
    // plugins used to pass) executes the cwd-planted program when PATH holds a
    // `.` element. This asserts the THREAT still exists at the platform level,
    // so the searchPath test above is guarding something real rather than a
    // strawman. Non-Windows platforms also honour `.` in PATH.
    const d = scratch("hijack-poc");
    copyFileSync(process.execPath, join(d, WINDOWS ? "pyths_probe.exe" : "pyths_probe"));
    const marker = join(d, "PWNED.txt");
    const p = "." + (WINDOWS ? ";" : ":") + (process.env.PATH || "");
    try {
        execFileSync(
            "pyths_probe",
            ["-e", `require('fs').writeFileSync(${JSON.stringify(marker)},'1')`],
            { cwd: d, env: { ...process.env, PATH: p, Path: p }, timeout: 30000 },
        );
    } catch {
        /* if the platform refuses, the threat is absent here — assertion below */
    }
    const hijacked = existsSync(marker);
    rmSync(d, { recursive: true, force: true });
    // Informational, not a failure: platforms differ. What must hold is that
    // OUR resolver refuses (asserted above) regardless of the platform's
    // appetite for cwd lookup.
    if (!hijacked) {
        console.log("  note: this platform/runtime did not honour `.` in PATH for a bare name");
    }
});

test("#1 resolvePythsCommand never yields a bare name", () => {
    const d = scratch("resolve-abs");
    const fake = join(d, "pyths-fake.exe");
    writeFileSync(fake, "");
    try {
        const cmd = resolvePythsCommand({ pythsBin: fake });
        assert.ok(
            cmd.command.includes("/") || cmd.command.includes("\\"),
            `resolved command must be a path, got ${cmd.command}`,
        );
        assert.ok(Array.isArray(cmd.prefixArgs));
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#1 a .js launcher is routed through the current node executable", () => {
    const d = scratch("launcher");
    const launcher = join(d, "pyths.js");
    writeFileSync(launcher, "// launcher\n");
    try {
        const cmd = resolvePythsCommand({ pythsBin: launcher });
        assert.equal(cmd.command, process.execPath);
        assert.deepEqual(cmd.prefixArgs, [launcher]);
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#1 an explicit pythsBin that does not exist is rejected, not silently downgraded", () => {
    assert.throws(
        () => resolvePythsCommand({ pythsBin: join(scratch("missing"), "nope.exe") }),
        /not found/i,
    );
});

// ===========================================================================
// #2 — CWE-59 symlink follow / unowned overwrite on the .d.ts sibling
// ===========================================================================

test("#2 writeGeneratedSibling refuses to write through a symlink", (t) => {
    const d = scratch("symlink");
    try {
        const victim = join(d, "victim.txt");
        writeFileSync(victim, "ORIGINAL-SECRET");
        const link = join(d, "Foo.d.ps.ts");
        if (!trySymlink(victim, link)) {
            t.skip("OS denied symlink creation (Windows without developer mode)");
            return;
        }
        assert.throws(() => writeGeneratedSibling(link, marked("export {};"), {}), /symlink/i);
        assert.equal(readFileSync(victim, "utf-8"), "ORIGINAL-SECRET");
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#2 writeGeneratedSibling refuses to clobber an unmarked file (hardlink vector)", () => {
    // A HARDLINK is the unprivileged Windows equivalent of the symlink vector:
    // `Foo.d.ps.ts` hardlinked onto a victim means writing the declaration
    // rewrites the victim's contents. lstat() cannot see it as a link, but the
    // nlink>1 guard refuses it directly (and, where nlink is unavailable, the
    // ownership/marker gate is the fallback) — which is why both controls exist.
    const d = scratch("hardlink");
    try {
        const victim = join(d, "victim.txt");
        writeFileSync(victim, "ORIGINAL-SECRET");
        const link = join(d, "Foo.d.ps.ts");
        try {
            linkSync(victim, link);
        } catch {
            return; // no hardlink support here; the marker test below still covers it
        }
        // Refused either by the hard-link guard (nlink>1) or, as a fallback,
        // the unmarked-file marker gate; both preserve the victim.
        assert.throws(() => writeGeneratedSibling(link, marked("export {};"), {}), /hard-linked|did not generate/i);
        assert.equal(readFileSync(victim, "utf-8"), "ORIGINAL-SECRET");
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("round-4: a MARKED hardlinked file is still refused (link count must be exactly 1)", () => {
    // Round-3 should-fix: the fd guard now requires nlink === 1 EXACTLY (an
    // unlink-after-open race yields nlink === 0 — an orphan fd that must not
    // count as success). The reachable regression via the public API is the
    // marked-hardlink case: the marker gate alone would ALLOW this overwrite,
    // so only the link-count guard protects the other name.
    const d = scratch("hardlink_marked");
    try {
        const victim = join(d, "backup.d.ts");
        writeFileSync(victim, marked("shared contents"));
        const link = join(d, "Foo.d.ps.ts");
        try {
            linkSync(victim, link);
        } catch {
            return; // no hardlink support on this filesystem
        }
        assert.throws(
            () => writeGeneratedSibling(link, marked("new"), {}),
            /hard-link/i,
        );
        assert.equal(readFileSync(victim, "utf-8"), marked("shared contents"));
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#2 writeGeneratedSibling refuses a hand-written .d.ps.ts, overwrites its own", () => {
    const d = scratch("marker");
    try {
        const p = join(d, "Foo.d.ps.ts");
        writeFileSync(p, "export declare const mine: number; // hand-written");
        assert.throws(() => writeGeneratedSibling(p, marked("a"), {}), /did not generate/i);
        assert.equal(readFileSync(p, "utf-8"), "export declare const mine: number; // hand-written");

        // Our own marked output IS overwritten.
        writeFileSync(p, marked("old"));
        writeGeneratedSibling(p, marked("new"), {});
        assert.equal(readFileSync(p, "utf-8"), marked("new"));

        // Fresh creation is always allowed.
        const fresh = join(d, "Bar.d.ps.ts");
        writeGeneratedSibling(fresh, marked("x"), {});
        assert.equal(readFileSync(fresh, "utf-8"), marked("x"));
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

// ===========================================================================
// #6 — CWE-73 private per-invocation output directory
// ===========================================================================

test("#6 makePrivateTempDir yields a fresh, unique directory each call", () => {
    const a = makePrivateTempDir("unit");
    const b = makePrivateTempDir("unit");
    try {
        assert.notEqual(a, b);
        assert.ok(existsSync(a) && existsSync(b));
    } finally {
        removePrivateTempDir(a);
        removePrivateTempDir(b);
    }
    assert.ok(!existsSync(a), "removePrivateTempDir must remove the directory");
});

// ── Round-2 review fixes (P1 / P4) ─────────────────────────────────────────
test("P4 looksPythsGenerated requires the marker as the LEADING comment", () => {
    // Our real output is owned.
    assert.ok(looksPythsGenerated(`// ${GENERATED_MARKER} — do not edit.\nconst x = 1;\n`));
    // A doc file that merely MENTIONS the phrase must NOT be treated as owned.
    assert.ok(!looksPythsGenerated(
        `// Preserve the literal "${GENERATED_MARKER}" for documentation\nconst mine = 1;\n`));
    // The marker not at column 0 of the first line does not count.
    assert.ok(!looksPythsGenerated(`const x = 1; // ${GENERATED_MARKER}\n`));
});

test("P1 resolvePythsCommand rejects a RELATIVE PYTHS_DEV_BIN", () => {
    const saved = process.env.PYTHS_DEV_BIN;
    try {
        process.env.PYTHS_DEV_BIN = "target/release/pyths"; // relative → cwd-exec hole
        assert.throws(() => resolvePythsCommand({}), /ABSOLUTE path/i);
    } finally {
        if (saved === undefined) delete process.env.PYTHS_DEV_BIN;
        else process.env.PYTHS_DEV_BIN = saved;
    }
});

// ── Final round: PYTHS_BIN / pythsBin overrides are ABSOLUTE-ONLY ───────────
test("final: a RELATIVE PYTHS_BIN override is refused, never cwd-resolved", () => {
    const d = scratch("relbin");
    // Plant a real file so cwd-resolution WOULD have succeeded before the fix.
    writeFileSync(join(d, "evil.exe"), "");
    const oldCwd = process.cwd();
    const saved = process.env.PYTHS_BIN;
    process.chdir(d);
    try {
        const bad = ["./evil.exe", ".\\evil.exe", "sub/evil.exe"];
        if (WINDOWS) bad.push("C:evil.exe", "\\evil.exe", "prog:ads.exe");
        for (const rel of bad) {
            process.env.PYTHS_BIN = rel;
            assert.throws(
                () => resolvePythsCommand({}),
                /ABSOLUTE/i,
                `override ${JSON.stringify(rel)} must be refused`,
            );
        }
        // The pythsBin plugin option follows the same rule.
        assert.throws(() => resolvePythsCommand({ pythsBin: "./evil.exe" }), /ABSOLUTE/i);
        // A truly absolute override is still honoured.
        process.env.PYTHS_BIN = join(d, "evil.exe");
        const cmd = resolvePythsCommand({});
        assert.ok(
            cmd.command.includes("/") || cmd.command.includes("\\"),
            `absolute override must resolve, got ${cmd.command}`,
        );
    } finally {
        process.chdir(oldCwd);
        if (saved === undefined) delete process.env.PYTHS_BIN;
        else process.env.PYTHS_BIN = saved;
        rmSync(d, { recursive: true, force: true });
    }
});

// ── #442: forward-slash / mixed-separator UNC parity with Rust std::path ───
test("#442: forward-slash and mixed-separator UNC overrides are classified like Rust", () => {
    if (process.platform !== "win32") return; // Windows path-shape semantics only
    const saved = process.env.PYTHS_BIN;
    try {
        // Rust `Path::is_absolute` accepts a UNC prefix spelled with EITHER
        // separator: these are absolute (anchored to \\server\share), so the
        // resolver must accept the SHAPE. The file does not exist, so the
        // accepted shape surfaces as "not found at the configured path" —
        // NOT as the refused-as-relative "must be an ABSOLUTE path" error.
        const acceptedShapes = [
            "//server/share/pyths.exe", // forward-slash UNC
            "\\\\server/share/pyths.exe", // mixed: backslash prefix, fwd body
            "/\\server\\share\\pyths.exe", // mixed: fwd+back prefix
            "\\/server/share/pyths.exe", // mixed: back+fwd prefix
        ];
        for (const p of acceptedShapes) {
            process.env.PYTHS_BIN = p;
            assert.throws(
                () => resolvePythsCommand({}),
                /not found at the configured path/,
                `${JSON.stringify(p)} must be accepted as absolute (Rust parity)`,
            );
        }
        // Rust parity the OTHER way: an incomplete UNC (no share, empty
        // share, or a doubled separator before the share) parses as RootDir
        // in Rust — current-drive-relative, NOT absolute — and must be
        // refused as non-absolute, never honored.
        const refusedShapes = [
            "//server/", // no share
            "//server//x.exe", // empty share component
            "///a/b.exe", // empty server component
            "\\\\server", // no share (backslash spelling)
            "\\\\server\\\\share\\x.exe", // doubled separator before share
        ];
        for (const p of refusedShapes) {
            process.env.PYTHS_BIN = p;
            assert.throws(
                () => resolvePythsCommand({}),
                /ABSOLUTE/i,
                `${JSON.stringify(p)} must be refused as non-absolute (Rust parity)`,
            );
        }
        // Verbatim (`\\?\C:\…`) fits the same server/share shape and still
        // resolves a REAL file — positive control that the tightened regex
        // did not break the verbatim family.
        const d = scratch("uncverbatim");
        const exe = join(d, "pyths_probe.exe");
        writeFileSync(exe, "x");
        process.env.PYTHS_BIN = "\\\\?\\" + exe;
        const cmd = resolvePythsCommand({});
        assert.ok(
            cmd.command.toLowerCase().endsWith("pyths_probe.exe"),
            `verbatim override must still resolve, got ${cmd.command}`,
        );
        rmSync(d, { recursive: true, force: true });
    } finally {
        if (saved === undefined) delete process.env.PYTHS_BIN;
        else process.env.PYTHS_BIN = saved;
    }
});

test("final: searchPath refuses a non-bare name and relative PATH entries", () => {
    // A non-bare name never reaches the PATH loop; only a truly absolute
    // executable is honoured.
    assert.equal(searchPath("./evil"), null);
    assert.equal(searchPath("sub/evil"), null);
    if (WINDOWS) {
        assert.equal(searchPath("C:evil.exe"), null);
        assert.equal(searchPath("\\evil.exe"), null);
    }
    // A PATH consisting only of a relative entry resolves nothing, even when
    // the file exists relative to the cwd.
    const d = scratch("relpath");
    const sub = join(d, "tools");
    const exe = join(sub, WINDOWS ? "pyths_rel_probe.exe" : "pyths_rel_probe");
    mkdirSync(sub);
    copyFileSync(process.execPath, exe);
    const oldCwd = process.cwd();
    const savedPath = process.env.PATH;
    process.chdir(d);
    try {
        process.env.PATH = "tools";
        assert.equal(searchPath("pyths_rel_probe"), null);
    } finally {
        process.chdir(oldCwd);
        process.env.PATH = savedPath;
        rmSync(d, { recursive: true, force: true });
    }
});

// ── Round-3 delta fixes (P8 already-suffixed name; P4 leading-marker) ───────
test("P8 searchPath resolves an already-.exe/.cmd name regardless of PATHEXT", () => {
    if (process.platform !== "win32") return; // Windows PATHEXT semantics only
    const dir = scratch("p8");
    writeFileSync(join(dir, "pyths.exe"), "x");
    const savedPath = process.env.PATH;
    const savedExt = process.env.PATHEXT;
    try {
        process.env.PATH = dir;
        process.env.PATHEXT = ".CMD"; // deliberately does NOT list .EXE
        assert.equal(searchPath("pyths.exe"), join(dir, "pyths.exe"));
    } finally {
        process.env.PATH = savedPath;
        if (savedExt === undefined) delete process.env.PATHEXT;
        else process.env.PATHEXT = savedExt;
        rmSync(dir, { recursive: true, force: true });
    }
});
