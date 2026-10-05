"""Deterministic host-interruption ownership tests, not native IPC acceptance."""
import ctypes as c
import sys
import threading
import time
import unittest
from unittest.mock import patch
from windows_pipe import PipeClient


class Api:
    def __init__(self,where): self.k=self;self.where=where;self.events=[]
    def CreateEventW(self,*_): self.events.append("event");return 321
    def ReadFile(self,handle,buf,count,transferred,operation):
        self.events.append("submit");operation._obj.Internal=0x103
        if self.where=="submit": raise KeyboardInterrupt()
        c.set_last_error(997);return False
    def WaitForSingleObject(self,*_):
        self.events.append("wait")
        if self.where=="wait": raise KeyboardInterrupt()
        return 258
    def CancelIoEx(self,*_): self.events.append("cancel");return True
    def GetOverlappedResult(self,handle,operation,transferred,wait):
        self.events.append("drain" if wait else "complete")
        operation._obj.Internal=0
        c.set_last_error(995);return False
    def CloseHandle(self,handle): self.events.append("close-pipe" if handle==123 else "close-event");return True


@unittest.skipUnless(sys.platform=="win32","Windows ctypes interruption tests")
class OwnershipTests(unittest.TestCase):
    def client(self,where):
        result=PipeClient.__new__(PipeClient)
        result._owner=threading.get_ident();result._handle=123;result._api=Api(where)
        return result
    def test_interrupt_after_submission_drains_before_buffers_released(self):
        for where in ("submit","wait"):
            client=self.client(where)
            with self.subTest(where=where),self.assertRaises(KeyboardInterrupt):
                client.read(4,time.monotonic_ns()+100_000_000)
            events=client._api.events
            self.assertLess(events.index("cancel"),events.index("drain"))
            self.assertLess(events.index("drain"),events.index("close-pipe"))
            self.assertLess(events.index("drain"),events.index("close-event"))
            self.assertIsNone(client._handle)
    def test_timeout_drains_before_close(self):
        client=self.client("timeout")
        with self.assertRaises(TimeoutError): client.read(4,time.monotonic_ns()+100_000_000)
        self.assertEqual(client._api.events,["event","submit","wait","cancel","drain","close-pipe","close-event"])
        self.assertIsNone(client._handle)
    def test_invalid_deadline_retires_handle_without_submitting(self):
        client=self.client("never")
        with self.assertRaises(TimeoutError): client.read(4,0)
        self.assertEqual(client._api.events,["close-pipe"])


if __name__=="__main__": unittest.main()
