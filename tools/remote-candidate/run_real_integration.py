"""Unique-runner integration: actual MariaAuthority/HTTP + host Go test process + player Bridge.
Uses a Go overlay in a private temp directory; host source files are never modified.
No DFO, HK, historical endpoint, production DB, or application main is started.
"""
from pathlib import Path
import asyncio,datetime,hashlib,json,msvcrt,os,secrets,subprocess,sys,tempfile,threading,time
ROOT=Path(os.environ.get('MEOW_DEV_ROOT') or Path(__file__).resolve().parents[2]).resolve();HERE=Path(__file__).resolve().parent
PD=ROOT/'_src/PlatformDatabase';SITE=ROOT/'_src/DfoAccountService';HTTP=ROOT/'_src/115us-identity-integration-candidate'
MODULE=ROOT/'_src/115us-host-identity-candidate/server/work/dfo-lan'
sys.path[:0]=[str(HERE),str(PD/'tests'),str(PD),str(PD/'.runtime/pylibs'),str(SITE),str(HTTP)]
from authority_maria_fixture import MariaFixture
from authority_maria_adapter import MariaAuthority,Challenge,Rejected as AuthorityRejected
from authority_http_candidate import serve,InMemorySessionResolver,SessionBinding
from bridge import Bridge,Target
from exchange import HttpExchange

def overlay_source(original):
    source=original.read_text(encoding='utf-8')
    for item in ['"bytes"','"encoding/base64"','"net/http"']:
        source=source.replace('\t'+item+'\n','')
    needle='\t\tpeer, err := net.Dial("tcp", listener.Addr().String())'
    replacement='''
        bridgeDir:=os.Getenv("DFO_BRIDGE_RENDEZVOUS")
        if bridgeDir=="" {t.Fatal("isolated bridge rendezvous required")}
        upstreamPath:=bridgeDir+"/upstream-"+strconv.Itoa(i)
        if err:=os.WriteFile(upstreamPath,[]byte(listener.Addr().String()),0600);err!=nil{t.Fatal(err)}
        proxyPath:=bridgeDir+"/proxy-"+strconv.Itoa(i)
        var proxy []byte
        deadline:=time.Now().Add(15*time.Second)
        for time.Now().Before(deadline){proxy,_=os.ReadFile(proxyPath);if len(proxy)>0{break};time.Sleep(20*time.Millisecond)}
        if len(proxy)==0{t.Fatal("bridge readiness timeout")}
        peer, err := net.Dial("tcp",string(proxy))'''
    if source.count(needle)!=1:raise RuntimeError('host test baseline changed')
    source=source.replace(needle,replacement)
    a=source.index('\t\tframe := controlledReadFrame');b=source.index('\t\tgot := <-results',a)
    source=source[:a]+source[b:]
    a=source.index('\t\toriginal := hostchallenge.Challenge');b=source.index('\n\t}',a)
    source=source[:a]+'''        native:=[]byte{0x11,0x22,byte(i),0xff}
        if _,err=peer.Write(native);err!=nil{t.Fatal(err)}
        received:=make([]byte,len(native));if _,err=io.ReadFull(got.conn,received);err!=nil||string(received)!=string(native){t.Fatal("player bridge native upstream mismatch")}
        if _,err=got.conn.Write(native);err!=nil{t.Fatal(err)}
        if _,err=io.ReadFull(peer,received);err!=nil||string(received)!=string(native){t.Fatal("player bridge native downstream mismatch")}
''' +source[b:]
    return source

async def run():
    stamp=datetime.datetime.now().strftime('%Y%m%d-%H%M%S')
    report_path=HERE/('real-integration-'+stamp+'.json')
    log_path=HERE/('real-integration-'+stamp+'.jsonl')
    source=MODULE/'cmd/wireprobe/real_http_identity_test.go'
    before=hashlib.sha256(source.read_bytes()).hexdigest()
    lock_path=Path('C:/dev-toolchain/tmp/maria-authority-fixture.lock')
    fixture=MariaFixture();bridges=[];server=None;thread=None
    report={'runner_pid':os.getpid(),'started_at':datetime.datetime.now().astimezone().isoformat(),'lock_path':str(lock_path),'production_access':False,'hk_access':False,'DFO':False,'application_main':False,'host_go_test_process':False,'passed':False}
    with lock_path.open('a+b') as lock:
        lock.seek(0);lock.write(b'0');lock.flush();lock.seek(0)
        try:msvcrt.locking(lock.fileno(),msvcrt.LK_NBLCK,1)
        except OSError:raise RuntimeError('fixture lock busy; no fixture started') from None
        owner_path=lock_path.with_suffix('.owner.json')
        owner_path.write_text(json.dumps({'pid':os.getpid(),'runner':str(Path(__file__).resolve()),'state':'locked','started_at':report['started_at']}))
        try:
            with fixture, tempfile.TemporaryDirectory(prefix='launcher-real-bridge-',dir='C:/dev-toolchain/tmp') as directory:
                temp=Path(directory)
                fixture.sql("INSERT INTO platform_user(public_uid,status) VALUES('00000002','active')")
                second=str(fixture.sql("SELECT user_id FROM platform_user WHERE public_uid='00000002'")[0][0])
                fixture.sql("INSERT INTO game_binding(user_id,realm_id,game_account_id,game_login_name) VALUES(%s,'fixture_a',4294967396,'second_native_115')",(second,))
                service=secrets.token_bytes(32);sessions=[secrets.token_bytes(32),secrets.token_bytes(32)]
                host='r5-host';issuer='fixture.auth';audience='fixture.host.r5';realm='fixture_a'
                authority=MariaAuthority({host:dict(issuer=issuer,audience=audience,realms=[realm],secret=secrets.token_bytes(32),service_token=service)},fixture.connection)
                users=[fixture.user_id,second]
                resolver=InMemorySessionResolver([SessionBinding(token=sessions[i],user_id=user,host_id=host,issuer=issuer,audience=audience,realm_id=realm,expires_at=int(time.time())+180) for i,user in enumerate(users)])
                server=serve(('127.0.0.1',0),authority,service,resolver);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
                url='http://127.0.0.1:'+str(server.server_address[1])
                (temp/'service.bin').write_bytes(service)
                meta=dict(fixture_only=True,url=url,host_id=host,issuer=issuer,audience=audience,realm=realm,service_token_file=str(temp/'service.bin'),bindings=[dict(user_id=users[i],uid=['00000001','00000002'][i],native_account_id=['4294967296','4294967396'][i],native_username=['owned_native_115','second_native_115'][i],session_file=str(temp/('session-'+str(i)))) for i in range(2)])
                (temp/'meta.json').write_text(json.dumps(meta))
                derived=temp/'real_http_identity_test.go';derived.write_text(overlay_source(source),encoding='utf-8')
                (temp/'overlay.json').write_text(json.dumps({'Replace':{str(source):str(derived)}}))
                env=os.environ.copy();env.update(DFO_REAL_IDENTITY_METADATA=str(temp/'meta.json'),DFO_BRIDGE_RENDEZVOUS=str(temp),GOTOOLCHAIN='local',GOCACHE='C:/dev-toolchain/cache/go-mod115',GOPROXY='https://goproxy.cn,direct',TEMP='C:/dev-toolchain/tmp',TMP='C:/dev-toolchain/tmp')
                exchanges=[]
                async def connect_bridges():
                    for slot in range(2):
                        upstream=temp/('upstream-'+str(slot))
                        while not upstream.exists():await asyncio.sleep(.025)
                        address=upstream.read_text();ip,port=address.rsplit(':',1)
                        exchange=HttpExchange(url,sessions[slot],fixture_loopback=True)
                        async def tracked(challenge,exchange=exchange):
                            token=await exchange(challenge);exchanges.append((dict(challenge),token));return token
                        bridge=Bridge(Target('local',ip,int(port),realm,audience,host,issuer),tracked)
                        local=await bridge.start('127.0.0.1',0);bridges.append(bridge)
                        (temp/('proxy-'+str(slot))).write_text(local[0]+':'+str(local[1]))
                watch=asyncio.create_task(connect_bridges())
                try:
                    with log_path.open('wb') as log:
                        cmd=[str(ROOT/'_tools/mod115-toolchain/go/bin/go.exe'),'test','-overlay='+str(temp/'overlay.json'),'-json','./cmd/wireprobe','-run','^TestActualMariaHTTPConfiguredGatewayIsolation$','-count=1','-timeout=60s']
                        result=await asyncio.to_thread(subprocess.run,cmd,cwd=MODULE,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=180)
                    report['host_go_test_process']=True;report['go_exit']=result.returncode
                    if result.returncode!=0:raise RuntimeError('Go overlay test failed; inspect redacted test log')
                    await asyncio.wait_for(watch,1)
                    consumed=fixture.sql('SELECT COUNT(*) FROM native_login_consumption')[0][0]
                    replay_denied=0
                    for challenge,token in exchanges:
                        try:authority.consume(Challenge(**challenge),service,token)
                        except AuthorityRejected:replay_denied+=1
                    if consumed!=2 or replay_denied!=2:raise RuntimeError('atomic consume/replay count mismatch')
                    report.update(passed=True,consumed=consumed,replay_denied=replay_denied,player_bridges=len(bridges),actual_http=True,actual_maria_DAO009=True,native_select_isolation=True,bidirectional_synthetic_bytes=True)
                finally:
                    watch.cancel();await asyncio.gather(watch,return_exceptions=True)
                    for bridge in bridges:await bridge.close()
                    server.shutdown();server.server_close();thread.join(5);server=None
        finally:
            if server:server.shutdown();server.server_close()
            owned_server=fixture._server
            report['maria_started_by_this_runner']=owned_server is not None
            report['maria_owned_pid']=owned_server.pid if owned_server else None
            report['maria_owned_exit_code']=owned_server.poll() if owned_server else None
            report['maria_owned_exit_verified']=(owned_server.poll() is not None) if owned_server else None
            report['fixture_cleanup_verified']=fixture.cleanup_verified
            report['host_source_unchanged']=hashlib.sha256(source.read_bytes()).hexdigest()==before
            report['host_source_sha256']=before
            report['log']=str(log_path)
            owner_path.write_text(json.dumps({'pid':os.getpid(),'runner':str(Path(__file__).resolve()),'state':'released','finished_at':datetime.datetime.now().astimezone().isoformat()}))
            lock.seek(0);msvcrt.locking(lock.fileno(),msvcrt.LK_UNLCK,1);report['fixture_lock_released']=True
            report_path.write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps(report))
    return 0 if report['passed'] and report['fixture_cleanup_verified'] and report['host_source_unchanged'] else 1
if __name__=='__main__':raise SystemExit(asyncio.run(run()))
