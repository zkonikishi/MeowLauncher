import contextlib,hashlib,io,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
from manual import check_config,main
from bridge import Rejected

class ManualTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory(dir='C:/dev-toolchain/tmp',prefix='manual-plan-test-');self.root=Path(self.temp.name)
  (self.root/'dfolauncher.exe').write_bytes(b'fixture-not-an-executable');(self.root/'DFO.exe').write_bytes(b'fixture-not-a-game')
  digest=lambda f:hashlib.sha256((self.root/f).read_bytes()).hexdigest()
  self.c=dict(review_only=True,auth_mode='account',deployment='local',upstream=dict(host='127.0.0.1',port=12345,realm_id='r',audience='a',host_id='h',issuer='i'),exchange_origin='http://127.0.0.1:12346',bind_port=7002,host_exe=str(self.root/'dfolauncher.exe'),host_sha256=digest('dfolauncher.exe'),client_dir=str(self.root),client_sha256=digest('DFO.exe'),log_dir=str(self.root/'logs'))
  self.path=self.root/'config.json'
 def tearDown(self):self.temp.cleanup()
 def write(self):self.path.write_text(json.dumps(self.c));return self.path
 def test_check_only_has_no_start_network_or_prompt(self):
  self.write()
  with patch('sys.argv',['manual','--config',str(self.path),'--review-unsigned-config','--check-only']),patch('asyncio.run',side_effect=AssertionError('must not run')),contextlib.redirect_stdout(io.StringIO()):main()
  self.assertFalse((self.root/'logs').exists())
 def test_target_import_validates_and_refuses_private_fields(self):
  from import_profile import import_target
  self.write();target={k:self.c[k] for k in ('review_only','auth_mode','deployment','upstream','exchange_origin','bind_port')};target['profile']='account-review-target-v1'
  target_path=self.root/'target.json';target_path.write_text(json.dumps(target))
  result=import_target(target_path,self.path,self.root/'imported.json');self.assertEqual(check_config(result)[0]['upstream'],self.c['upstream'])
  with self.assertRaises(Rejected):import_target(target_path,self.path,result)
  target['service_token']='forbidden';target_path.write_text(json.dumps(target))
  with self.assertRaises(Rejected):import_target(target_path,self.path,self.root/'rejected.json')
  self.assertFalse((self.root/'rejected.json').exists())
 def test_missing_signature_review_ack_rejected(self):
  self.write()
  with patch('sys.argv',['manual','--config',str(self.path),'--check-only']):
   with self.assertRaises(Rejected):main()
 def test_bad_hash_mode_unknown_and_endpoint_reject(self):
  for mutation in [lambda:self.c.update(host_sha256='0'*64),lambda:self.c.update(auth_mode='simple'),lambda:self.c.update(service_token='never accepted'),lambda:self.c['upstream'].update(host='203.0.113.1')]:
   before=json.loads(json.dumps(self.c));mutation()
   with self.assertRaises(Rejected):check_config(self.write())
   self.c=before
if __name__=='__main__':unittest.main(verbosity=2)
