import os,sys,json,importlib.util
sys.path.insert(0,'workers/vc_worker')
import model_bootstrap
model_bootstrap.runpy.run_path=lambda path,run_name: print(json.dumps({'python_pid':os.getpid(),'fixed_runtime':path,'torch_imported':'torch' in sys.modules,'torch_origin':importlib.util.find_spec('torch').origin,'no_model_site_processing':'bootstrap uses sys.path.insert, not site.addsitedir'}))
model_bootstrap.main()