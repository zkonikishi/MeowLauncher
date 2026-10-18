import asyncio
import base64
import os
import ssl
import unittest
from bridge import Bridge, Target, MAGIC, READY_TYPE, send, frame, Rejected, host_client_plan

class BridgeTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.servers=[];self.bridges=[];self.host_tasks=set();self.bound=[];self.forwarded=[]
    async def asyncTearDown(self):
        for bridge in self.bridges:await asyncio.wait_for(bridge.close(),3)
        for server in self.servers:server.close();await server.wait_closed()
        tasks=list(self.host_tasks)
        for task in tasks:task.cancel()
        await asyncio.gather(*tasks,return_exceptions=True)
    async def setup_bridge(self,reject=False,wrong_scope=False,delay=0,ready_patch=None,raw_ready=None):
        tokens={};counter=0
        async def host(reader,writer):
            nonlocal counter
            task=asyncio.current_task();self.host_tasks.add(task)
            try:
                counter+=1;cid=str(counter)
                c={'host_id':'host-fixture','issuer':'auth-fixture',
                    'audience':'host-fixture','realm_id':'wrong' if wrong_scope else 'realm-fixture',
                    'version_id':'115us','connection_id':cid,'nonce':os.urandom(16).hex(),'proof':'fixture-not-a-real-proof'}
                writer.write(MAGIC);await writer.drain();await send(writer,{'type':'host-challenge-v1','challenge':c})
                a=await frame(reader)
                if reject or tokens.get(a.get('assertion'))!=cid:return
                await asyncio.sleep(delay)
                self.bound.append(cid)
                ready={'type':READY_TYPE,'connection_id':cid,'version_id':'115us','ready':True}
                if ready_patch:ready.update(ready_patch)
                if raw_ready:
                    import struct
                    writer.write(struct.pack('>I',len(raw_ready))+raw_ready);await writer.drain()
                else:await send(writer,ready)
                while data:=await reader.read(65536):
                    self.forwarded.append((cid,data));writer.write(data);await writer.drain()
            except (ConnectionError,Rejected):pass
            finally:
                writer.close();await writer.wait_closed();self.host_tasks.discard(task)
        server=await asyncio.start_server(host,'127.0.0.1',0);self.servers.append(server)
        async def exchange(challenge):
            token='a1.'+base64.urlsafe_b64encode(os.urandom(32)).decode().rstrip('=')
            tokens[token]=challenge['connection_id'];return token
        target=Target('local','127.0.0.1',server.sockets[0].getsockname()[1],'realm-fixture','host-fixture','host-fixture','auth-fixture')
        bridge=Bridge(target,exchange,timeout=.2);self.bridges.append(bridge)
        address=await bridge.start('127.0.0.1',0)
        return address,bridge
    async def test_two_connections_are_independent_and_bidirectional(self):
        addr,_=await self.setup_bridge()
        async def client(data):
            reader,writer=await asyncio.open_connection(*addr);writer.write(data);await writer.drain()
            result=await asyncio.wait_for(reader.readexactly(len(data)),1)
            writer.close();await writer.wait_closed();return result
        self.assertEqual(await asyncio.gather(client(b'\x00one\xff'),client(b'\x01two\xfe')),[b'\x00one\xff',b'\x01two\xfe'])
        self.assertEqual(len(set(self.bound)),2)
        self.assertEqual(len(set(cid for cid,_ in self.forwarded)),2)
    async def test_denied_consume_never_forwards_game_bytes(self):
        addr,_=await self.setup_bridge(reject=True)
        reader,writer=await asyncio.open_connection(*addr);writer.write(b'native-before-auth');await writer.drain()
        self.assertEqual(await asyncio.wait_for(reader.read(20),1),b'');self.assertEqual(self.forwarded,[])
        writer.close();await writer.wait_closed()
    async def test_wrong_scope_rejects_before_exchange(self):
        addr,bridge=await self.setup_bridge(wrong_scope=True)
        async def forbidden(_):raise AssertionError('wrong scope must never reach exchange')
        bridge.exchange=forbidden
        reader,writer=await asyncio.open_connection(*addr)
        self.assertEqual(await asyncio.wait_for(reader.read(20),1),b'');self.assertEqual(self.bound,[])
        writer.close();await writer.wait_closed()
    async def test_timeout_before_ready_never_forwards(self):
        addr,_=await self.setup_bridge(delay=.5)
        reader,writer=await asyncio.open_connection(*addr);writer.write(b'native-before-auth');await writer.drain()
        self.assertEqual(await asyncio.wait_for(reader.read(20),1),b'');self.assertEqual(self.forwarded,[])
        writer.close();await writer.wait_closed()
    async def test_invalid_ready_rejects_without_native_forwarding(self):
        for patch in ({'connection_id':'other'},{'version_id':'115tw'},{'ready':False},{'ready':1},{'unknown':True}):
            with self.subTest(patch=patch):
                addr,_=await self.setup_bridge(ready_patch=patch)
                reader,writer=await asyncio.open_connection(*addr);writer.write(b'not-forwarded');await writer.drain()
                self.assertEqual(await asyncio.wait_for(reader.read(20),1),b'')
                writer.close();await writer.wait_closed()
        self.assertEqual(self.forwarded,[])
    async def test_duplicate_ready_field_is_rejected(self):
        raw=b'{"type":"native-ready-v1","connection_id":"1","version_id":"115us","ready":true,"ready":true}'
        addr,_=await self.setup_bridge(raw_ready=raw)
        reader,writer=await asyncio.open_connection(*addr)
        self.assertEqual(await asyncio.wait_for(reader.read(20),1),b'')
        writer.close();await writer.wait_closed();self.assertEqual(self.forwarded,[])
    async def test_player_disconnect_cancels_pending_exchange(self):
        addr,bridge=await self.setup_bridge()
        started=asyncio.Event();cancelled=asyncio.Event()
        async def waiting(_):
            started.set()
            try:await asyncio.Future()
            finally:cancelled.set()
        bridge.exchange=waiting
        reader,writer=await asyncio.open_connection(*addr)
        await asyncio.wait_for(started.wait(),1);writer.close();await writer.wait_closed()
        await asyncio.wait_for(cancelled.wait(),1);self.assertEqual(self.forwarded,[])
    async def test_shutdown_closes_connection_and_listener(self):
        addr,bridge=await self.setup_bridge()
        reader,writer=await asyncio.open_connection(*addr);writer.write(b'live');await writer.drain()
        self.assertEqual(await asyncio.wait_for(reader.readexactly(4),2),b'live');await asyncio.wait_for(bridge.close(),3)
        self.assertEqual(await asyncio.wait_for(reader.read(1),1),b'')
        with self.assertRaises(OSError):await asyncio.open_connection(*addr)
        writer.close();await writer.wait_closed()

class PlanTests(unittest.TestCase):
    def test_cloud_enforces_certificate_and_hostname_checks(self):
        t=Target('cloud','host.example.invalid',443,'realm','aud','host','issuer')
        ctx=t.tls();self.assertEqual(ctx.verify_mode,ssl.CERT_REQUIRED);self.assertTrue(ctx.check_hostname)
        self.assertEqual(ctx.minimum_version,ssl.TLSVersion.TLSv1_3)
        with self.assertRaises(Rejected):Target('local','host.example.invalid',1,'r','a','h','i').validate()
    def test_host_client_plan_is_not_local_server_launch(self):
        args=host_client_plan('D:/fixture/dfolauncher.exe','D:/fixture/DFO','D:/fixture/client.log','D:/fixture/breakpoints.txt')
        self.assertEqual(args[1],'--host-client');self.assertNotIn('launch',args)
        self.assertIn('127.0.0.4?7002?probe?',args[-1]);self.assertNotIn('token',' '.join(args))

if __name__=='__main__':unittest.main(verbosity=2)
