import json
from control import decode_control
r={"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000001","binding":{"session_tag":"1","epoch":1},"event":{"kind":"Ready","args":{"capabilities":{"engine_id":"meanvc2","model_sha256":"a"*64,"backend":"cuda","native_input_rate":16000,"native_output_rate":16000,"chunk_samples":2560,"lookahead_samples":640,"conditioning_schema":"\ud800","duration_preserving":False,"capability_test_run_id":"codec_fixture_not_model","model_memory_budget_bytes":"1","device_memory_budget_bytes":None}}}}
p=json.dumps(r).encode("utf-8")
try:
 decode_control(p,"WorkerResponse")
 print("BEFORE_FIX: Python accepted isolated high surrogate in valid Ready")
except ValueError:
 print("BEFORE_FIX: Python rejected isolated surrogate")