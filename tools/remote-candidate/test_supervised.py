import copy,hashlib,json,tempfile,unittest
from pathlib import Path
from supervised import validate_supervisor
from bridge import Rejected
class SupervisedTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory(dir='C:/dev-toolchain/tmp',prefix='supervised-state-test-');self.root=Path(self.temp.name)
  (self.root/'state').mkdir();(self.root/'config').mkdir()
  self.c=dict(auth_mode='account',deployment='local',upstream=dict(host='127.0.0.1',port=40000,realm_id='r',audience='a',host_id='h',issuer='i'),exchange_origin='http://127.0.0.1:28100',bind_port=7002,channels=dict(profile='account-review-channels-v1',game_port_start=40000,game_port_count=51,directory_port=40051,bridge_host='127.0.0.4',directory_host='127.0.0.4'))
  target=self.root/'config/launcher-target.json';target.write_text(json.dumps(self.c))
  self.proofs={name:dict(pid=pid,image=image,creation_time_utc='2026-10-16T00:00:00+00:00') for name,pid,image in [('supervisor',101,'D:/fixture/python.exe'),('native',102,'D:/fixture/wireprobe.exe')]}
  self.state=dict(run_id='fixture-run',phase='healthy',healthy=True,smoke=False,user='First',supervisor_pid=101,native_pid=102,process_identity=copy.deepcopy(self.proofs),native_ready=dict(pid=102,channels=51,address='127.0.0.1:40000',advertise='127.0.0.4:40000'),game_bridges=52,directory_relay=True,session_file=str(self.root/'private/session.bin'),target_profile_sha256=hashlib.sha256(target.read_bytes()).hexdigest())
  self.ports=[dict(address='127.0.0.4',port=p,pid=101) for p in [*range(40000,40052),7002]]+[dict(address='127.0.0.1',port=p,pid=102) for p in range(40000,40052)]+[dict(address='127.0.0.1',port=28100,pid=101)]
 def tearDown(self):self.temp.cleanup()
 def validate(self):
  path=self.root/'state/supervisor.json';path.write_text(json.dumps(self.state))
  return validate_supervisor(path,self.c,process_query=lambda pid:next(copy.deepcopy(p) for p in self.proofs.values() if p['pid']==pid),listener_query=lambda:self.ports,session_check=lambda _:None)
 def test_healthy_attach_inspects_only_no_bind_or_stop(self):self.assertEqual(self.validate()['run_id'],'fixture-run')
 def test_pid_reuse_rejects(self):
  self.proofs['supervisor']['creation_time_utc']='2026-10-16T00:00:01+00:00'
  with self.assertRaises(Rejected):self.validate()
 def test_missing_identity_or_smoke_phase_rejects(self):
  original=copy.deepcopy(self.state)
  for patch in ({'process_identity':{}},{'smoke':True},{'healthy':False},{'phase':'smoke-game-channels'}):
   self.state=copy.deepcopy(original);self.state.update(patch)
   with self.assertRaises(Rejected):self.validate()
 def test_wrong_route_owner_rejects(self):
  self.ports[0]['pid']=999
  with self.assertRaises(Rejected):self.validate()
 def test_changed_scope_or_session_path_rejects(self):
  original=copy.deepcopy(self.state)
  for patch in ({'target_profile_sha256':'0'*64},{'session_file':'D:/other/session.bin'}):
   self.state=copy.deepcopy(original);self.state.update(patch)
   with self.assertRaises(Rejected):self.validate()
if __name__=='__main__':unittest.main(verbosity=2)
