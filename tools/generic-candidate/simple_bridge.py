"""Generic edition transport: MEOWSIMPLE/1, no platform UID/session or DB access.
Not connected to current GUI. Host protocol is frozen by simple-protocol-v1.json.
"""
import asyncio,ipaddress,json,re,ssl,struct
from dataclasses import dataclass

MAGIC=b'MEOWSIMPLE/1\n'
LIMIT=4096
class Rejected(Exception):pass

def credentials(username,password):
 if not isinstance(username,str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,32}',username):raise Rejected('invalid credentials')
 if not isinstance(password,str):raise Rejected('invalid credentials')
 try:valid=1<=len(password.encode('utf-8'))<=72
 except UnicodeError:valid=False
 if not valid:raise Rejected('invalid credentials')

def native_id(value):
 if not isinstance(value,str) or not re.fullmatch(r'[1-9][0-9]{0,18}',value) or int(value)>9223372036854775807:raise Rejected('invalid native account id')
 return value

async def read_frame(reader):
 try:
  size=struct.unpack('>I',await reader.readexactly(4))[0]
  if not 1<=size<=LIMIT:raise Rejected('invalid frame length')
  data=await reader.readexactly(size)
  def unique(pairs):
   value={}
   for key,item in pairs:
    if key in value:raise Rejected('duplicate key')
    value[key]=item
   return value
  value=json.loads(data,object_pairs_hook=unique)
  if not isinstance(value,dict):raise Rejected('invalid frame')
  return value
 except (ValueError,UnicodeError,asyncio.IncompleteReadError):raise Rejected('invalid frame') from None

async def write_frame(writer,value):
 data=json.dumps(value,ensure_ascii=False,separators=(',',':')).encode('utf-8')
 if len(data)>LIMIT:raise Rejected('invalid frame length')
 writer.write(struct.pack('>I',len(data))+data);await writer.drain()

@dataclass(frozen=True)
class ServerTarget:
 host:str
 port:int
 authority_id:str
 realm_id:str
 save_profile_id:str
 ca_file:str|None=None
 allow_loopback_plaintext:bool=False
 def context(self):
  if not self.host or type(self.port) is not int or not 1<=self.port<=65535 or not all(isinstance(x,str) and 0<len(x)<=256 for x in (self.authority_id,self.realm_id,self.save_profile_id)):raise Rejected('invalid server target')
  if self.allow_loopback_plaintext:
   try:
    if not ipaddress.ip_address(self.host).is_loopback:raise Rejected('plaintext only on numeric loopback')
   except ValueError:raise Rejected('plaintext only on numeric loopback') from None
   return None
  ctx=ssl.create_default_context(cafile=self.ca_file);ctx.minimum_version=ssl.TLSVersion.TLSv1_3
  return ctx

class SimpleConnector:
 def __init__(self,target,timeout=10):
  self.target=target;self.context=target.context();self.timeout=timeout
 async def exchange(self,operation,username,password):
  credentials(username,password)
  if operation not in ('register','login'):raise Rejected('invalid operation')
  reader=writer=None
  try:
   async with asyncio.timeout(self.timeout):
    reader,writer=await asyncio.open_connection(self.target.host,self.target.port,ssl=self.context,server_hostname=self.target.host if self.context else None)
    if self.context is None:
     if not all(ipaddress.ip_address(writer.get_extra_info(name)[0]).is_loopback for name in ('sockname','peername')):raise Rejected('plaintext endpoint mismatch')
    if await reader.readexactly(len(MAGIC))!=MAGIC:raise Rejected('wrong protocol')
    c=await read_frame(reader)
    expected=dict(type='simple-challenge-v1',mode='simple',authority_id=self.target.authority_id,realm_id=self.target.realm_id,save_profile_id=self.target.save_profile_id)
    if set(c)!=set(expected)|{'connection_id','nonce'} or any(c[k]!=v for k,v in expected.items()):raise Rejected('wrong target scope')
    if not isinstance(c['connection_id'],str) or not re.fullmatch('[0-9a-f]{32}',c['connection_id']) or not isinstance(c['nonce'],str) or not re.fullmatch('[0-9a-f]{64}',c['nonce']):raise Rejected('invalid socket challenge')
    await write_frame(writer,dict(type='simple-credentials-v1',operation=operation,username=username,password=password,nonce=c['nonce']))
    password=None
    response=await read_frame(reader)
    result=dict(mode='simple',authority_id=self.target.authority_id,realm_id=self.target.realm_id,save_profile_id=self.target.save_profile_id,connection_id=c['connection_id'])
    if operation=='register':
     if response!={**result,'type':'simple-registered-v1','registered':True} or type(response.get('registered')) is not bool:raise Rejected('invalid registration response')
     if await reader.read(1)!=b'':raise Rejected('registration must not enter game')
     writer.close();await writer.wait_closed();return {'registered':True}
    if set(response)!=set(result)|{'type','ready','native_account_id','username'} or any(response[k]!=v for k,v in result.items()) or response.get('type')!='simple-ready-v1' or response.get('ready') is not True or response.get('username')!=username:raise Rejected('invalid login ready')
    identity=dict(result,native_account_id=native_id(response['native_account_id']),username=response['username'])
    return reader,writer,identity
  except BaseException:
   if writer:
    writer.close()
    try:await asyncio.wait_for(writer.wait_closed(),1)
    except (OSError,TimeoutError):pass
   raise
 async def register(self,username,password):return await self.exchange('register',username,password)
 async def login(self,username,password):return await self.exchange('login',username,password)

class SimpleBridge:
 def __init__(self,target,credential_provider):
  self.connector=SimpleConnector(target);self.provider=credential_provider;self.server=None;self.active=set()
 async def start(self,host='127.0.0.4',port=0):
  if not ipaddress.ip_address(host).is_loopback:raise Rejected('client bind must be loopback')
  self.server=await asyncio.start_server(self.handle,host,port)
  return self.server.sockets[0].getsockname()
 async def handle(self,reader,writer):
  task=asyncio.current_task();self.active.add(task);upstream=None;copies=[]
  async def disconnected():
   while not reader.at_eof():await asyncio.sleep(.025)
   task.cancel()
  watcher=asyncio.create_task(disconnected())
  try:
   username,password=await self.provider()
   source,upstream,identity=await self.connector.login(username,password);password=None
   watcher.cancel();await asyncio.gather(watcher,return_exceptions=True)
   async def copy(src,dst):
    while data:=await src.read(65536):dst.write(data);await dst.drain()
   copies=[asyncio.create_task(copy(reader,upstream)),asyncio.create_task(copy(source,writer))]
   await asyncio.wait(copies,return_when=asyncio.FIRST_COMPLETED)
  except Exception:pass
  finally:
   watcher.cancel()
   for copy in copies:copy.cancel()
   streams=[s for s in (writer,upstream) if s]
   for stream in streams:stream.close()
   try:
    await asyncio.gather(watcher,*copies,return_exceptions=True)
    await asyncio.wait_for(asyncio.gather(*(s.wait_closed() for s in streams),return_exceptions=True),1)
   except (OSError,TimeoutError):pass
   finally:self.active.discard(task)
 async def close(self):
  if self.server:self.server.close()
  tasks=list(self.active)
  for task in tasks:task.cancel()
  await asyncio.gather(*tasks,return_exceptions=True)
  if self.server:await self.server.wait_closed()
