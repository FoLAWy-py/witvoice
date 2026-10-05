$ErrorActionPreference='Stop'
Set-Location 'D:\Project\witvoice'
. '.\tools\dev\toolchain-env.ps1'
$env:CARGO_TARGET_DIR='D:\Project\witvoice\.local\t008-t016-r2-leader\target'
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
& "$env:CARGO_HOME\bin\cargo.exe" check --offline -p witvoice-engines --lib 1> '.local/t011-warmup-root/lock-check.stdout' 2> '.local/t011-warmup-root/lock-check.stderr'
$c=$LASTEXITCODE; Write-Output "lock-check exit=$c"; if($c-ne0){exit $c}
$env:PYTHONPATH='D:\Project\witvoice\.local\t011-leader\python-deps'
& 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m unittest discover -s workers/vc_worker -p 'test_*.py' -v 1> '.local/t011-warmup-root/python-tests.stdout' 2> '.local/t011-warmup-root/python-tests.stderr'
$c=$LASTEXITCODE; Write-Output "python-tests exit=$c";if($c-ne0){exit $c}
& 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m py_compile workers/vc_worker/runtime.py workers/vc_worker/warmup.py workers/vc_worker/test_runtime.py 1> '.local/t011-warmup-root/compile.stdout' 2> '.local/t011-warmup-root/compile.stderr'
Write-Output "python-compile exit=$LASTEXITCODE";exit $LASTEXITCODE