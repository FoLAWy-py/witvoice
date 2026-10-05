import ctypes,sys
sys.path.insert(0,"D:/Project/witvoice/workers/vc_worker")
from windows_pipe import _Api,_Overlapped
api=_Api()
sid=api.sid(api.k.GetCurrentProcess())
if not sid.startswith("S-1-"): raise ValueError("invalid SID shape")
print("Windows ABI verified: OVERLAPPED size",ctypes.sizeof(_Overlapped),"; current SID queried, not logged; pipes/model/audio=NOT_RUN")