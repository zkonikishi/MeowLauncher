import asyncio,socket,unittest
from bridge import Target
from channels import ChannelBridges,validate_channels

def block(count):
 for base in range(39000,55000,64):
  held=[]
  try:
   for port in range(base,base+count):
    s=socket.socket();s.bind(('127.0.0.4',port));held.append(s)
   return base
  except OSError:pass
  finally:
   for s in held:s.close()
 raise RuntimeError('no test port block')

def mapping(base,count=51):return dict(profile='account-review-channels-v1',game_port_start=base,game_port_count=count,directory_port=base+count,bridge_host='127.0.0.4',directory_host='127.0.0.4')
class ChannelsTests(unittest.IsolatedAsyncioTestCase):
 async def test_51_channels_alias_and_directory_all_close(self):
  base=block(54);group=ChannelBridges(Target('local','127.0.0.1',base,'r','a','h','i'),None,mapping(base),base+53)
  await group.start();self.assertEqual(len(group.routes),53);await group.close()
  # All our listeners released; exact same ports can immediately be bound again.
  sockets=[]
  try:
   for port in [*range(base,base+52),base+53]:
    s=socket.socket();s.bind(('127.0.0.4',port));sockets.append(s)
  finally:
   for s in sockets:s.close()
 async def test_partial_bind_failure_rolls_back_without_touching_occupied_owner(self):
  base=block(6);occupied=socket.socket();occupied.bind(('127.0.0.4',base+2));occupied.listen()
  group=ChannelBridges(Target('local','127.0.0.1',base,'r','a','h','i'),None,mapping(base,3),base+5)
  try:
   with self.assertRaises(OSError):await group.start()
   self.assertEqual(group.routes,[])
   for port in (base,base+1):
    s=socket.socket();s.bind(('127.0.0.4',port));s.close()
   self.assertEqual(occupied.getsockname()[1],base+2)
  finally:occupied.close();await group.close()
 async def test_cross_channel_auth_ids_and_directory_forwarding(self):
  from bridge import MAGIC,READY_TYPE,send,frame
  import secrets
  base=block(7);seen=[];servers=[];tasks=set()
  async def host(reader,writer):
   task=asyncio.current_task();tasks.add(task)
   try:
    cid=secrets.token_hex(16);seen.append(cid)
    challenge=dict(host_id='h',issuer='i',audience='a',realm_id='r',version_id='115us',connection_id=cid,nonce=secrets.token_hex(32),proof='fixture')
    writer.write(MAGIC);await send(writer,{'type':'host-challenge-v1','challenge':challenge});await frame(reader)
    await send(writer,{'type':READY_TYPE,'connection_id':cid,'version_id':'115us','ready':True})
    data=await reader.read(20);writer.write(data);await writer.drain()
   finally:writer.close();await writer.wait_closed();tasks.discard(task)
  async def directory(reader,writer):
   writer.write(b'directory-fixture');await writer.drain();writer.close();await writer.wait_closed()
  for port in (base,base+1):servers.append(await asyncio.start_server(host,'127.0.0.1',port))
  servers.append(await asyncio.start_server(directory,'127.0.0.1',base+2))
  async def exchange(_):return 'a1.'+'x'*43
  group=ChannelBridges(Target('local','127.0.0.1',base,'r','a','h','i'),exchange,mapping(base,2),base+4)
  try:
   await group.start()
   for port in (base,base+1,base+4):
    reader,writer=await asyncio.open_connection('127.0.0.4',port);writer.write(b'native');await writer.drain()
    self.assertEqual(await asyncio.wait_for(reader.readexactly(6),1),b'native');writer.close();await writer.wait_closed()
   self.assertEqual(len(set(seen)),3)
   reader,writer=await asyncio.open_connection('127.0.0.4',base+2)
   self.assertEqual(await asyncio.wait_for(reader.readexactly(17),1),b'directory-fixture');writer.close();await writer.wait_closed()
  finally:
   await group.close()
   for server in servers:server.close();await server.wait_closed()
   if tasks:await asyncio.gather(*list(tasks),return_exceptions=True)
 def test_route_overlap_rejected(self):
  with self.assertRaises(Exception):validate_channels(mapping(40000),40001)
if __name__=='__main__':unittest.main(verbosity=2)
