"""Explicit native transport test only; no model, endpoints or production Ready."""
import argparse
import sys
import time
from windows_pipe import PipeClient
from protocol import MAX_BYTES, encode_pcm


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control",required=True)
    parser.add_argument("--media",required=True)
    parser.add_argument("--node-pid",required=True,type=int)
    args=parser.parse_args()
    # Node's allow-listed bootstrap handle carries exactly 32 bytes, then EOF.
    # Node must enforce process startup time, including bootstrap, with its Job.
    secret=sys.stdin.buffer.read(33)
    if len(secret)!=32 or args.control==args.media: raise ValueError("invalid test bootstrap")
    deadline=time.monotonic_ns()+2_500_000_000
    with PipeClient(args.control,args.node_pid,secret,deadline) as control:
        with PipeClient(args.media,args.node_pid,secret,deadline) as media:
            control.write_frame(b"transport-test-only",65536,deadline)
            if control.read_frame(65536,deadline)!=b"transport-ack": raise ValueError("control interop mismatch")
            packet=encode_pcm([.01]*2560,1,1,0,0,1)
            media.write_frame(packet,MAX_BYTES,deadline)
            if media.read_frame(MAX_BYTES,deadline)!=packet: raise ValueError("media interop mismatch")
    print("NATIVE_PYTHON_TRANSPORT_ONLY; model/audio=NOT_RUN; PCM_disk=false")
    return 0


if __name__=="__main__": raise SystemExit(main())
