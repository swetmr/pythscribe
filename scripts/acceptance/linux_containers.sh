#!/usr/bin/env bash
# M4 Linux acceptance leg -- APP container + CONTROLLER container (spec 13-09-26, plan M4 topology; validation §A).
#
#   bash scripts/acceptance/linux_containers.sh <phase>
#   phases, IN ORDER (each its own `run:` step in release.yml):
#     acquire  (network on)  pull the app + controller images; build the OFFLINE wheelhouse INSIDE the app image
#                            (`pip download "<W>[server,gradio]"`, deps bound to W's metadata, resolved natively on the
#                            leg's platform -- aarch64 under QEMU); assert no other pythscribe-* in it
#     start                  internal-only docker network; the APP container (python:3.12-slim -- no node anywhere;
#                            networking = the internal net only) with the stage mounted READ-ONLY
#     a0                     explicit `command -v node/npm` exit + `find /` for node files -> any hit exit 1 -> a0.json
#     app                    fresh venv; pip install --no-index --find-links wheelhouse "<abs W>[server]"; A1..A4 from
#                            /work/cwd (outside any checkout; only hello.py + the runner script staged) -> leg.json
#     a4b                    pre-A5 filesystem re-check (container legs: the same absence check) -> a4b.json
#     a5-app                 [gradio,server] from the wheelhouse; build the demo kernel; serve on 0.0.0.0:7860 (internal net)
#     a5-ctl                 the CONTROLLER container (Playwright + Chromium pre-installed, same internal net, no egress)
#                            loads http://app:7860 -> a5.json
#     a5b                    post-A5 re-check: no node file exists + the /proc sampler saw no node process -> a5b.json
#     stop                   collect legs/<triple>/, remove containers + network
#
# Required env: ACC_TARGET, ACC_WHEEL, ACC_PLATFORM (linux/amd64 | linux/arm64), GITHUB_WORKSPACE, PLAYWRIGHT_VERSION.
set -euo pipefail

PHASE="${1:?phase}"
APP_IMAGE="python:3.12-slim"
CTL_IMAGE="mcr.microsoft.com/playwright/python:v${PLAYWRIGHT_VERSION}-noble"
NET="acc-net"
STAGE="$GITHUB_WORKSPACE/acc-stage"
OUT="$GITHUB_WORKSPACE/acc-out"
W_IN="/stage/dist/$ACC_WHEEL"

app() { docker exec "$@"; }

case "$PHASE" in
  acquire)
    echo "::group::[linux:acquire] images + offline wheelhouse inside the app image (network on)"
    docker pull --platform "$ACC_PLATFORM" "$APP_IMAGE"
    docker pull "$CTL_IMAGE"
    mkdir -p "$STAGE/dist" "$STAGE/wheelhouse" "$STAGE/scripts" "$STAGE/gradio" "$OUT"
    cp "$GITHUB_WORKSPACE/dist/$ACC_WHEEL" "$STAGE/dist/"
    cp "$GITHUB_WORKSPACE"/scripts/{wheel_acceptance.py,wheel_acceptance_controller.py} "$STAGE/scripts/"
    cp "$GITHUB_WORKSPACE/release_manifest.json" "$STAGE/release_manifest.json"
    cp "$GITHUB_WORKSPACE/examples/hello.py" "$STAGE/hello.py"
    cp "$GITHUB_WORKSPACE"/examples/gradio-wasm/{app.py,kernels.py} "$STAGE/gradio/"
    # deps resolved by the leg's OWN platform/interpreter, bound to the candidate's metadata
    docker run --rm --platform "$ACC_PLATFORM" -v "$STAGE:/stage" "$APP_IMAGE" \
      pip download --quiet "${W_IN}[server,gradio]" --dest /stage/wheelhouse
    python3 "$GITHUB_WORKSPACE/scripts/wheel_acceptance.py" bind --wheel "$STAGE/dist/$ACC_WHEEL" --manifest "$STAGE/release_manifest.json" \
      --target "$ACC_TARGET" --wheelhouse "$STAGE/wheelhouse" --out "$OUT/bind.json"
    # `pip download` ran as ROOT inside the container (-v mount), so $STAGE/wheelhouse is root-owned and a
    # host-side (runner-user) chmod fails "Operation not permitted". sudo (passwordless on the runner) can
    # chmod files it does not own. $OUT is runner-created, so a plain chmod is fine there.
    sudo chmod -R a+rX "$STAGE"; chmod 777 "$OUT"
    echo "::endgroup::" ;;

  start)
    echo "::group::[linux:start] internal-only network + the app container (no node; stage read-only)"
    docker network create --internal "$NET"
    docker run -d --name app --network "$NET" --platform "$ACC_PLATFORM" \
      -v "$STAGE:/stage:ro" -v "$OUT:/out" -e PIP_NO_CACHE_DIR=1 -e PIP_DISABLE_PIP_VERSION_CHECK=1 \
      "$APP_IMAGE" sleep infinity
    app app sh -c 'mkdir -p /work/cwd /work/gradio'
    echo "::endgroup::" ;;

  a0)
    echo "::group::[linux:a0] node ABSENCE, explicit exit + filesystem find"
    app app sh -c 'if command -v node >/dev/null 2>&1; then echo "::error::node leaked: $(command -v node)"; exit 1; fi
                   if command -v npm  >/dev/null 2>&1; then echo "::error::npm leaked: $(command -v npm)"; exit 1; fi
                   if command -v npx  >/dev/null 2>&1; then echo "::error::npx leaked: $(command -v npx)"; exit 1; fi
                   if find / -xdev -type f \( -name node -o -name node.exe \) 2>/dev/null | grep .; then echo "::error::node file present in the app container"; exit 1; fi
                   python /stage/scripts/wheel_acceptance.py a0 --out /out/a0.json'
    echo "::endgroup::" ;;

  app)
    echo "::group::[linux:app] fresh venv; offline install by ABSOLUTE PATH with extras; A1..A4 from /work/cwd"
    app app sh -c "python -m venv /work/venv && /work/venv/bin/pip list --format=freeze"
    app app sh -c "/work/venv/bin/pip install --quiet --no-index --find-links /stage/wheelhouse '${W_IN}[server]'"
    app app sh -c 'cp /stage/hello.py /stage/scripts/wheel_acceptance.py /work/cwd/'
    app -d app python /stage/scripts/wheel_acceptance.py sampler --out /out/sampler.log
    app -w /work/cwd -e PATH=/work/venv/bin:/usr/local/bin:/usr/bin:/bin app /work/venv/bin/python /work/cwd/wheel_acceptance.py run \
      --manifest /stage/release_manifest.json --target "$ACC_TARGET" --checkout /stage --hello /work/cwd/hello.py --out /out/leg.json
    echo "::endgroup::" ;;

  a4b)
    echo "::group::[linux:a4b] pre-A5 filesystem re-check"
    app app python /stage/scripts/wheel_acceptance.py a0 --out /out/a4b.json
    echo "::endgroup::" ;;

  a5-app)
    echo "::group::[linux:a5-app] [gradio,server] from the wheelhouse; build the demo kernel; serve on the internal net"
    app app sh -c "/work/venv/bin/pip install --quiet --no-index --find-links /stage/wheelhouse '${W_IN}[gradio,server]'"
    app app sh -c 'cp /stage/gradio/app.py /stage/gradio/kernels.py /work/gradio/'
    app -w /work/gradio -e PATH=/work/venv/bin:/usr/local/bin:/usr/bin:/bin app /work/venv/bin/pyths build kernels.py
    app -d -w /work/gradio -e PATH=/work/venv/bin:/usr/local/bin:/usr/bin:/bin -e GRADIO_SERVER_NAME=0.0.0.0 -e GRADIO_SERVER_PORT=7860 app \
      sh -c '/work/venv/bin/python app.py > /out/app.log 2>&1'
    echo "::endgroup::" ;;

  a5-ctl)
    echo "::group::[linux:a5-ctl] the controller container drives http://app:7860 (internal net, no egress)"
    docker run --rm --network "$NET" -v "$STAGE:/stage:ro" -v "$OUT:/out" "$CTL_IMAGE" \
      python /stage/scripts/wheel_acceptance_controller.py --url http://app:7860 --out /out/a5.json
    echo "::endgroup::" ;;

  a5b)
    echo "::group::[linux:a5b] post-A5: no node file + the sampler saw nothing"
    app app sh -c 'if find / -xdev -type f \( -name node -o -name node.exe \) 2>/dev/null | grep .; then echo "::error::node file present after A5"; exit 1; fi
                   python /stage/scripts/wheel_acceptance.py post-a5 --sampler-log /out/sampler.log --out /out/a5b.json'
    echo "::endgroup::" ;;

  stop)
    echo "::group::[linux:stop] collect legs/<triple>/; tear down"
    LEG="$GITHUB_WORKSPACE/legs/$ACC_TARGET"; mkdir -p "$LEG"
    cp "$OUT"/*.json "$LEG/" 2>/dev/null || true
    docker logs app > "$LEG/app-container.log" 2>&1 || true
    docker rm -f app >/dev/null 2>&1 || true
    docker network rm "$NET" >/dev/null 2>&1 || true
    ls -la "$LEG"
    echo "::endgroup::" ;;

  *) echo "unknown phase $PHASE"; exit 2 ;;
esac
