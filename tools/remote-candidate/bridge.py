"""Isolated controlled-bridge candidate. No native packet parser or deployment entry.
The JSON envelope below is a proposed bridge-only transport, NOT existing DFO wire.
Exchange is injected: player code must never hold private Authority service credentials.
"""
import asyncio
import ipaddress
import json
import re
import ssl
import struct
from dataclasses import dataclass

MAGIC = b'MEOW115/1\n'
# r5 host-owner frozen READY: emitted only after Consume/Bind/native mapping/Lease.
READY_TYPE = 'native-ready-v1'
LIMIT = 16384

class Rejected(Exception):
    pass

@dataclass(frozen=True)
class Target:
    mode: str
    host: str
    port: int
    realm_id: str
    audience: str
    host_id: str
    issuer: str

    def validate(self):
        if self.mode not in ('local', 'cloud') or not 1 <= self.port <= 65535:
            raise Rejected('invalid target')
        if not self.host or any(not x or len(x)>256 for x in (self.realm_id,self.audience,self.host_id,self.issuer)):
            raise Rejected('missing trusted scope')
        if self.mode == 'local':
            try:
                if not ipaddress.ip_address(self.host).is_loopback:
                    raise Rejected('local requires numeric loopback')
            except ValueError:
                raise Rejected('local requires numeric loopback') from None

    def tls(self, cafile=None):
        self.validate()
        if self.mode == 'local':
            return None
        # Standard CA + hostname verification; callers cannot pass an insecure context.
        context = ssl.create_default_context(cafile=cafile)
        context.minimum_version = ssl.TLSVersion.TLSv1_3
        return context

async def frame(reader):
    try:
        size=struct.unpack('>I',await reader.readexactly(4))[0]
        if not 0<size<=LIMIT:raise Rejected('invalid bridge length')
        line=await reader.readexactly(size)
        def unique(pairs):
            result={}
            for k,v in pairs:
                if k in result: raise Rejected('duplicate bridge key')
                result[k]=v
            return result
        value=json.loads(line,object_pairs_hook=unique)
        if not isinstance(value,dict):raise Rejected('invalid bridge frame')
        return value
    except (ValueError, asyncio.IncompleteReadError,asyncio.LimitOverrunError):
        raise Rejected('invalid bridge frame') from None

async def send(writer, value):
    payload=json.dumps(value,separators=(',',':')).encode()
    if len(payload)>LIMIT:raise Rejected('oversized bridge frame')
    writer.write(struct.pack('>I',len(payload))+payload)
    await writer.drain()

class Bridge:
    def __init__(self,target,exchange,timeout=5,cafile=None):
        target.validate()
        self.target=target
        self.exchange=exchange
        self.timeout=timeout
        self.context=target.tls(cafile)
        self.server=None
        self.active=set()
        self.writers=set()

    async def start(self,host='127.0.0.4',port=0):
        if not ipaddress.ip_address(host).is_loopback:raise Rejected('player bind must be loopback')
        self.server=await asyncio.start_server(self.handle,host,port,limit=LIMIT)
        return self.server.sockets[0].getsockname()

    async def handle(self,reader,writer):
        task=asyncio.current_task();self.active.add(task);self.writers.add(writer)
        upstream=None
        async def watch_disconnect():
            while not reader.at_eof():await asyncio.sleep(.025)
            task.cancel()
        disconnect=asyncio.create_task(watch_disconnect())
        try:
            async with asyncio.timeout(self.timeout):
                source,sink=await asyncio.open_connection(self.target.host,self.target.port,ssl=self.context,
                    server_hostname=self.target.host if self.context else None,limit=LIMIT)
                upstream=sink;self.writers.add(sink)
                if await source.readexactly(len(MAGIC))!=MAGIC:raise Rejected('wrong bridge prelude')
                envelope=await frame(source)
                if set(envelope)!={'type','challenge'} or envelope['type']!='host-challenge-v1' or not isinstance(envelope['challenge'],dict):raise Rejected('wrong challenge envelope')
                challenge=envelope['challenge']
                required={'host_id','issuer','audience','realm_id','version_id','connection_id','nonce','proof'}
                if set(challenge)!=required:
                    raise Rejected('unexpected bridge challenge')
                expected={'host_id':self.target.host_id,'issuer':self.target.issuer,'audience':self.target.audience,
                    'realm_id':self.target.realm_id,'version_id':'115us'}
                if any(challenge[k]!=v for k,v in expected.items()):raise Rejected('wrong scope')
                if not all(isinstance(challenge[k],str) and 0<len(challenge[k])<=4096
                    for k in ('connection_id','nonce','proof')):raise Rejected('invalid challenge')
                # No user-supplied socket ID; exchange sees only the upstream host challenge.
                token=await self.exchange(dict(challenge))
                if not isinstance(token,str) or not re.fullmatch(r'a1\.[A-Za-z0-9_-]{43}',token):
                    raise Rejected('invalid opaque assertion')
                await send(sink,{'type':'native-assertion-v1','assertion':token})
                token=None
                ready=await frame(source)
                if ready!={'type':READY_TYPE,'connection_id':challenge['connection_id'],'version_id':'115us','ready':True} or type(ready.get('ready')) is not bool:
                    raise Rejected('host did not accept this connection')
            disconnect.cancel()
            await asyncio.gather(disconnect,return_exceptions=True)
            # Only after READY do we read or forward any native client bytes.
            async def copy(src,dst):
                while data:=await src.read(65536):
                    dst.write(data);await dst.drain()
            transfers=[asyncio.create_task(copy(reader,sink)),asyncio.create_task(copy(source,writer))]
            try:
                await asyncio.wait(transfers,return_when=asyncio.FIRST_COMPLETED)
            finally:
                for transfer in transfers:transfer.cancel()
                await asyncio.gather(*transfers,return_exceptions=True)
        except Exception:
            pass # No challenges, credentials, opaque assertions or private exception text in logs.
        finally:
            disconnect.cancel()
            await asyncio.gather(disconnect,return_exceptions=True)
            streams=[stream for stream in (writer,upstream) if stream]
            for stream in streams:stream.close()
            try:
                await asyncio.wait_for(asyncio.gather(*(stream.wait_closed() for stream in streams),return_exceptions=True),1)
            except (OSError,TimeoutError):pass
            finally:
                for stream in streams:self.writers.discard(stream)
                self.active.discard(task)

    async def close(self):
        if self.server:self.server.close()
        tasks=list(self.active)
        for task in tasks:task.cancel()
        await asyncio.gather(*tasks,return_exceptions=True)
        if self.server:await self.server.wait_closed()


def host_client_plan(exe,client_dir,log_path,breakpoints,loopback_port=7002):
    """Plan only. Does not start WFP, DFO, gateway, storage, or subprocesses."""
    from pathlib import Path
    for path in (exe,client_dir,log_path,breakpoints):
        if not Path(path).is_absolute():raise Rejected('absolute paths required')
    if not 1<=loopback_port<=65535:raise Rejected('invalid local port')
    payload=f'3?127.0.0.4?{loopback_port}?probe?'+('0'*32)+'?0?0?30?0?0?0'
    return [exe,'--host-client',client_dir,log_path,'55','interactive-ui',breakpoints,payload]
