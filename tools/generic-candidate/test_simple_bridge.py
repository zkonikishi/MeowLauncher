import asyncio,secrets,ssl,unittest
from simple_bridge import MAGIC,ServerTarget,SimpleConnector,SimpleBridge,Rejected,read_frame,write_frame,native_id,credentials

class SimpleTests(unittest.IsolatedAsyncioTestCase):
 async def asyncSetUp(self):self.tasks=set();self.received=[];self.bridge=None
 async def asyncTearDown(self):
  if self.bridge:await self.bridge.close()
  self.server.close()
  if self.tasks:
   for task in list(self.tasks):task.cancel()
   await asyncio.gather(*list(self.tasks),return_exceptions=True)
  await self.server.wait_closed()
 async def backend(self,patch=None,extra=b''):
  async def handle(reader,writer):
   task=asyncio.current_task();self.tasks.add(task)
   try:
    c=dict(type='simple-challenge-v1',mode='simple',authority_id='native-server',realm_id='realm',save_profile_id='save',connection_id=secrets.token_hex(16),nonce=secrets.token_hex(32))
    writer.write(MAGIC);await write_frame(writer,c)
    value=await read_frame(reader);self.received.append(value)
    if value['password']=='wrong':return
    common={key:c[key] for key in ('mode','authority_id','realm_id','save_profile_id','connection_id')}
    if value['operation']=='register':
     await write_frame(writer,dict(common,type='simple-registered-v1',registered=True))
     writer.write(extra);await writer.drain();return
    ready=dict(common,type='simple-ready-v1',ready=True,native_account_id='9223372036854775807',username=value['username'])
    ready.update(patch or {});await write_frame(writer,ready)
    while data:=await reader.read(1024):writer.write(data);await writer.drain()
   except (Rejected,ConnectionError):pass
   finally:writer.close();await writer.wait_closed();self.tasks.discard(task)
  self.server=await asyncio.start_server(handle,'127.0.0.1',0)
  return ServerTarget('127.0.0.1',self.server.sockets[0].getsockname()[1],'native-server','realm','save',allow_loopback_plaintext=True)
 async def test_register_then_fresh_login_with_string_id(self):
  target=await self.backend();client=SimpleConnector(target)
  self.assertEqual(await client.register('Alice',' exact '),{'registered':True})
  reader,writer,identity=await client.login('Alice',' exact ')
  self.assertEqual(identity['native_account_id'],'9223372036854775807');self.assertNotIn('uid',identity)
  self.assertEqual(self.received[0]['password'],' exact ')
  self.assertEqual(set(self.received[0]),{'type','operation','nonce','username','password'})
  writer.close();await writer.wait_closed()
 async def test_registration_never_returns_game_stream(self):
  target=await self.backend(extra=b'native')
  with self.assertRaises(Rejected):await SimpleConnector(target).register('Alice','x')
 async def test_bridge_wrong_password_closes_without_forwarding(self):
  target=await self.backend()
  async def provider():return 'Alice','wrong'
  self.bridge=SimpleBridge(target,provider);addr=await self.bridge.start('127.0.0.1',0)
  reader,writer=await asyncio.open_connection(*addr);writer.write(b'native');await writer.drain()
  self.assertEqual(await asyncio.wait_for(reader.read(20),1),b'');writer.close();await writer.wait_closed()
 async def test_bridge_native_bytes_only_after_matching_ready(self):
  target=await self.backend()
  async def provider():return 'Alice','x'
  self.bridge=SimpleBridge(target,provider);addr=await self.bridge.start('127.0.0.1',0)
  reader,writer=await asyncio.open_connection(*addr);writer.write(b'\x00native\xff');await writer.drain()
  self.assertEqual(await asyncio.wait_for(reader.readexactly(8),1),b'\x00native\xff');writer.close();await writer.wait_closed()
 async def test_wrong_profile_scope_rejects(self):
  target=await self.backend(patch={'save_profile_id':'other'})
  with self.assertRaises(Rejected):await SimpleConnector(target).login('Alice','x')
 async def test_numeric_ready_and_extra_uid_reject(self):
  target=await self.backend(patch={'ready':1,'uid':'00000001'})
  with self.assertRaises(Rejected):await SimpleConnector(target).login('Alice','x')

class BoundaryTests(unittest.TestCase):
 def test_exact_password_bytes_and_int64(self):
  credentials('Alice','密'*24)
  for args in [(' Alice','x'),('Alice','密'*25)]:
   with self.assertRaises(Rejected):credentials(*args)
  for value in ['9223372036854775808','01',42]:
   with self.assertRaises(Rejected):native_id(value)
 def test_remote_requires_tls13_hostname(self):
  ctx=ServerTarget('server.example.invalid',443,'a','r','s').context()
  self.assertEqual(ctx.minimum_version,ssl.TLSVersion.TLSv1_3);self.assertTrue(ctx.check_hostname);self.assertEqual(ctx.verify_mode,ssl.CERT_REQUIRED)
  with self.assertRaises(Rejected):ServerTarget('server.example.invalid',443,'a','r','s',allow_loopback_plaintext=True).context()
if __name__=='__main__':unittest.main(verbosity=2)
