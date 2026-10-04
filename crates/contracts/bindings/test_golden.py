"""Cross-language golden bytes; no networking, devices or inference."""
import json
from pathlib import Path
import struct
import unittest


class GoldenTests(unittest.TestCase):
    def test_rust_export_matches_python_network_header(self):
        vector = json.loads(Path(__file__).with_name("media-golden.json").read_text())
        layout = ">BBHQIIQQHH"
        self.assertEqual(struct.calcsize(layout), 40)
        header = struct.pack(layout, 1, vector["kind"], 0, int(vector["session_tag"]), vector["epoch"],
                             vector["sequence"], int(vector["media_sample_index"]),
                             int(vector["source_sample_index"]), vector["sample_count"], 0)
        self.assertEqual(header.hex(), vector["header_hex"])
        self.assertEqual(struct.unpack(layout, header)[3], 0x0102030405060708)
        self.assertEqual(struct.pack("<hh", -32768, 32767).hex(), vector["pcm_sample_hex"])
        self.assertNotEqual(struct.pack(">hh", -32768, 32767).hex(), vector["pcm_sample_hex"])


if __name__ == "__main__":
    unittest.main()
