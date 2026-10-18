"""Single owner window: Rust auth/bridge plus unchanged host native codec test.
No backend/DFO start. Private credentials stdin only; no raw error output.
"""
import hashlib,json,os,pathlib,subprocess,tempfile,sys,time
ROOT=pathlib.Path('C:/workspace/Launcher/MeowLauncher')
MODULE=pathlib.Path('C:/workspace/_src/meowlink-preview-r1/server/work/dfo-lan')
STATE=pathlib.Path('C:/workspace/_tools/logs/115us-host-identity/rust-host-current.json')
TMP=pathlib.Path('C:/dev-toolchain/tmp').resolve()
def load(p):
 b=pathlib.Path(p).read_bytes()
 if len(b)>262144:raise ValueError('size')
 return json.loads(b)
def private(p):
 p=pathlib.Path(p).resolve()
 if not p.is_relative_to(TMP):raise ValueError('private path')
 return p
def main():
 receipt=None;report={'passed':False,'noGame':True,'noBackendStart':True,'scope':'Rust auth and Rust loopback bridges; unchanged host Go native codec drives C1/C8/C4/N124'}
 relay=None
 try:
  state=load(STATE)
  if state.get('phase')!='ready':raise ValueError('not ready')
  m=load(private(state['metadata_file']));run=m['run_id']
  if run!=state['run_id']:raise ValueError('run')
  receipt=private(m['result_file'])
  if receipt.exists():raise ValueError('consumed window')
  report['run_id']=run
  host,port=m['address'].rsplit(':',1)
  if host!='127.0.0.1':raise ValueError('not isolated')
  password=private(m['password_file']).read_bytes().decode()
  native=private(m['expected_native_file']).read_bytes()
  target=dict(host=host,port=int(port),serverName=m['server_name'],authorityId=m['authority_id'],realmId=m['realm_id'],saveProfileId=m['save_profile_id'],caCertificate=str(private(m['ca_der_file'])))
  vector=dict(writeHex='',readLength=len(native),expectedSha256=hashlib.sha256(native).hexdigest())
  users=[m['registration_username'],*m['seeded_users']]
  if users!=['rust_registered','simple_first','simple_second'] or len(native)!=1060:raise ValueError('fixture')
  accounts=[dict(username=u,password=password,registerFirst=i==0,nativeSteps=[vector]) for i,u in enumerate(users)]
  accounts[0].update(wrongPassword='deliberately-wrong-test-only',checkWrongHostname=True,checkWrongScope=True)
  report['phase']='auth'
  p=subprocess.run([str(ROOT/'target/release/examples/generic_auth_probe.exe')],input=json.dumps(dict(target=target,accounts=accounts)).encode(),capture_output=True,timeout=120,creationflags=subprocess.CREATE_NO_WINDOW)
  auth=json.loads(p.stdout);report['auth']=auth
  if p.returncode or not auth.get('passed') or [r['nativeAccountId'] for r in auth['accounts']]!=['3','1','2']:raise ValueError('auth')
  report['checks']=dict(register_reconnect_login=True,same_socket_native_bytes=True,wrong_password_rejected=True,tls_hostname_verified=True)
  env=os.environ.copy();env.update(TEMP=str(TMP),TMP=str(TMP),GOCACHE='C:/dev-toolchain/cache/go-mod115',GOTOOLCHAIN='local')
  with tempfile.TemporaryDirectory(prefix='launcher-native-',dir=TMP) as d:
   d=pathlib.Path(d);test=d/'native-codec.test.exe'
   source=MODULE/'cmd/wireprobe/real_native_town_test.go';report['codecSourceSha256']=hashlib.sha256(source.read_bytes()).hexdigest()
   report['phase']='codec-build'
   built=subprocess.run(['C:/workspace/_tools/mod115-toolchain/go/bin/go.exe','test','-c','-o',str(test),'./cmd/wireprobe'],cwd=MODULE,env=env,capture_output=True,timeout=150)
   if built.returncode:raise ValueError('codec build')
   # Host admission counts challenge-only wrong-scope as well as credentials:
   # register + wrong password + wrong scope + registered login + two seed
   # logins = 6. Hostname mismatch fails TLS before admission. Do not weaken
   # the real 6/60s policy or retry a rejected login in a tight loop.
   report['phase']='admission-window-refresh'
   report['admissionPacing']={'authAttempts':6,'nextBridgeLogins':2,'waitSeconds':61,'serverLimitUnchanged':True}
   print(json.dumps({'phase':'admission-window-refresh','run_id':run,'waitSeconds':61}),flush=True)
   for _ in range(61):
    time.sleep(1)
    current=load(STATE)
    if current.get('run_id')!=run or current.get('phase')!='ready':raise ValueError('window ended while pacing')
   descriptor=load(private(m['player_target_file']));rows=[]
   directory_expected=private(m['directory_expected_file']);directory_bytes=directory_expected.read_bytes()
   if not 1<=len(directory_bytes)<=131072:raise ValueError('directory vector size')
   directory_sha=hashlib.sha256(directory_bytes).hexdigest();directory_probe=private(directory_expected.parent/'reviewdirectory.exe')
   if not directory_probe.is_file():raise ValueError('fixture directory reader missing')
   report['directoryProbeSha256']=hashlib.sha256(directory_probe.read_bytes()).hexdigest();directory_rows=[]
   for user,name,other,identity in [('simple_first','First','Second','1'),('simple_second','Second','First','2')]:
    report['phase']='bridge-ready-'+identity
    relay=subprocess.Popen([str(ROOT/'target/release/examples/generic_native_bridge.exe')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
    payload=json.dumps(dict(target=target,descriptor=descriptor,username=user,password=password)).encode()+b'\n'
    relay.stdin.write(payload);relay.stdin.flush();payload=None
    ready=json.loads(relay.stdout.readline());report['lastBridgeReady']=ready
    if ready.get('phase')!='ready' or ready.get('nativeAccountId')!=identity or ready.get('boundCount')!=53:raise ValueError('bridge ready')
    game_address=ready.get('gameAddress','');game_host,game_port=game_address.rsplit(':',1)
    if game_host!='127.0.0.4' or not game_port.isdecimal() or int(game_port)==7002 or not 1<=int(game_port)<=65535:raise ValueError('mapped game address')
    report['phase']='native-directory-'+identity
    fetched=subprocess.run([str(directory_probe),'--fetch','127.0.0.4:7002',str(directory_expected)],capture_output=True,timeout=20,creationflags=subprocess.CREATE_NO_WINDOW)
    if fetched.returncode:raise ValueError('native directory alias rejected')
    directory_result=json.loads(fetched.stdout)
    if directory_result.get('passed')is not True or directory_result.get('sha256')!=directory_sha or directory_result.get('directory_bytes')!=len(directory_bytes) or directory_result.get('native_sequence')!='11/12 9/10 1/3':raise ValueError('native directory bytes mismatch')
    directory_rows.append(dict(nativeAccountId=identity,nativeSequence='11/12 9/10 1/3',directoryBytes=len(directory_bytes),expectedSha256=directory_sha,passed=True))
    meta=d/'native.json';meta.write_text(json.dumps(dict(fixture_only=True,fixture_bytes=1060,targets=[dict(address=game_address,name=name,other_name=other,username=user,native_account_id=identity)])),encoding='utf-8')
    report['phase']='native-codec-'+identity
    native_run=subprocess.run([str(test),'-test.run=^TestActualControlledNativeTownPipeline$','-test.timeout=35s','-test.v'],cwd=MODULE,env=env|{'DFO_TOWN_SMOKE_METADATA':str(meta)},capture_output=True,timeout=40,creationflags=subprocess.CREATE_NO_WINDOW)
    relay.stdin.write(b'done\n');relay.stdin.flush();closed=json.loads(relay.stdout.readline());relay.stdin.close();relay.wait(timeout=15)
    report['lastNativeExit']=native_run.returncode;report['lastBridgeClosed']=closed
    if native_run.returncode or relay.returncode or not closed.get('allPortsReleased') or not closed.get('passed'):
     # This subprocess receives no password or auth metadata; still redact the
     # parent's password defensively before retaining bounded codec diagnostics.
     report['codecDiagnostic']=(native_run.stdout+native_run.stderr).decode(errors='replace').replace(password,'[redacted]')[-6000:]
     raise ValueError('native/cleanup')
    rows.append(dict(nativeAccountId=identity,C1_C8_C4_N124=True,otherAccountAbsent=True,boundCount=53,allPortsReleased=True))
    relay=None
   if hashlib.sha256(source.read_bytes()).hexdigest()!=report['codecSourceSha256']:raise ValueError('codec changed')
   report['nativeAccounts']=rows
   report['directoryAccounts']=directory_rows
   report['routeScope']='TYPE3 alias7002 verified by native11/12,9/10,1/3 against host serializer bytes; game C1/C8/C4/N124 uses explicit mapped game port, never7002.'
   report['directoryCloseDoesNotStopGameRoute']=True
  if load(STATE).get('run_id')!=run or load(STATE).get('phase')!='ready':raise ValueError('window ended')
  report['passed']=True;report['phase']='complete'
 except Exception as e:report['errorType']=type(e).__name__
 finally:
  if relay is not None:
   try:relay.stdin.close();relay.wait(timeout=15)
   except Exception:relay.kill();relay.wait()
  path=ROOT/'docs/qa/generic-rust-native-actual.json';path.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
  if report.get('run_id') and report['run_id'].replace('-','').isdigit():(ROOT/'docs/qa'/('generic-rust-native-'+report['run_id']+'.json')).write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
  if receipt is not None and not receipt.exists():
   reply=dict(run_id=report.get('run_id'),passed=report['passed'],**report.get('checks',{}));staged=receipt.with_suffix('.new');staged.write_text(json.dumps(reply),encoding='utf-8');staged.replace(receipt)
  print(json.dumps(dict(passed=report['passed'],report=str(path))))
 return 0 if report['passed'] else 1
if __name__=='__main__':sys.exit(main())
