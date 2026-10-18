"""One authorized play-test identity, passed via stdin; never print its secret."""
import json
import subprocess
from pathlib import Path

root=Path(__file__).resolve().parents[1]
credential=Path('C:/Users/user/AppData/Local/AradPlaytest/10000001.json')
identity=json.loads(credential.read_text(encoding='utf-8'))
if identity.get('uid')!='10000001' or identity.get('game_account_id')!=3 or identity.get('state')!='created-login-verified':
    raise SystemExit('Test identity receipt does not match the authorized account')
exe=root/'releases/0.5.0-meow-renamed/MeowLauncher.exe'
result=root/'target/online-play-test-result.json'
payload=json.dumps({'identifier':identity['uid'],'password':identity['password'],'directory':'C:/workspace/86JP S4A21/Client/S4A21_CN'}).encode()
try:
    completed=subprocess.run([str(exe),'--test-launch','--output',str(result)],input=payload,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=45)
finally:
    identity.clear();payload=b''
if not result.exists():raise SystemExit('Launcher did not produce a safe result')
report=json.loads(result.read_text(encoding='utf-8'))
print(json.dumps(report,ensure_ascii=False))
raise SystemExit(completed.returncode)
