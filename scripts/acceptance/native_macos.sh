#!/usr/bin/env bash
# M4 native macOS acceptance leg (spec 13-09-26, plan M4 topology rev-6..8; validation §A native-leg boundary).
#
#   bash scripts/acceptance/native_macos.sh <phase>
#   phases, IN ORDER (each is its own `run:` step in release.yml; NO `uses:` step between scrub and restore):
#     acquire  (network on)  users `app` (unprivileged) + `ctl`; the controller (playwright + chromium) installed
#                            AS ctl into ~ctl (0700: venv, driver dir, browser cache); backup of <runner>/externals
#     scrub    (window open) hosted/system Node deleted from app's reach; the runner's OWN externals/ made
#                            inaccessible to app BY PERMISSION (never deleted); pf egress block for user app
#     a0       (admin)       PATH check + every node file on the box is under ~ctl or externals/ AND fails the app probe
#     app      (as app)      venv from the wheelhouse (offline), A1..A4 via wheel_acceptance.py run
#     a4b      (admin)       fail-closed boundary check (native_node_boundary.py check --step A4b) + shared-temp check
#     a5-app   (as app)      [gradio,server] from the wheelhouse; build the demo kernel; serve on 127.0.0.1:7860
#     a5-ctl   (as ctl)      Chromium drives the app over loopback TCP only -> a5.json
#     a5b      (admin)       boundary re-check (--step A5b) + the diagnostic sampler log
#     stop                   stop the app (the leg's node_free verdict is fixed from a0/a4b/a5b outputs)
#     restore  (window end)  externals/ re-provisioned from the backup (perms reset) BEFORE any `uses:` step
#
# Required env: ACC_TARGET (triple), ACC_WHEEL (basename of W), GITHUB_WORKSPACE, RUNNER_TEMP, PLAYWRIGHT_VERSION.
# Stage layout (world-traversable, admin-owned, read-only for app/ctl): $ACC/stage/{dist,wheelhouse,scripts,
# release_manifest.json,hello.py,gradio/}; outputs: $ACC/out (admin), $ACC/out-app (app), $ACC/out-ctl (ctl).
set -euo pipefail

PHASE="${1:?phase}"
ACC=/private/tmp/acc
STAGE="$ACC/stage"
OUT="$ACC/out"
OUT_APP="$ACC/out-app"
OUT_CTL="$ACC/out-ctl"
APP_HOME=/Users/app
CTL_HOME=/Users/ctl
PY="$(command -v python3)"
export ACC_APP_USER=app ACC_CTL_USER=ctl
SAMPLER_LOG="$OUT/sampler.log"

log() { echo "::group::[native_macos:$PHASE] $*"; }
endlog() { echo "::endgroup::"; }

runner_externals() {
  # The Actions runner's OWN runtime dir (<runner>/externals) -- located from the live worker process, with a
  # filesystem search as the fallback. It is never deleted, only made inaccessible to `app` by permission.
  local worker root
  worker="$(ps -axo command= | grep -m1 '[R]unner.Worker' | awk '{print $1}' || true)"
  if [ -n "$worker" ] && [ -d "$(dirname "$worker")/../externals" ]; then
    root="$(cd "$(dirname "$worker")/.." && pwd)"; echo "$root/externals"; return
  fi
  find /Users/runner -maxdepth 4 -type d -name externals 2>/dev/null | head -1
}

runner_node_roots() {
  # EVERY runner-OWNED dir that legitimately holds a node interpreter -- the resolved externals PLUS the
  # drift the current runner image adds (extra versioned `externals` under the runner home; the shared
  # /opt/runner-cache). These are the runner's OWN node, denied to `app` by the scrub's chmod; A0 allows
  # them (only a node the APP can reach is a real leak). Enumerated (not a single path) because the runner
  # image scatters node across several of these and the set drifts between images.
  { [ -f "$ACC/externals.path" ] && cat "$ACC/externals.path"
    find /Users/runner/actions-runner -maxdepth 3 -type d -name externals 2>/dev/null
    [ -d /opt/runner-cache/externals ] && echo /opt/runner-cache/externals
    # the acquire step's `cp -a` of externals -> $ACC/externals.bak PRESERVES app-readable perms, so the
    # backup's node is reachable by absolute path; treat it as a runner node root so scrub locks it and A0
    # allow-lists+PROBES it (a reachable one still RED) instead of it being skipped/hidden (codex 2026-09-17).
    [ -d "$ACC/externals.bak" ] && echo "$ACC/externals.bak"
  } | sort -u
}

as_app() { sudo -n -u app -H env HOME="$APP_HOME" TMPDIR="$APP_HOME/tmp" PATH="$APP_HOME/venv/bin:/usr/bin:/bin:/usr/sbin:/sbin" "$@"; }
as_ctl() { sudo -n -u ctl -H env HOME="$CTL_HOME" TMPDIR="$CTL_HOME/tmp" PATH="/usr/bin:/bin:/usr/sbin:/sbin" PLAYWRIGHT_BROWSERS_PATH="$CTL_HOME/ms-playwright" "$@"; }

case "$PHASE" in
  acquire)
    log "users + controller (as ctl, 0700) + externals backup"
    sudo mkdir -p "$ACC" "$STAGE" "$OUT" "$OUT_APP" "$OUT_CTL"
    for u in app ctl; do
      if ! id "$u" >/dev/null 2>&1; then
        sudo sysadminctl -addUser "$u" -fullName "acceptance $u" -password "acc-$u-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')" -home "/Users/$u" >/dev/null
      fi
      sudo mkdir -p "/Users/$u/tmp"; sudo chown -R "$u" "/Users/$u"; sudo chmod -R go-rwx "/Users/$u"; sudo chmod 700 "/Users/$u" "/Users/$u/tmp"
    done
    sudo chown app "$OUT_APP"; sudo chmod 700 "$OUT_APP"
    sudo chown ctl "$OUT_CTL"; sudo chmod 700 "$OUT_CTL"
    # stage (read-only for everyone but admin): the candidate + wheelhouse + scripts + fixtures
    sudo cp -R "$GITHUB_WORKSPACE/dist" "$STAGE/dist"
    sudo cp -R "$GITHUB_WORKSPACE/wheelhouse" "$STAGE/wheelhouse"
    sudo mkdir -p "$STAGE/scripts" "$STAGE/gradio"
    sudo cp "$GITHUB_WORKSPACE"/scripts/{wheel_acceptance.py,wheel_acceptance_controller.py,native_node_boundary.py} "$STAGE/scripts/"
    sudo cp "$GITHUB_WORKSPACE/release_manifest.json" "$STAGE/release_manifest.json"
    sudo cp "$GITHUB_WORKSPACE/examples/hello.py" "$STAGE/hello.py"
    sudo cp "$GITHUB_WORKSPACE"/examples/gradio-wasm/{app.py,kernels.py} "$STAGE/gradio/"
    sudo chmod -R a+rX "$STAGE"
    # the controller, as ctl, INSIDE ~ctl (0700 on the venv, the driver dir and the browser cache)
    as_ctl "$PY" -m venv "$CTL_HOME/venv"
    as_ctl "$CTL_HOME/venv/bin/pip" install --quiet "playwright==$PLAYWRIGHT_VERSION"
    as_ctl "$CTL_HOME/venv/bin/playwright" install chromium
    sudo chmod -R go-rwx "$CTL_HOME"
    EXT="$(runner_externals)"; [ -n "$EXT" ] && [ -d "$EXT" ] || { echo "::error::runner externals/ not found"; exit 1; }
    echo "$EXT" | sudo tee "$ACC/externals.path" >/dev/null
    sudo rm -rf "$ACC/externals.bak"; sudo cp -a "$EXT" "$ACC/externals.bak"
    echo "externals=$EXT"; endlog ;;

  scrub)
    log "scrub hosted/system Node from app's reach; externals by permission; pf egress block for app"
    sudo rm -rf "${RUNNER_TOOL_CACHE:-/Users/runner/hostedtoolcache}/node" /usr/local/bin/node /usr/local/bin/npm /usr/local/bin/npx \
      /usr/local/lib/node_modules /usr/local/Cellar/node* /opt/homebrew/bin/node /opt/homebrew/bin/npm /opt/homebrew/bin/npx \
      /opt/homebrew/opt/node* /opt/homebrew/Cellar/node* /opt/homebrew/lib/node_modules /Users/runner/.nvm /Users/runner/.npm 2>/dev/null || true
    # owner (runner) keeps its interpreter(s); app/ctl cannot read or exec them. Cover EVERY runner node
    # root, not just the resolved externals -- the current runner image also ships node under other
    # externals dirs + /opt/runner-cache, and A0 requires each allowed-root node to be app-denied.
    for r in $(runner_node_roots); do [ -d "$r" ] && sudo chmod -R go-rwx "$r"; done
    printf 'block drop out quick user app to ! 127.0.0.0/8\n' | sudo pfctl -q -ef - || sudo pfctl -q -e || true
    endlog ;;

  a0)
    log "A0: explicit PATH check + every node file under ~ctl/externals AND refused by the app probe"
    if command -v node >/dev/null 2>&1; then echo "::error::node leaked onto PATH: $(command -v node)"; exit 1; fi
    if command -v npm  >/dev/null 2>&1; then echo "::error::npm leaked onto PATH: $(command -v npm)"; exit 1; fi
    if command -v npx  >/dev/null 2>&1; then echo "::error::npx leaked onto PATH: $(command -v npx)"; exit 1; fi
    EXT="$(cat "$ACC/externals.path")"
    sudo -E "$PY" "$STAGE/scripts/native_node_boundary.py" check --step A0 --allow-under "$CTL_HOME" $(runner_node_roots) --out "$OUT/a0.json"
    endlog ;;

  app)
    log "A1..A4 as app (offline venv from the wheelhouse; cwd outside the checkout; only hello.py staged)"
    W="$STAGE/dist/$ACC_WHEEL"
    as_app "$PY" -m venv "$APP_HOME/venv"
    as_app "$APP_HOME/venv/bin/pip" install --quiet --no-index --find-links "$STAGE/wheelhouse" "${W}[server]"
    as_app mkdir -p "$APP_HOME/cwd"
    as_app cp "$STAGE/hello.py" "$STAGE/scripts/wheel_acceptance.py" "$APP_HOME/cwd/"
    as_app "$APP_HOME/venv/bin/pyths" --version   # Docker-executability control: the candidate runs NATIVELY here
    (cd "$APP_HOME/cwd" && as_app "$APP_HOME/venv/bin/python" "$APP_HOME/cwd/wheel_acceptance.py" run \
        --manifest "$STAGE/release_manifest.json" --target "$ACC_TARGET" --checkout "$GITHUB_WORKSPACE" \
        --hello "$APP_HOME/cwd/hello.py" --out "$OUT_APP/leg.json")
    endlog ;;

  a4b)
    log "A4b: fail-closed boundary check (every node file probed as app) + shared-temp check"
    EXT="$(cat "$ACC/externals.path")"
    sudo -E "$PY" "$STAGE/scripts/native_node_boundary.py" check --step A4b --allow-under "$CTL_HOME" $(runner_node_roots) --app-tmp "$APP_HOME/tmp" --out "$OUT/a4b.json"
    endlog ;;

  a5-app)
    log "A5 (app side): [gradio,server] from the wheelhouse; build the demo kernel node-free; serve on loopback"
    W="$STAGE/dist/$ACC_WHEEL"
    as_app "$APP_HOME/venv/bin/pip" install --quiet --no-index --find-links "$STAGE/wheelhouse" "${W}[gradio,server]"
    as_app mkdir -p "$APP_HOME/gradio"
    as_app cp "$STAGE/gradio/app.py" "$STAGE/gradio/kernels.py" "$APP_HOME/gradio/"
    (cd "$APP_HOME/gradio" && as_app "$APP_HOME/venv/bin/pyths" build kernels.py)
    sudo -n -u app -H env HOME="$APP_HOME" TMPDIR="$APP_HOME/tmp" PATH="$APP_HOME/venv/bin:/usr/bin:/bin" GRADIO_SERVER_NAME=127.0.0.1 GRADIO_SERVER_PORT=7860 \
      bash -c "cd '$APP_HOME/gradio' && nohup '$APP_HOME/venv/bin/python' app.py > '$OUT_APP/app.log' 2>&1 & echo \$! > '$OUT_APP/app.pid'"
    # diagnostic sampler (never the verdict): any process named node* during A5
    sudo bash -c "while true; do ps -axo comm= | grep -i '^node' >> '$SAMPLER_LOG' || true; sleep 0.2; done" &
    echo $! | sudo tee "$OUT/sampler.pid" >/dev/null
    endlog ;;

  a5-ctl)
    log "A5 (controller side, as ctl): Chromium over loopback TCP only"
    as_ctl "$CTL_HOME/venv/bin/python" "$STAGE/scripts/wheel_acceptance_controller.py" --url http://127.0.0.1:7860 --out "$OUT_CTL/a5.json"
    endlog ;;

  a5b)
    log "A5b: boundary re-check after A5 (permission drift) + the sampler log"
    EXT="$(cat "$ACC/externals.path")"
    sudo -E "$PY" "$STAGE/scripts/native_node_boundary.py" check --step A5b --allow-under "$CTL_HOME" $(runner_node_roots) --app-tmp "$APP_HOME/tmp" --out "$OUT/a5b.json"
    sudo touch "$SAMPLER_LOG"
    sudo "$PY" - "$OUT/a5b.json" "$SAMPLER_LOG" <<'EOF'
import json, sys
p, log = sys.argv[1], sys.argv[2]
j = json.load(open(p)); j["sampler_hits"] = open(log).read().splitlines(); json.dump(j, open(p, "w"), indent=2, sort_keys=True)
EOF
    endlog ;;

  stop)
    log "stop the app + sampler; collect the leg outputs (node_free is fixed from a0/a4b/a5b)"
    sudo kill "$(sudo cat "$OUT/sampler.pid" 2>/dev/null)" 2>/dev/null || true
    sudo kill "$(sudo cat "$OUT_APP/app.pid" 2>/dev/null)" 2>/dev/null || true
    LEG="$GITHUB_WORKSPACE/legs/$ACC_TARGET"; mkdir -p "$LEG"
    sudo cp "$OUT"/*.json "$LEG/" 2>/dev/null || true
    sudo cp "$OUT_APP"/leg.json "$LEG/" 2>/dev/null || true
    sudo cp "$OUT_CTL"/a5.json "$LEG/" 2>/dev/null || true
    sudo cp "$GITHUB_WORKSPACE/bind.json" "$LEG/bind.json"
    sudo chown -R "$(id -u)" "$LEG"
    ls -la "$LEG"; endlog ;;

  restore)
    log "restore the runner runtime (externals/ from the backup; perms reset) BEFORE the first post-window uses: step"
    EXT="$(cat "$ACC/externals.path")"
    sudo rsync -a "$ACC/externals.bak/" "$EXT/"                 # restore the backed-up (resolved) externals content
    # Reset perms on EVERY root the scrub locked down, not just the resolved externals -- otherwise a
    # post-window `uses:` (JS) action whose node lives in a drifted externals dir runs against a
    # still-go-rwx interpreter. Symmetric with the scrub loop above.
    for r in $(runner_node_roots); do [ -d "$r" ] && sudo chmod -R go+rX "$r"; done
    sudo pfctl -q -d 2>/dev/null || true
    "$EXT"/node*/bin/node --version | head -1   # LOUD: the runner's interpreter must run again
    endlog ;;

  *) echo "unknown phase $PHASE"; exit 2 ;;
esac
