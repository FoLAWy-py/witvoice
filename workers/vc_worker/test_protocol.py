"""Software codec/framing negatives; no model/device/native-pipe claim."""
import struct
import unittest
from protocol import WIRE, HEADER, MAX_BYTES, decode_pcm, encode_pcm, read_frame, write_frame


class ProtocolTests(unittest.TestCase):
    def source(self):
        return encode_pcm([0.01] * 2560, 0x0102030405060708, 9, 0xfffffffe, 2560, 1)

    def test_rust_golden_and_float_endian(self):
        packet = self.source()
        self.assertEqual(packet[:40].hex(), WIRE["golden_header_hex"])
        self.assertEqual(packet[40:44], struct.pack("<f", 0.01))
        header, samples = decode_pcm(packet, 0x0102030405060708, 9, 1)
        self.assertEqual(header["sequence"], 0xfffffffe)
        self.assertEqual(len(samples), 2560)

    def test_corruption_stale_nonfinite_length(self):
        p = self.source()
        for offset in (0, 1, 2, 4, 15, 28, 32, 36):
            bad = bytearray(p); bad[offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                decode_pcm(bad, 0x0102030405060708, 9, 1)
        for value in (float("nan"), float("inf"), -float("inf")):
            bad = p[:40] + struct.pack("<f", value) + p[44:]
            with self.assertRaises(ValueError): decode_pcm(bad, 0x0102030405060708, 9, 1)
        for bad in (p[:39], p[:-1], p+b"\0", bytes(MAX_BYTES+1)):
            with self.assertRaises(ValueError): decode_pcm(bad, 0x0102030405060708, 9, 1)

    def test_converted_bounds_and_source_chunk(self):
        for count in (1, 160, 2560, 4096):
            self.assertEqual(len(decode_pcm(encode_pcm([0.0]*count, 1, 1, 0, 0, 2), 1, 1, 2)[1]), count)
        for count in (0, 1, 2559, 2561, 4097):
            with self.assertRaises(ValueError): encode_pcm([0.0]*count, 1, 1, 0, 0, 1)
        with self.assertRaises(ValueError): encode_pcm([0.0]*2560, 1, 1, 0, (1<<64)-2560, 1)

    def test_partial_io_uses_one_absolute_deadline(self):
        written = bytearray(); deadlines=[]
        def write(buf, deadline):
            deadlines.append(deadline); written.extend(buf[:1]); return 1
        write_frame(write, b"hello", 100, 5, clock=lambda: 1)
        def read(count, deadline):
            deadlines.append(deadline); result=bytes(written[:1]);del written[:1];return result
        self.assertEqual(read_frame(read, 100, 5, clock=lambda: 1), b"hello")
        self.assertEqual(set(deadlines), {100})
        self.assertEqual(written, b"")

    def test_oversized_length_rejected_before_payload_read(self):
        calls=[]
        def read(count, deadline): calls.append(count);return (65537).to_bytes(4,"big")
        with self.assertRaises(ValueError): read_frame(read, 100, 65536, clock=lambda: 1)
        self.assertEqual(calls, [4])

    def test_deadline_partial_eof_and_zero_write(self):
        times=iter((1, 100))
        with self.assertRaises(TimeoutError): read_frame(lambda n,d:b"\0", 100, 5, clock=lambda:next(times))
        with self.assertRaises(EOFError): read_frame(lambda n,d:b"", 100, 5, clock=lambda:1)
        with self.assertRaises(EOFError): write_frame(lambda p,d:0, b"a", 100, 5, clock=lambda:1)
        with self.assertRaises(TimeoutError): write_frame(lambda p,d:len(p), b"a", 100, 5, clock=lambda:100)


if __name__ == "__main__": unittest.main()
