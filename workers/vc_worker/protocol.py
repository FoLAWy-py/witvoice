"""Bounded local worker PCM codec. No devices, models, IPC handles or credentials in logs.

Wire limits/layout come from Rust export_schema. Native I/O adapters must enforce
the supplied absolute local deadline while blocked; these helpers also check it
before and after every partial operation. Not a production pipe implementation.
"""
from __future__ import annotations
import json
import math
from pathlib import Path
import struct
import time

ROOT = Path(__file__).resolve().parents[2]
WIRE = json.loads((ROOT / "crates/contracts/bindings/worker-media.json").read_text(encoding="utf-8"))
HEADER = struct.Struct(WIRE["header_format"])
MAX_BYTES = WIRE["header_bytes"] + WIRE["maximum_samples"] * 4
if HEADER.size != WIRE["header_bytes"] or WIRE["pcm_format"] != "mono_f32le":
    raise RuntimeError("unsupported Rust worker layout")


def _integer(value, maximum):
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError("invalid worker integer")
    return value


def decode_pcm(packet: bytes, session_tag: int, epoch: int, kind: int):
    _integer(session_tag, (1 << 64) - 1)
    _integer(epoch, (1 << 32) - 1)
    if type(kind) is not int or kind not in (1, 2):
        raise ValueError("invalid worker direction")
    if len(packet) < HEADER.size or len(packet) > MAX_BYTES:
        raise ValueError("worker media size limit")
    version, actual_kind, reserved, tag, actual_epoch, sequence, index, rate, count, tail = HEADER.unpack_from(packet)
    if version != 1 or reserved or tail or actual_kind not in (1, 2) or actual_kind != kind:
        raise ValueError("invalid worker media header")
    if not tag or not actual_epoch or tag != session_tag or actual_epoch != epoch:
        raise ValueError("stale worker binding")
    if rate != WIRE["native_rate"] or not 1 <= count <= WIRE["maximum_samples"]:
        raise ValueError("unsupported worker media shape")
    if actual_kind == 1 and count != WIRE["chunk_samples"]:
        raise ValueError("source chunk mismatch")
    if index + count > (1 << 64) - 1 or len(packet) != HEADER.size + count * 4:
        raise ValueError("invalid worker media length or timeline")
    samples = struct.unpack_from(f"<{count}f", packet, HEADER.size)
    if not all(math.isfinite(sample) for sample in samples):
        raise ValueError("nonfinite worker PCM")
    return {"kind": actual_kind, "session_tag": tag, "epoch": actual_epoch,
            "sequence": sequence, "source_sample_index": index, "sample_rate": rate}, samples


def encode_pcm(samples, session_tag: int, epoch: int, sequence: int, index: int, kind: int):
    _integer(session_tag, (1 << 64) - 1)
    _integer(epoch, (1 << 32) - 1)
    _integer(sequence, (1 << 32) - 1)
    _integer(index, (1 << 64) - 1)
    if type(kind) is not int or kind not in (1, 2):
        raise ValueError("invalid worker direction")
    # Check known-sized buffers before formatting/allocating a payload.
    count = len(samples)
    if not 1 <= count <= WIRE["maximum_samples"]:
        raise ValueError("worker sample count limit")
    packet = HEADER.pack(1, kind, 0, session_tag, epoch, sequence, index,
                         WIRE["native_rate"], count, 0) + struct.pack(f"<{count}f", *samples)
    decode_pcm(packet, session_tag, epoch, kind)
    return packet


def _check(deadline_ns, clock):
    if type(deadline_ns) is not int or clock() >= deadline_ns:
        raise TimeoutError("worker I/O deadline")


def read_frame(read, deadline_ns: int, maximum_bytes: int, clock=time.monotonic_ns):
    if type(maximum_bytes) is not int or not 1 <= maximum_bytes <= WIRE["control_max_bytes"]:
        raise ValueError("worker frame bound")
    def exact(count):
        chunks = bytearray()
        while len(chunks) < count:
            _check(deadline_ns, clock)
            part = read(count - len(chunks), deadline_ns)
            _check(deadline_ns, clock)
            if not part:
                raise EOFError("worker frame truncated")
            if not isinstance(part, bytes) or len(part) > count - len(chunks):
                raise ValueError("worker reader contract")
            chunks.extend(part)
        return bytes(chunks)
    length = int.from_bytes(exact(4), "big")
    if not 1 <= length <= maximum_bytes:
        raise ValueError("worker frame size limit")
    return exact(length)


def write_frame(write, payload: bytes, deadline_ns: int, maximum_bytes: int, clock=time.monotonic_ns):
    if type(maximum_bytes) is not int or not 1 <= maximum_bytes <= WIRE["control_max_bytes"] or not 1 <= len(payload) <= maximum_bytes:
        raise ValueError("worker frame size limit")
    frame = len(payload).to_bytes(4, "big") + payload
    offset = 0
    while offset < len(frame):
        _check(deadline_ns, clock)
        count = write(memoryview(frame)[offset:], deadline_ns)
        _check(deadline_ns, clock)
        if type(count) is not int or not 1 <= count <= len(frame) - offset:
            raise EOFError("worker write incomplete")
        offset += count
