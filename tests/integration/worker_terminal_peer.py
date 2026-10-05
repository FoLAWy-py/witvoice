"""Finite test-only failure peer; never a production engine/capability selector."""
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(root / '.local/t011-leader/python-deps'),
               str(root / 'workers/vc_worker')]
if not sys.flags.isolated or not sys.flags.no_site or len(sys.argv) < 3:
    raise SystemExit(2)
cause = sys.argv[1]
if cause not in ('Failed', 'MemoryPressure'):
    raise SystemExit(2)
sys.argv = [sys.argv[0], *sys.argv[2:]]
import model_process
import runtime
import json
import time

def fail_only(request, diagnostic=False):
    # No model process is created and no Torch/assets are imported.
    if diagnostic:
        raise AssertionError('test must not write phase evidence')
    if cause == 'MemoryPressure':
        raise MemoryError('test-only')
    raise RuntimeError('test-only')

model_process.prepare_isolated = fail_only

class OrderedFailure(runtime.WarmupSession):
    """Test-only barrier fixes ACK -> terminal ordering without timing guesses."""
    def __init__(self, loader):
        super().__init__(loader)
        self._first_heartbeat = False

    def handle(self, payload):
        if json.loads(payload)['command']['kind'] == 'Heartbeat' and self._phase == 'WARMING':
            limit = time.monotonic() + 1
            while self._result.empty() and time.monotonic() < limit:
                time.sleep(.001)
            if self._result.empty():
                raise TimeoutError('test failure task did not complete')
            self._first_heartbeat = True
        return super().handle(payload)

    def poll_ready(self):
        return super().poll_ready() if self._first_heartbeat else []

runtime.WarmupSession = OrderedFailure
raise SystemExit(runtime.main())
