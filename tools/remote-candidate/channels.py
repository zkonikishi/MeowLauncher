"""Typed channel map only: authenticated game routes, distinct directory relay.
Never fall back to directory forwarding after game authentication failure.
"""
import asyncio
from dataclasses import replace
from bridge import Bridge,Rejected

KEYS={'profile','game_port_start','game_port_count','directory_port','bridge_host','directory_host'}
def validate_channels(value,alias):
 if not isinstance(value,dict) or set(value)!=KEYS or value['profile']!='account-review-channels-v1':raise Rejected('invalid channel map')
 if value['bridge_host']!='127.0.0.4' or value['directory_host']!='127.0.0.4':raise Rejected('review map requires explicit loopback alias')
 start,count,directory=value['game_port_start'],value['game_port_count'],value['directory_port']
 if any(type(x) is not int for x in (start,count,directory,alias)) or not 1<=count<=51 or not 1<=start<=65535 or start+count-1>65535 or not 1<=directory<=65535:raise Rejected('invalid port block')
 games=set(range(start,start+count))
 if directory in games or directory==alias or (alias in games and alias!=start):raise Rejected('overlapping channel route')
 return start,count,directory

class DirectoryRelay:
 def __init__(self,target):
  self.target=target;self.server=None;self.active=set();self.context=target.tls()
 async def start(self,host,port):
  self.server=await asyncio.start_server(self.handle,host,port)
 async def handle(self,reader,writer):
  task=asyncio.current_task();self.active.add(task);upstream=None;copies=[]
  try:
   source,upstream=await asyncio.wait_for(asyncio.open_connection(self.target.host,self.target.port,ssl=self.context,server_hostname=self.target.host if self.context else None),5)
   async def copy(src,dst):
    while data:=await src.read(65536):dst.write(data);await dst.drain()
   copies=[asyncio.create_task(copy(reader,upstream)),asyncio.create_task(copy(source,writer))]
   await asyncio.wait(copies,return_when=asyncio.FIRST_COMPLETED)
  except Exception:pass
  finally:
   for copy in copies:copy.cancel()
   streams=[stream for stream in (writer,upstream) if stream]
   # Close both before awaiting either: shutdown may cancel a task already in finally.
   for stream in streams:stream.close()
   try:
    await asyncio.gather(*copies,return_exceptions=True)
    await asyncio.wait_for(asyncio.gather(*(stream.wait_closed() for stream in streams),return_exceptions=True),1)
   except (OSError,TimeoutError):pass
   finally:self.active.discard(task)
 async def close(self):
  if self.server:self.server.close()
  tasks=list(self.active)
  for task in tasks:task.cancel()
  await asyncio.gather(*tasks,return_exceptions=True)
  if self.server:await self.server.wait_closed()

class ChannelBridges:
 def __init__(self,target,exchange,channels,alias=7002):
  self.start_port,self.count,self.directory_port=validate_channels(channels,alias)
  if target.port!=self.start_port:raise Rejected('primary target does not match channel block')
  self.target=target;self.exchange=exchange;self.channels=channels;self.alias=alias;self.routes=[]
 async def start(self):
  try:
   ports=list(range(self.start_port,self.start_port+self.count))
   if self.alias!=self.start_port:ports.append(self.alias)
   for port in ports:
    upstream=self.start_port if port==self.alias else port
    bridge=Bridge(replace(self.target,port=upstream),self.exchange);self.routes.append(bridge)
    await bridge.start(self.channels['bridge_host'],port)
   directory=DirectoryRelay(replace(self.target,port=self.directory_port));self.routes.append(directory)
   await directory.start(self.channels['directory_host'],self.directory_port)
  except BaseException:
   await self.close();raise
 async def close(self):
  for route in reversed(self.routes):await route.close()
  self.routes=[]
