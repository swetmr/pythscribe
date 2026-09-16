# M4 native Windows acceptance leg (spec 13-09-26, plan M4 topology rev-6..8; validation §A native-leg boundary).
#
#   pwsh -File scripts/acceptance/native_windows.ps1 -Phase <phase>
#   phases, IN ORDER (each is its own `run:` step in release.yml; NO `uses:` step between scrub and restore):
#     acquire  (network on)  local users `app` + `ctl`; the controller installed AS ctl under C:\acc\ctl with an
#                            explicit ACL DENYING app Read/Execute/Traverse (deny wins); externals backup
#     scrub    (window open) Program Files\nodejs, the toolcache Node, chocolatey Node deleted; the runner's OWN
#                            externals\ made inaccessible to app by a deny-ACL (never deleted); firewall egress block
#     a0       (admin)       PATH check + every node.exe on the box under C:\acc\ctl or externals\ AND refused by app
#     app      (as app)      venv from the wheelhouse (offline), A1..A4 via wheel_acceptance.py run
#     a4b      (admin)       fail-closed boundary check: REAL exec-as-app via CreateProcessWithLogonW must fail
#                            WinError 5 (os.access is DACL-blind and is never consulted) + shared-temp check
#     a5-app / a5-ctl / a5b / stop / restore   as on macOS
#
# Required env: ACC_TARGET, ACC_WHEEL, GITHUB_WORKSPACE, PLAYWRIGHT_VERSION. Credentials for the two users are
# generated at acquire and exported to $GITHUB_ENV (ACC_APP_PASSWORD / ACC_CTL_PASSWORD) for the later steps.
param([Parameter(Mandatory = $true)][string]$Phase)
$ErrorActionPreference = "Stop"
$ACC = "C:\acc"; $STAGE = "$ACC\stage"; $OUT = "$ACC\out"; $OUT_APP = "$ACC\out-app"; $OUT_CTL = "$ACC\out-ctl"
$APP_HOME = "$ACC\app"; $CTL_HOME = "$ACC\ctl"
$PY = (Get-Command python).Source
$env:ACC_APP_USER = "app"; $env:ACC_CTL_USER = "ctl"

function Cred([string]$user, [string]$pw) {
  New-Object System.Management.Automation.PSCredential($user, (ConvertTo-SecureString $pw -AsPlainText -Force))
}
function RunAs([string]$user, [string]$pw, [string]$cmdFile, [string]$cwd, [switch]$NoWait) {
  # A REAL logon (CreateProcessWithLogonW via Start-Process -Credential): the kernel applies the user's token.
  $args = @{ FilePath = "cmd.exe"; ArgumentList = @("/c", "`"$cmdFile`""); Credential = (Cred $user $pw); WorkingDirectory = $cwd; WindowStyle = "Hidden"; PassThru = $true }
  if (-not $NoWait) { $args["Wait"] = $true }
  $p = Start-Process @args
  if (-not $NoWait -and $p.ExitCode -ne 0) { throw "$cmdFile as $user exited $($p.ExitCode)" }
  return $p
}
function WriteCmd([string]$path, [string[]]$lines) { Set-Content -Path $path -Value ($lines -join "`r`n") -Encoding ascii }
function RunnerExternals {
  $w = Get-Process Runner.Worker -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($w -and $w.Path) { $root = Split-Path (Split-Path $w.Path -Parent) -Parent; if (Test-Path "$root\externals") { return "$root\externals" } }
  $hit = Get-ChildItem -Path "C:\" -Directory -Filter externals -Recurse -Depth 3 -ErrorAction SilentlyContinue | Where-Object { Test-Path "$($_.FullName)\node*\bin\node.exe" } | Select-Object -First 1
  if ($hit) { return $hit.FullName }
  throw "runner externals\ not found"
}

switch ($Phase) {
  "acquire" {
    Write-Host "::group::[native_windows:acquire] users + controller (as ctl, deny-ACL for app) + externals backup"
    $appPw = "Acc-app-" + [Guid]::NewGuid().ToString("N").Substring(0, 12) + "!"
    $ctlPw = "Acc-ctl-" + [Guid]::NewGuid().ToString("N").Substring(0, 12) + "!"
    foreach ($u in @(@("app", $appPw), @("ctl", $ctlPw))) { net user $u[0] $u[1] /add /y | Out-Null }
    Write-Host "::add-mask::$appPw"; Write-Host "::add-mask::$ctlPw"
    Add-Content -Path $env:GITHUB_ENV -Value "ACC_APP_PASSWORD=$appPw"
    Add-Content -Path $env:GITHUB_ENV -Value "ACC_CTL_PASSWORD=$ctlPw"
    $env:ACC_APP_PASSWORD = $appPw; $env:ACC_CTL_PASSWORD = $ctlPw
    New-Item -ItemType Directory -Force -Path $ACC, $STAGE, $OUT, $OUT_APP, $OUT_CTL, $APP_HOME, "$APP_HOME\tmp", $CTL_HOME, "$CTL_HOME\tmp" | Out-Null
    # stage: readable by everyone (Users), writable by admin only
    Copy-Item -Recurse "$env:GITHUB_WORKSPACE\dist" "$STAGE\dist"
    Copy-Item -Recurse "$env:GITHUB_WORKSPACE\wheelhouse" "$STAGE\wheelhouse"
    New-Item -ItemType Directory -Force -Path "$STAGE\scripts", "$STAGE\gradio" | Out-Null
    foreach ($f in @("wheel_acceptance.py", "wheel_acceptance_controller.py", "native_node_boundary.py")) { Copy-Item "$env:GITHUB_WORKSPACE\scripts\$f" "$STAGE\scripts\" }
    Copy-Item "$env:GITHUB_WORKSPACE\release_manifest.json" "$STAGE\release_manifest.json"
    Copy-Item "$env:GITHUB_WORKSPACE\examples\hello.py" "$STAGE\hello.py"
    Copy-Item "$env:GITHUB_WORKSPACE\examples\gradio-wasm\app.py", "$env:GITHUB_WORKSPACE\examples\gradio-wasm\kernels.py" "$STAGE\gradio\"
    icacls $STAGE /inheritance:r /grant "Administrators:(OI)(CI)F" /grant "Users:(OI)(CI)RX" | Out-Null
    # app's private area: only app (and admin); ctl explicitly denied (shared-temp control)
    icacls $APP_HOME /inheritance:r /grant "Administrators:(OI)(CI)F" /grant "app:(OI)(CI)F" /deny "ctl:(OI)(CI)F" | Out-Null
    icacls $OUT_APP /inheritance:r /grant "Administrators:(OI)(CI)F" /grant "app:(OI)(CI)F" | Out-Null
    icacls $OUT_CTL /inheritance:r /grant "Administrators:(OI)(CI)F" /grant "ctl:(OI)(CI)F" | Out-Null
    # the controller AS ctl inside C:\acc\ctl: venv + driver + browser cache all under the deny-ACL for app
    icacls $CTL_HOME /inheritance:r /grant "Administrators:(OI)(CI)F" /grant "ctl:(OI)(CI)F" /deny "app:(OI)(CI)F" | Out-Null
    WriteCmd "$CTL_HOME\acquire.cmd" @(
      "set PLAYWRIGHT_BROWSERS_PATH=$CTL_HOME\ms-playwright",
      "`"$PY`" -m venv $CTL_HOME\venv || exit /b 1",
      "$CTL_HOME\venv\Scripts\pip.exe install --quiet playwright==$env:PLAYWRIGHT_VERSION || exit /b 1",
      "$CTL_HOME\venv\Scripts\playwright.exe install chromium || exit /b 1")
    RunAs "ctl" $ctlPw "$CTL_HOME\acquire.cmd" $CTL_HOME | Out-Null
    $EXT = RunnerExternals
    Set-Content -Path "$ACC\externals.path" -Value $EXT -Encoding ascii
    if (Test-Path "$ACC\externals.bak") { Remove-Item -Recurse -Force "$ACC\externals.bak" }
    Copy-Item -Recurse $EXT "$ACC\externals.bak"
    Write-Host "externals=$EXT"; Write-Host "::endgroup::"
  }
  "scrub" {
    Write-Host "::group::[native_windows:scrub] delete hosted/system Node; externals deny-ACL for app; firewall egress block"
    foreach ($d in @("$env:RUNNER_TOOL_CACHE\node", "C:\Program Files\nodejs", "C:\Program Files (x86)\nodejs", "C:\ProgramData\chocolatey\lib\nodejs", "C:\ProgramData\chocolatey\lib\nodejs.install", "$env:APPDATA\npm", "$env:LOCALAPPDATA\npm-cache")) {
      if (Test-Path $d) { Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue }
    }
    Get-ChildItem "C:\ProgramData\chocolatey\bin" -Filter "node*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    Get-ChildItem "C:\ProgramData\chocolatey\bin" -Filter "np*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    $EXT = Get-Content "$ACC\externals.path"
    icacls $EXT /deny "app:(OI)(CI)F" | Out-Null          # the runner keeps its interpreter; app cannot read or exec it
    $sid = (New-Object System.Security.Principal.NTAccount("app")).Translate([System.Security.Principal.SecurityIdentifier]).Value
    New-NetFirewallRule -DisplayName "acc-app-egress" -Direction Outbound -Action Block -LocalUser "D:(A;;CC;;;$sid)" -Profile Any | Out-Null
    Write-Host "::endgroup::"
  }
  "a0" {
    Write-Host "::group::[native_windows:a0] explicit PATH check + every node.exe under ctl/externals AND refused by app"
    foreach ($n in @("node", "npm", "npx")) { $c = Get-Command $n -ErrorAction SilentlyContinue; if ($c) { Write-Host "::error::$n leaked onto PATH: $($c.Source)"; exit 1 } }
    $EXT = Get-Content "$ACC\externals.path"
    & $PY "$STAGE\scripts\native_node_boundary.py" check --step A0 --allow-under $CTL_HOME $EXT --out "$OUT\a0.json"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "::endgroup::"
  }
  "app" {
    Write-Host "::group::[native_windows:app] A1..A4 as app (offline venv from the wheelhouse; cwd outside the checkout)"
    $W = "$STAGE\dist\$env:ACC_WHEEL"
    New-Item -ItemType Directory -Force -Path "$APP_HOME\cwd" | Out-Null
    Copy-Item "$STAGE\hello.py", "$STAGE\scripts\wheel_acceptance.py" "$APP_HOME\cwd\"
    icacls "$APP_HOME\cwd" /grant "app:(OI)(CI)F" | Out-Null
    WriteCmd "$APP_HOME\run.cmd" @(
      "set TMP=$APP_HOME\tmp", "set TEMP=$APP_HOME\tmp", "set PATH=$APP_HOME\venv\Scripts;C:\Windows\System32;C:\Windows",
      "`"$PY`" -m venv $APP_HOME\venv || exit /b 1",
      "$APP_HOME\venv\Scripts\pip.exe install --quiet --no-index --find-links $STAGE\wheelhouse `"$W[server]`" || exit /b 1",
      "$APP_HOME\venv\Scripts\pyths.exe --version || exit /b 1",
      "cd /d $APP_HOME\cwd",
      "$APP_HOME\venv\Scripts\python.exe $APP_HOME\cwd\wheel_acceptance.py run --manifest $STAGE\release_manifest.json --target $env:ACC_TARGET --checkout $env:GITHUB_WORKSPACE --hello $APP_HOME\cwd\hello.py --out $OUT_APP\leg.json > $OUT_APP\run.log 2>&1 || exit /b 1")
    RunAs "app" $env:ACC_APP_PASSWORD "$APP_HOME\run.cmd" "$APP_HOME\cwd" | Out-Null
    Get-Content "$OUT_APP\run.log" -Tail 40
    Write-Host "::endgroup::"
  }
  "a4b" {
    Write-Host "::group::[native_windows:a4b] fail-closed boundary check (real exec-as-app must fail WinError 5) + shared temp"
    $EXT = Get-Content "$ACC\externals.path"
    & $PY "$STAGE\scripts\native_node_boundary.py" check --step A4b --allow-under $CTL_HOME $EXT --app-tmp "$APP_HOME\tmp" --out "$OUT\a4b.json"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "::endgroup::"
  }
  "a5-app" {
    Write-Host "::group::[native_windows:a5-app] [gradio,server] from the wheelhouse; build the demo kernel; serve on loopback"
    $W = "$STAGE\dist\$env:ACC_WHEEL"
    New-Item -ItemType Directory -Force -Path "$APP_HOME\gradio" | Out-Null
    Copy-Item "$STAGE\gradio\app.py", "$STAGE\gradio\kernels.py" "$APP_HOME\gradio\"
    icacls "$APP_HOME\gradio" /grant "app:(OI)(CI)F" | Out-Null
    WriteCmd "$APP_HOME\a5-prep.cmd" @(
      "set TMP=$APP_HOME\tmp", "set TEMP=$APP_HOME\tmp", "set PATH=$APP_HOME\venv\Scripts;C:\Windows\System32;C:\Windows",
      "$APP_HOME\venv\Scripts\pip.exe install --quiet --no-index --find-links $STAGE\wheelhouse `"$W[gradio,server]`" || exit /b 1",
      "cd /d $APP_HOME\gradio", "$APP_HOME\venv\Scripts\pyths.exe build kernels.py || exit /b 1")
    RunAs "app" $env:ACC_APP_PASSWORD "$APP_HOME\a5-prep.cmd" "$APP_HOME\gradio" | Out-Null
    WriteCmd "$APP_HOME\a5-serve.cmd" @(
      "set TMP=$APP_HOME\tmp", "set TEMP=$APP_HOME\tmp", "set PATH=$APP_HOME\venv\Scripts;C:\Windows\System32;C:\Windows",
      "set GRADIO_SERVER_NAME=127.0.0.1", "set GRADIO_SERVER_PORT=7860",
      "cd /d $APP_HOME\gradio", "$APP_HOME\venv\Scripts\python.exe app.py > $OUT_APP\app.log 2>&1")
    $p = RunAs "app" $env:ACC_APP_PASSWORD "$APP_HOME\a5-serve.cmd" "$APP_HOME\gradio" -NoWait
    Set-Content -Path "$OUT\app.pid" -Value $p.Id
    # diagnostic sampler (never the verdict)
    $sampler = Start-Job -ScriptBlock { param($log) while ($true) { Get-Process -Name "node*" -ErrorAction SilentlyContinue | ForEach-Object { Add-Content -Path $log -Value ("{0} pid={1} name={2}" -f (Get-Date -Format o), $_.Id, $_.ProcessName) }; Start-Sleep -Milliseconds 200 } } -ArgumentList "$OUT\sampler.log"
    Set-Content -Path "$OUT\sampler.job" -Value $sampler.Id
    Write-Host "::endgroup::"
  }
  "a5-ctl" {
    Write-Host "::group::[native_windows:a5-ctl] Chromium as ctl over loopback TCP only"
    WriteCmd "$CTL_HOME\a5.cmd" @(
      "set PLAYWRIGHT_BROWSERS_PATH=$CTL_HOME\ms-playwright", "set TMP=$CTL_HOME\tmp", "set TEMP=$CTL_HOME\tmp",
      "$CTL_HOME\venv\Scripts\python.exe $STAGE\scripts\wheel_acceptance_controller.py --url http://127.0.0.1:7860 --out $OUT_CTL\a5.json > $OUT_CTL\a5.log 2>&1 || exit /b 1")
    RunAs "ctl" $env:ACC_CTL_PASSWORD "$CTL_HOME\a5.cmd" $CTL_HOME | Out-Null
    Get-Content "$OUT_CTL\a5.log" -Tail 40
    Write-Host "::endgroup::"
  }
  "a5b" {
    Write-Host "::group::[native_windows:a5b] boundary re-check after A5 + the sampler log"
    $EXT = Get-Content "$ACC\externals.path"
    & $PY "$STAGE\scripts\native_node_boundary.py" check --step A5b --allow-under $CTL_HOME $EXT --app-tmp "$APP_HOME\tmp" --out "$OUT\a5b.json"
    $rc = $LASTEXITCODE
    if (-not (Test-Path "$OUT\sampler.log")) { New-Item -ItemType File -Path "$OUT\sampler.log" | Out-Null }
    $j = Get-Content "$OUT\a5b.json" -Raw | ConvertFrom-Json
    $j | Add-Member -NotePropertyName sampler_hits -NotePropertyValue @(Get-Content "$OUT\sampler.log") -Force
    $j | ConvertTo-Json -Depth 8 | Set-Content "$OUT\a5b.json" -Encoding utf8
    if ($rc -ne 0) { exit $rc }
    Write-Host "::endgroup::"
  }
  "stop" {
    Write-Host "::group::[native_windows:stop] stop the app + sampler; collect the leg outputs"
    if (Test-Path "$OUT\sampler.job") { Stop-Job -Id (Get-Content "$OUT\sampler.job") -ErrorAction SilentlyContinue }
    if (Test-Path "$OUT\app.pid") { Stop-Process -Id (Get-Content "$OUT\app.pid") -Force -ErrorAction SilentlyContinue }
    Get-Process -Name python -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$APP_HOME\*" } | Stop-Process -Force -ErrorAction SilentlyContinue
    $LEG = "$env:GITHUB_WORKSPACE\legs\$env:ACC_TARGET"
    New-Item -ItemType Directory -Force -Path $LEG | Out-Null
    Copy-Item "$OUT\*.json" $LEG -ErrorAction SilentlyContinue
    Copy-Item "$OUT_APP\leg.json" $LEG -ErrorAction SilentlyContinue
    Copy-Item "$OUT_CTL\a5.json" $LEG -ErrorAction SilentlyContinue
    Copy-Item "$env:GITHUB_WORKSPACE\bind.json" "$LEG\bind.json"
    Get-ChildItem $LEG
    Write-Host "::endgroup::"
  }
  "restore" {
    Write-Host "::group::[native_windows:restore] externals\ from the backup; deny-ACL removed; firewall rule removed -- BEFORE any uses: step"
    $EXT = Get-Content "$ACC\externals.path"
    icacls $EXT /remove:d app /T /C | Out-Null
    Copy-Item -Recurse -Force "$ACC\externals.bak\*" $EXT
    Remove-NetFirewallRule -DisplayName "acc-app-egress" -ErrorAction SilentlyContinue
    $node = Get-ChildItem "$EXT" -Filter node.exe -Recurse | Select-Object -First 1
    & $node.FullName --version   # LOUD: the runner's interpreter must run again
    if ($LASTEXITCODE -ne 0) { exit 1 }
    Write-Host "::endgroup::"
  }
  default { Write-Host "unknown phase $Phase"; exit 2 }
}
