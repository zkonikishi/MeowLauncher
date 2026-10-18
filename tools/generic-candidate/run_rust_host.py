"""Client-only acceptance against a host-owner-managed bounded universal main.
No listener/process/backend lifecycle ownership; only invoke our Rust probe.
Secrets are read into memory and sent on stdin, never argv or public reports.
"""
import hashlib,json,subprocess,sys,datetime
from pathlib import Path

ROOT=Path('C:/workspace/Launcher/MeowLauncher')
STATE=Path('C:/workspace/_tools/logs/115us-host-identity/rust-host-current.json')
TMP=Path('C:/dev-toolchain/tmp').resolve()

def within(path):
 p=Path(path).resolve()
 if not p.is_relative_to(TMP):raise ValueError('private metadata outside expected runtime')
 return p

def load(path,limit=16384):
 data=path.read_bytes()
 if len(data)>limit:raise ValueError('oversized metadata')
 return json.loads(data)

def main():
 receipt=None;run_id=None;report={'passed':False,'scope':'Rust generic adapter actual host TLS and supplied native bootstrap; not GUI/DFO/game acceptance','noGame':True,'noBackendStart':True}
 try:
  state=load(STATE)
  if state.get('phase')!='ready':raise ValueError('host owner is not ready')
  run_id=state['run_id'];metadata_path=within(state['metadata_file']);metadata=load(metadata_path)
  if metadata['run_id']!=run_id:raise ValueError('run mismatch')
  receipt=within(metadata['result_file'])
  if receipt.parent!=metadata_path.parent or receipt.name!='rust-result.json' or receipt.exists():raise ValueError('receipt path or lifecycle mismatch')
  host,port=metadata['address'].rsplit(':',1)
  if host!='127.0.0.1' or metadata['server_name']!='127.0.0.1':raise ValueError('not bounded owner loopback TLS target')
  ca=within(metadata['ca_der_file']);password_file=within(metadata['password_file']);native_file=within(metadata['expected_native_file'])
  native=native_file.read_bytes()
  if len(native)!=1060:raise ValueError('unexpected owner bootstrap vector')
  password=password_file.read_bytes().decode('utf-8')
  target=dict(host=host,port=int(port),serverName=metadata['server_name'],authorityId=metadata['authority_id'],realmId=metadata['realm_id'],saveProfileId=metadata['save_profile_id'],caCertificate=str(ca))
  vector=dict(writeHex='',readLength=len(native),expectedSha256=hashlib.sha256(native).hexdigest())
  users=[metadata['registration_username'],*metadata['seeded_users']]
  if users!=['rust_registered','simple_first','simple_second']:raise ValueError('unexpected owner test accounts')
  accounts=[dict(username=user,password=password,registerFirst=i==0,nativeSteps=[vector]) for i,user in enumerate(users)]
  accounts[0].update(wrongPassword='deliberately-wrong-test-only',checkWrongHostname=True)
  payload=json.dumps(dict(target=target,accounts=accounts),ensure_ascii=False).encode('utf-8')
  probe=ROOT/'target/debug/examples/generic_auth_probe.exe'
  result=subprocess.run([str(probe)],input=payload,capture_output=True,timeout=150,creationflags=subprocess.CREATE_NO_WINDOW)
  payload=None;password=None;accounts=None
  if len(result.stdout)>65536:raise ValueError('oversized Rust report')
  rust=json.loads(result.stdout)
  report.update(run_id=run_id,rust_exit=result.returncode,rust=rust)
  if result.returncode or rust.get('passed') is not True:raise ValueError('Rust acceptance rejected')
  results=rust['accounts']
  first=results[0]
  checks=dict(register_reconnect_login=first.get('registered') is True and first.get('reconnectedLogin') is True and first.get('nativeAccountId')==metadata['expected_native_account_id'],same_socket_native_bytes=all(r.get('sameTlsStream') is True and len(r.get('nativeSteps',[]))==1 and r['nativeSteps'][0].get('matched') is True and r['nativeSteps'][0].get('read')==1060 for r in results),wrong_password_rejected='wrong-password-rejected' in first.get('negativeChecks',[]),tls_hostname_verified=all(r.get('tls')=='TLS1.3' for r in results) and 'wrong-hostname-rejected' in first.get('negativeChecks',[]))
  report['checks']=checks
  if len(results)!=3 or len({r['nativeAccountId'] for r in results})!=3 or not all(checks.values()):raise ValueError('missing real checks')
  latest=load(STATE)
  if latest.get('run_id')!=run_id or latest.get('phase')!='ready':raise ValueError('host lifecycle changed')
  report['passed']=True
 except Exception as e:
  report['error_type']=type(e).__name__
 finally:
  now=datetime.datetime.now().astimezone().isoformat();report['updated_at']=now
  public=ROOT/'docs/qa/generic-rust-host-actual.json';public.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
  if receipt is not None:
   reply={'run_id':run_id,'passed':report['passed'],**report.get('checks',{})}
   staged=receipt.with_suffix('.new');staged.write_text(json.dumps(reply)+'\n',encoding='utf-8');staged.replace(receipt)
  print(json.dumps({'passed':report['passed'],'report':str(public),'host_receipt_written':receipt is not None}))
 return 0 if report['passed'] else 1

if __name__=='__main__':raise SystemExit(main())
