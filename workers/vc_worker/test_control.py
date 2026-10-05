import json
import unittest
from control import decode_control


class ControlTests(unittest.TestCase):
    def base(self):
        return {"protocol_version": 1, "request_id": "00000000-0000-0000-0000-000000000001",
                "binding": {"session_tag": "1", "epoch": 1}, "command": {"kind": "Heartbeat"}}

    def check(self, value): return decode_control(json.dumps(value).encode(), "WorkerRequest")

    def test_rust_shapes_and_stale_binding(self):
        self.assertEqual(self.check(self.base()), self.base())
        warmup = self.base();warmup["command"] = {"kind": "Warmup", "args": {
            "model_sha256": "a"*64, "reference_id": warmup["request_id"], "backend": "Cuda"}}
        self.assertEqual(self.check(warmup), warmup)
        with self.assertRaises(ValueError):
            decode_control(json.dumps(self.base()).encode(), "WorkerRequest", {"session_tag": "1", "epoch": 2})

    def test_json_ambiguity_limits_and_nonfinite(self):
        for bad in (b"", b" "*65537, b'{"x":1,"x":2}', b'{"x":NaN}', b"\xff", b"{", b"[]"):
            with self.subTest(bad=bad[:20]), self.assertRaises(ValueError): decode_control(bad, "WorkerRequest")

    def test_schema_and_rust_integer_semantics(self):
        for key, value in (("epoch", 0), ("epoch", 1<<32), ("epoch", True),
                           ("session_tag", "01"), ("session_tag", "0"),
                           ("session_tag", str(1<<64)), ("session_tag", 1)):
            bad=self.base();bad["binding"][key]=value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError): self.check(bad)
        for key, value in (("protocol_version", 2), ("protocol_version", 1<<16),
                           ("request_id", "x"*36), ("extra", True)):
            bad=self.base();bad[key]=value
            with self.assertRaises(ValueError): self.check(bad)
        for command in ({"kind":"FakeEngine"}, {"kind":"Heartbeat","args":{}},
                        {"kind":"Warmup","args":{"model_sha256":"x"*64,"reference_id":"bad","backend":"Cpu"}}):
            bad=self.base();bad["command"]=command
            with self.assertRaises(ValueError): self.check(bad)

    def test_ready_semantics_match_rust_rejection(self):
        response=self.base();del response["command"]
        cap={"engine_id":"meanvc2","model_sha256":"a"*64,"backend":"cuda",
             "native_input_rate":16000,"native_output_rate":16000,"chunk_samples":2560,
             "lookahead_samples":640,"conditioning_schema":"codec_fixture","duration_preserving":False,
             "capability_test_run_id":"codec_fixture_not_model","model_memory_budget_bytes":"1",
             "device_memory_budget_bytes":None}
        response["event"]={"kind":"Ready","args":{"capabilities":cap}}
        self.assertEqual(decode_control(json.dumps(response).encode(),"WorkerResponse"),response)
        for key,value in (("backend","cpu"),("engine_id","identity"),("native_output_rate",48000),
                          ("chunk_samples",480),("capability_test_run_id",""),("model_sha256","z"*64)):
            saved=cap[key];cap[key]=value
            with self.assertRaises(ValueError): decode_control(json.dumps(response).encode(),"WorkerResponse")
            cap[key]=saved


if __name__ == "__main__": unittest.main()
