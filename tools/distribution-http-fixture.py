"""Run the real Rust engine against a local, isolated HTTP/Range server."""
import hashlib, io, json, os, socket, subprocess, threading, time, zipfile
from collections import Counter
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

root = Path(__file__).resolve().parents[1]
requests = Counter()
range_requests = 0
interrupted = False
payloads = {
    '/files/v1/dnf.bin': b'fixture executable, never run' * 128,
    '/files/v1/script.bin': b'fixture script v1' * 1024,
    '/files/v1/images.bin': b'fixture image archive' * 8192,
    '/files/v1/sound.bin': b'fixture sound' * 1024,
    '/files/v1/config.bin': b'[Plugins]\r\nfixture=default\r\n',
    '/files/v2/script.bin': b'fixture script v2' * 16384,
    '/files/v2/sound.bin': b'fixture new sound' * 2048,
    '/files/resume/dnf.bin': bytes(range(256)) * 16384,
    '/files/pause/dnf.bin': bytes(reversed(range(256))) * 16384,
}
def sha(data): return hashlib.sha256(data).hexdigest()
def make_zip(entries):
    output=io.BytesIO()
    with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED) as z:
        for name,data in entries.items(): z.writestr(name,data)
    return output.getvalue()
names={'DNF.exe':'/files/v1/dnf.bin','Script.pvf':'/files/v1/script.bin','ImagePacks2/main.NPK':'/files/v1/images.bin','SoundPacks/main.ogg':'/files/v1/sound.bin','GameGaurd.ini':'/files/v1/config.bin'}
payloads['/files/wrapped.zip']=make_zip({'S4A21_CN/'+name:payloads[url] for name,url in names.items()})
payloads['/files/v1/client.zip']=make_zip({name:payloads[url] for name,url in names.items()})
payloads['/files/bad-path.zip']=make_zip({'DNF.exe':payloads[names['DNF.exe']],'../outside.bin':b'blocked'})
output=io.BytesIO()
with zipfile.ZipFile(output,'w') as z:
    info=zipfile.ZipInfo('DNF.exe');info.create_system=3;info.external_attr=0o120777 << 16;z.writestr(info,b'../outside.bin')
payloads['/files/bad-link.zip']=output.getvalue()
payloads['/files/bad-content.zip']=make_zip({name:(b'x'*len(payloads[url]) if name=='Script.pvf' else payloads[url]) for name,url in names.items()})
manifests={}
class Handler(BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*args): pass
    def do_GET(self):
        global range_requests,interrupted
        requests[self.path]+=1
        if self.path=='/stats': data=json.dumps({'requests':requests,'rangeRequests':range_requests}).encode();return self.reply(data,'application/json')
        if self.path.startswith('/manifests/'):
            name=self.path.removeprefix('/manifests/').removesuffix('.json')
            if name not in manifests:self.send_error(404);return
            return self.reply(json.dumps(manifests[name],separators=(',',':')).encode(),'application/json')
        if self.path not in payloads:self.send_error(404);return
        data=payloads[self.path];etag='"'+sha(data)+'"';start=0;status=200
        if self.headers.get('Range'):
            assert self.headers.get('If-Range')==etag
            start=int(self.headers['Range'].removeprefix('bytes=').removesuffix('-'));range_requests+=1;status=206
        self.send_response(status);self.send_header('ETag',etag);self.send_header('Accept-Ranges','bytes');self.send_header('Content-Length',str(len(data)-start));self.send_header('Content-Type','application/octet-stream')
        if status==206:self.send_header('Content-Range',f'bytes {start}-{len(data)-1}/{len(data)}')
        self.end_headers()
        if self.path=='/files/resume/dnf.bin' and not interrupted:
            interrupted=True;self.wfile.write(data[:131072]);self.wfile.flush();self.connection.shutdown(socket.SHUT_RDWR);self.connection.close();self.close_connection=True;return
        try:
            if self.path=='/files/pause/dnf.bin':
                for offset in range(start,len(data),16384):self.wfile.write(data[offset:offset+16384]);self.wfile.flush();time.sleep(.01)
            else:self.wfile.write(data[start:])
        except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError):self.close_connection=True
    def reply(self,data,kind):
        self.send_response(200);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

def main():
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler);origin=f'http://127.0.0.1:{server.server_port}'
    def manifest(version,files,archive=None):
        value={'schemaVersion':1,'edition':'fixture','realm':'s4a21','adapter':'fixture-a21','version':version,'files':[{'path':name,'size':len(payloads[url]),'sha256':sha(payloads[url]),'url':origin+url} for name,url in files.items()]}
        if archive:value['archive']={'url':origin+archive,'size':len(payloads[archive]),'sha256':sha(payloads[archive])}
        return value
    v1=manifest('1.0.0',names,'/files/v1/client.zip');manifests['v1']=v1
    v2names=names.copy();v2names['Script.pvf']='/files/v2/script.bin';v2names.pop('SoundPacks/main.ogg');v2names['SoundPacks/new.ogg']='/files/v2/sound.bin'
    v2=manifest('1.1.0',v2names);v2['removeFiles']=['SoundPacks/main.ogg'];manifests['v2']=v2
    def copy():return json.loads(json.dumps(v1))
    for label,url in [('bad-zip-path','/files/bad-path.zip'),('bad-zip-link','/files/bad-link.zip'),('bad-zip-content','/files/bad-content.zip')]:manifests[label]=manifest('1.0.0',names,url)
    manifests['bad-hash']=copy();manifests['bad-hash']['archive']['sha256']='0'*64
    manifests['bad-edition']=copy();manifests['bad-edition']['edition']='sponsor'
    manifests['bad-size']=copy();manifests['bad-size']['archive']['size']+=1
    manifests['wrapped']=manifest('1.0.0',names,'/files/wrapped.zip');manifests['wrapped']['archiveRoot']='S4A21_CN'
    manifests['pause']=manifest('1.0.0',{'DNF.exe':'/files/pause/dnf.bin'})
    manifests['resume']=manifest('1.0.0',{'DNF.exe':'/files/resume/dnf.bin'})
    threading.Thread(target=server.serve_forever,daemon=True).start()
    env=os.environ.copy();env['OO_DIST_FIXTURE']=origin
    try:
        cargo=Path.home()/'.cargo/bin/cargo.exe'
        result=subprocess.run([str(cargo),'test','-p','meow-launcher-core','distribution::tests::http_range_workflow','--locked','--','--ignored','--nocapture'],cwd=root,env=env)
        if result.returncode:raise SystemExit(result.returncode)
        report={'passed':True,'scope':'Real Rust engine with local HTTP Range and real ZIP fixtures, no game execution or production downloads','rangeRequests':range_requests,'checks':['first-install','incremental-update','managed-removal','unchanged-skipped','range-resume','pause-resume','wrapped-zip','modified-delete-conflict','bad-sha256','bad-size','unsafe-zip-path','zip-symlink','wrong-edition','game-running','commit-failure-rollback','crash-recovery','private-data-preserved']}
        target=root/'docs/qa/client-distribution-http.json';target.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(report))
    finally:server.shutdown();server.server_close()
if __name__=='__main__':main()
