"""Operator-run account-mode candidate. Never imported by the shipped 0.7.1 app.
No automatic startup: the operator must explicitly invoke --start-client.
Configuration is explicitly review-only until GM config signing/trust is integrated.
"""
import argparse,asyncio,ctypes,hashlib,json,os,sys
from pathlib import Path
from bridge import Bridge,Target,Rejected,host_client_plan
from exchange import HttpExchange

KEYS={'review_only','auth_mode','deployment','upstream','exchange_origin','bind_port','host_exe','host_sha256','client_dir','client_sha256','log_dir'}
SCOPE={'host','port','realm_id','audience','host_id','issuer'}
def check_config(path):
    def unique(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise Rejected('配置含重复字段')
            result[key]=value
        return result
    if Path(path).stat().st_size>16384:raise Rejected('配置过大')
    c=json.loads(Path(path).read_text(encoding='utf-8-sig'),object_pairs_hook=unique)
    if not isinstance(c,dict) or set(c) not in (KEYS,KEYS|{'channels'}) or c['review_only'] is not True or c['auth_mode']!='account':
        raise Rejected('仅支持明确标记的自定义模式审阅配置')
    if not isinstance(c['upstream'],dict) or set(c['upstream'])!=SCOPE:raise Rejected('目标配置字段无效')
    if type(c['upstream']['port']) is not int or type(c['bind_port']) is not int:raise Rejected('端口须为整数')
    target=Target(c['deployment'],**c['upstream']);target.validate()
    if 'channels' in c:
        from channels import validate_channels
        validate_channels(c['channels'],c['bind_port'])
        if c['upstream']['port']!=c['channels']['game_port_start']:raise Rejected('首入口与频道映射不一致')
    if not 1<=c['bind_port']<=65535:raise Rejected('本地端口无效')
    for key in ('host_exe','client_dir','log_dir'):
        if not isinstance(c[key],str) or not Path(c[key]).is_absolute():raise Rejected('文件目录须为绝对路径')
    for key,file in (('host_sha256',Path(c['host_exe'])),('client_sha256',Path(c['client_dir'])/'DFO.exe')):
        expected=c[key]
        if not isinstance(expected,str) or len(expected)!=64 or any(x not in '0123456789abcdef' for x in expected):raise Rejected('文件摘要格式无效')
        if not file.is_file():raise Rejected('宿主或客户端文件不存在')
        with file.open('rb') as source:actual=hashlib.file_digest(source,'sha256').hexdigest()
        if actual!=expected:raise Rejected('宿主或客户端文件摘要不匹配')
    # URL policy checked without network, using a non-credential placeholder in memory.
    HttpExchange(c['exchange_origin'],b'x',fixture_loopback=c['deployment']=='local')
    return c,target

def session_metadata(path):
    # Only a path is passed via argv; the backend owns private ACL and revocation.
    import subprocess
    source=Path(path)
    if os.name!='nt' or not source.is_absolute() or source.drive.upper()=='C:' or source.is_symlink() or not source.is_file() or source.stat().st_size!=32:
        raise Rejected('需要目标环境生成的私密会话文件')
    script="""$ErrorActionPreference='Stop'; $acl=Get-Acl -LiteralPath $env:MEOW_REVIEW_SESSION_ACL_PATH; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value; $allowed=@($sid,'S-1-5-18','S-1-5-32-544'); $private=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -in $allowed; foreach($ace in $acl.Access){if($ace.AccessControlType -eq 'Allow' -and (([int64]$ace.FileSystemRights -band 1) -ne 0)){if($ace.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin $allowed){$private=$false}}}; @{private=$private}|ConvertTo-Json -Compress"""
    from windows_inspection import inspection_environment
    env=inspection_environment();env['MEOW_REVIEW_SESSION_ACL_PATH']=str(source)
    check=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script],env=env,capture_output=True,timeout=10,creationflags=0x08000000)
    if check.returncode or json.loads(check.stdout).get('private') is not True:raise Rejected('会话文件ACL不是私密的')
    return source

def session_file(path):
    source=session_metadata(path)
    with source.open('rb') as file:raw=file.read(33)
    if len(raw)!=32:raise Rejected('会话文件格式无效')
    return raw

async def start(c,target,start_client,session_path):
    session=session_file(session_path)
    exchange=HttpExchange(c['exchange_origin'],session,fixture_loopback=target.mode=='local')
    if 'channels' in c:
        from channels import ChannelBridges
        bridge=ChannelBridges(target,exchange,c['channels'],c['bind_port'])
    else:bridge=Bridge(target,exchange)
    session=None
    process=None;log=None
    try:
        if 'channels' in c:await bridge.start()
        else:await bridge.start('127.0.0.4',c['bind_port'])
        print('自定义模式审阅连接桥已就绪，等待客户端连接；这不代表认证或游戏入场成功。')
        if start_client:
            if os.name!='nt' or not ctypes.windll.shell32.IsUserAnAdmin():raise Rejected('原生客户端宿主需要安装 WFP 网络隔离；请操作员以管理员身份手动运行')
            logs=Path(c['log_dir']);logs.mkdir(parents=True,exist_ok=True)
            import datetime
            run=logs/datetime.datetime.now().strftime('manual-%Y%m%d-%H%M%S')
            run.mkdir(exist_ok=False)
            breakpoint=run/'breakpoints.txt';breakpoint.write_text('')
            argv=host_client_plan(c['host_exe'],c['client_dir'],str(run/'client.log'),str(breakpoint),c['bind_port'])
            log=(run/'host.log').open('wb')
            process=await asyncio.create_subprocess_exec(*argv,stdin=asyncio.subprocess.DEVNULL,stdout=log,stderr=log,creationflags=0x08000000)
            code=await process.wait()
            print('客户端宿主已退出，退出码：'+str(code))
        else:
            print('仅桥接：未启动任何游戏/网关/存储。按 Ctrl+C 退出。')
            await asyncio.Future()
    finally:
        await bridge.close()
        if process and process.returncode is None:
            process.terminate()
            try:await asyncio.wait_for(process.wait(),5)
            except TimeoutError:process.kill();await process.wait()
        if log:log.close()

def main():
    parser=argparse.ArgumentParser(description='自定义模式手动审阅候选，非正式启动器')
    parser.add_argument('--config',required=True);parser.add_argument('--review-unsigned-config',action='store_true')
    parser.add_argument('--check-only',action='store_true');parser.add_argument('--start-client',action='store_true');parser.add_argument('--session-file');parser.add_argument('--use-supervised-bridge')
    args=parser.parse_args()
    if not args.review_unsigned_config:raise Rejected('配置签名信任尚未集成；仅审阅请显式指定 --review-unsigned-config')
    c,target=check_config(args.config)
    if args.use_supervised_bridge:
        from supervised import validate_supervisor,client_only
        if args.check_only:
            validate_supervisor(args.use_supervised_bridge,c)
            print('受监管环境和配置检查通过；未启动客户端或改变后台。')
        else:asyncio.run(client_only(c,args.use_supervised_bridge,args.start_client))
        return
    if args.check_only:
        print('配置与宿主/DFO摘要通过；未联网、未写配置、未启动程序。');return
    if not args.session_file:raise Rejected('请使用隔离环境提供的私密会话文件，不要粘贴token')
    asyncio.run(start(c,target,args.start_client,args.session_file))
if __name__=='__main__':
    try:main()
    except KeyboardInterrupt:print('审阅会话已结束。')
    except Exception:print('审阅操作未完成。请检查配置、文件摘要、服务和权限；未切换其他认证模式。',file=sys.stderr);raise SystemExit(1)
