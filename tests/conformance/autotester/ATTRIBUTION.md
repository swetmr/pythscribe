# Attribution — vendored Transcrypt autotester testlets

The conformance testlets in `testlets/` originate from **Transcrypt**
(https://github.com/TranscryptOrg/Transcrypt), licensed under the
**Apache License 2.0**.

- **Upstream source commit**: `32b216bcd3cb08c81645528c1a108a78be0b2325`
- **Testlet path in upstream**: `transcrypt/development/automated_tests/transcrypt/<name>/__init__.py`
- **Porting provenance**: each `testlets/<name>.ps` carries a header listing
  the exact mechanical edits applied (Transcrypt browser-stub import strips
  only; no testlet logic altered). The port was produced by the reference-app
  `experiments/autotester-ps` harness (2026-08-16 full-surface pass) and
  vendored here as the canonical copy for the recurring CI gate
  (`run_autotester.py`); the reference-app copy remains the experiment record.
- **AutoTester shim** (`autotester_shim.py`): our own work — a plain-Python
  reimplementation of the `org.transcrypt.autotester.AutoTester` check/done
  convention, inlined ahead of each testlet so the identical source runs
  under both CPython and `pyths run`.

## Oracle discipline (load-bearing)

The testlets are used as **test programs only**, run as a
**PythScribe-vs-CPython differential — CPython is the oracle, never
Transcrypt's own expected outputs** (Transcrypt's conformance is
pragma-dependent and not a well-posed reference; using its outputs would be
the model↔model trap). See `docs/python-oracle-policy.md` for the oracle pin
and Paper C §transcrypt-oracle for the framing.
