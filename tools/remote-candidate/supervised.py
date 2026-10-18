"""Read-only attach to a supervisor-owned INTERNAL review bridge.
State identity fields are an explicit review seam, not a new wire protocol.
Missing proof fields reject; no auto-start/rebind/stop fallback.
"""
import asyncio,datetime,hashlib,json,os,re,subprocess,urllib.parse
from pathlib import Path
from bridge import Rejected,host_client_plan
from import_profile import load_json
from windows_inspection import inspection_environment

def powershell(script,env=None):
 result=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script],env=inspection_environment(env),capture_output=True,timeout=10,creationflags=0x08000000)
 if result.returncode or len(result.stdout)>1048576:raise Rejected('supervisor inspection failed')
 return json.loads(result.stdout.decode('utf-8-sig'))

def process_identity(pid):
 if type(pid) is not int or pid<=0:raise Rejected('invalid supervisor pid')
 env=os.environ.copy();env['MEOW_REVIEW_PID']=str(pid)
 return powershell("$ErrorActionPreference='Stop';$p=Get-CimInstance Win32_Process -Filter ('ProcessId = '+[int]$env:MEOW_REVIEW_PID);if(-not $p){throw 'missing process'};@{pid=[int]$p.ProcessId;image=$p.ExecutablePath;creation_time_utc=$p.CreationDate.ToUniversalTime().ToString('o')}|ConvertTo-Json -Compress",env)

def listeners():
 value=powershell("$ErrorActionPreference='Stop';@(Get-NetTCPConnection -State Listen|Where-Object {$_.LocalAddress -in @('127.0.0.1','127.0.0.4')}|ForEach-Object {@{address=$_.LocalAddress;port=[int]$_.LocalPort;pid=[int]$_.OwningProcess}})|ConvertTo-Json -Compress")
 return value if isinstance(value,list) else [value]

def timestamp(value):
 try:return datetime.datetime.fromisoformat(value.replace('Z','+00:00')).astimezone(datetime.timezone.utc)
 except (ValueError,TypeError,AttributeError):raise Rejected('invalid process creation time') from None

def healthy(state):
 return state.get('phase')=='healthy' and state.get('healthy') is True and state.get('smoke') is False

def validate_supervisor(path,config,process_query=process_identity,listener_query=listeners,session_check=None):
 state_path=Path(path).resolve();root=state_path.parent.parent
 if state_path.name!='supervisor.json' or state_path.parent.name!='state' or state_path.drive.upper()=='C:':raise Rejected('invalid supervisor state path')
 state=load_json(state_path)
 if not healthy(state) or not isinstance(state.get('run_id'),str) or not re.fullmatch('[A-Za-z0-9_-]{1,100}',state['run_id']):raise Rejected('supervisor is not in healthy manual-review phase')
 if state.get('user') not in ('First','Second'):raise Rejected('review session user missing')
 target_path=root/'config/launcher-target.json';profile=load_json(target_path)
 if hashlib.sha256(target_path.read_bytes()).hexdigest()!=state.get('target_profile_sha256'):raise Rejected('supervisor target digest mismatch')
 for key in ('auth_mode','deployment','upstream','exchange_origin','bind_port','channels'):
  if profile.get(key)!=config.get(key):raise Rejected('supervisor target differs from reviewed configuration')
 channels=config['channels'];start=channels['game_port_start'];count=channels['game_port_count'];directory=channels['directory_port']
 if count!=51 or state.get('game_bridges')!=52 or state.get('directory_relay') is not True:raise Rejected('full channel bridges not ready')
 proofs=state.get('process_identity')
 if not isinstance(proofs,dict) or set(proofs)!={'supervisor','native'}:raise Rejected('process creation/image proof not available')
 for name,pid_key in (('supervisor','supervisor_pid'),('native','native_pid')):
  expected=proofs[name]
  if not isinstance(expected,dict) or set(expected)!={'pid','image','creation_time_utc'} or expected['pid']!=state.get(pid_key):raise Rejected('invalid process identity proof')
  actual=process_query(expected['pid'])
  if actual.get('pid')!=expected['pid'] or os.path.normcase(os.path.normpath(actual.get('image','')))!=os.path.normcase(os.path.normpath(expected['image'])) or timestamp(actual.get('creation_time_utc'))!=timestamp(expected['creation_time_utc']):raise Rejected('process missing or pid reused')
 ready=state.get('native_ready')
 if not isinstance(ready,dict) or ready.get('pid')!=state['native_pid'] or ready.get('channels')!=51 or ready.get('address')!=f"{config['upstream']['host']}:{start}" or ready.get('advertise')!=f"127.0.0.4:{start}":raise Rejected('native target not ready')
 expected_session=(root/'private/session.bin').resolve()
 if Path(state.get('session_file','')).resolve()!=expected_session:raise Rejected('unexpected session file')
 if session_check is None:
  from manual import session_metadata
  session_check=session_metadata
 session_check(expected_session) # ACL/size only: does not read credentials or consume a session.
 owned={(x['address'],x['port']):x['pid'] for x in listener_query()}
 for port in [*range(start,start+count),config['bind_port'],directory]:
  if owned.get(('127.0.0.4',port))!=state['supervisor_pid']:raise Rejected('bridge listener not owned by supervisor')
 for port in [*range(start,start+count),directory]:
  if owned.get((config['upstream']['host'],port))!=state['native_pid']:raise Rejected('native listener ownership mismatch')
 exchange=urllib.parse.urlsplit(config['exchange_origin'])
 if owned.get((exchange.hostname,exchange.port))!=state['supervisor_pid']:raise Rejected('session service listener ownership mismatch')
 reread=load_json(state_path)
 if not healthy(reread) or reread.get('run_id')!=state['run_id']:raise Rejected('supervisor changed during inspection')
 return state

async def client_only(config,state_path,start_client):
 state=validate_supervisor(state_path,config)
 print('受监管 Bridge 健康检查通过；不会重复监听端口或停止后台环境。')
 if not start_client:return
 import ctypes
 if os.name!='nt' or not ctypes.windll.shell32.IsUserAnAdmin():raise Rejected('原生宿主的 WFP 隔离需要管理员；请手动以管理员权限运行')
 import datetime
 logs=Path(config['log_dir'])/datetime.datetime.now().strftime('supervised-client-%Y%m%d-%H%M%S');logs.mkdir(parents=True,exist_ok=False)
 breakpoints=logs/'breakpoints.txt';breakpoints.write_text('')
 argv=host_client_plan(config['host_exe'],config['client_dir'],str(logs/'client.log'),str(breakpoints),config['bind_port'])
 process=None
 with (logs/'host.log').open('wb') as output:
  try:
   latest=load_json(state_path)
   if not healthy(latest) or latest.get('run_id')!=state['run_id']:raise Rejected('supervisor stopped before client start')
   process=await asyncio.create_subprocess_exec(*argv,stdin=asyncio.subprocess.DEVNULL,stdout=output,stderr=output,creationflags=0x08000000)
   while process.returncode is None:
    await asyncio.sleep(.5)
    latest=load_json(state_path)
    if not healthy(latest) or latest.get('run_id')!=state['run_id']:raise Rejected('supervised session no longer healthy')
   print('本次客户端宿主已退出。后台环境未停止。')
  finally:
   if process and process.returncode is None:
    process.terminate()
    try:await asyncio.wait_for(process.wait(),5)
    except TimeoutError:process.kill();await process.wait()
